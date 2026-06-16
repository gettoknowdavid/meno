use crate::modules::chat::cache::ChatCache;
use crate::modules::chat::dto::{
    ChatMessageQuery, ChatMessageResponse, ChatReactionResponse, DeleteMessageRequest,
    EditMessageRequest, SendMessageRequest, SendReactionRequest,
};
use crate::modules::chat::errors::ChatError;
use crate::modules::chat::repository::ChatRepository;
use crate::shared::constants::{TTL_10_SECS, TTL_60_SECS};
use crate::shared::pagination::{Cursor, CursorPage};
use crate::shared::services::redis::RedisService;
use crate::shared::services::redis::coalescing::coalesce_cache;
use crate::shared::services::ws::WsService;
use crate::shared::services::ws::dto::WsPayload;
use crate::state::MenoState;

#[derive(Clone)]
pub struct ChatService {
    repo: ChatRepository,
    redis: RedisService,
    cache: ChatCache,
    ws: WsService,
}
impl ChatService {
    pub fn new(db: sqlx::PgPool, redis: RedisService, ws: WsService) -> Self {
        Self {
            repo: ChatRepository::new(db),
            cache: ChatCache::new(redis.clone()),
            redis,
            ws,
        }
    }

    #[tracing::instrument(
        name = "chat.send_message",
        skip(self, app, req),
        fields(broadcast_id = %req.broadcast_id, sender_id = %req.sender_id)
    )]
    pub async fn send_message(
        &self,
        app: &MenoState,
        req: &SendMessageRequest,
    ) -> Result<ChatMessageResponse, ChatError> {
        let (is_active_broadcast_result, is_participant_result) = tokio::join!(
            self.repo.is_active_broadcast(req.broadcast_id),
            self.repo
                .is_broadcast_participant(req.broadcast_id, req.sender_id)
        );

        let is_active_broadcast = is_active_broadcast_result?;
        let is_participant = is_participant_result?;

        if !is_active_broadcast {
            return Err(ChatError::BroadcastNotLive);
        }
        if !is_participant {
            return Err(ChatError::NotParticipant);
        }

        let row = self
            .repo
            .create_message(req.broadcast_id, req.sender_id, &req.content)
            .await?;

        self.cache.invalidate_chat(req.broadcast_id).await;

        let response = ChatMessageResponse::from(row);

        let b_id = req.broadcast_id.clone();
        let response_clone = response.clone();
        let broadcast_service = app.broadcast.service.clone();
        let ws = self.ws.clone();
        tokio::spawn(async move {
            let payload = WsPayload::new_message(response_clone);
            if let Ok(ids) = broadcast_service.get_participants_ids(b_id).await {
                ws.send_to_users(&ids, payload).await;
            }
        });

        tracing::info!(
            broadcast_id = %req.broadcast_id,
            message_id   = %response.id,
            "Chat message sent"
        );

        Ok(response)
    }

    #[tracing::instrument(
        name = "chat.get_messages",
        skip(self, query),
        fields(broadcast_id = %query.broadcast_id)
    )]
    pub async fn get_messages(
        &self,
        query: &ChatMessageQuery,
    ) -> Result<CursorPage<ChatMessageResponse>, ChatError> {
        let broadcast_id = query.broadcast_id;

        let limit = query.limit();
        let cursor_str = query.cursor().map(|c| c.0.as_str()).unwrap_or("");
        let cache_key = ChatCache::chat_list_cache_key(broadcast_id, Some(cursor_str), limit);

        let ttl = if self.cache.is_live(broadcast_id).await.unwrap_or(false) {
            TTL_10_SECS
        } else {
            TTL_60_SECS
        };

        coalesce_cache(&self.redis, &cache_key, ttl, || async {
            let rows = self.repo.find_messages(broadcast_id, &query).await?;
            Ok(CursorPage::from_rows(rows, limit, |r| {
                Cursor::from_timestamp_id(r.created_at, r.id)
            }))
        })
        .await
    }

    #[tracing::instrument(
        name = "chat.edit_message",
        skip(self, app, req),
        fields(message_id = %req.message_id, sender_id = %req.sender_id)
    )]
    pub async fn edit_message(
        &self,
        app: &MenoState,
        req: &EditMessageRequest,
    ) -> Result<ChatMessageResponse, ChatError> {
        let (message_result, is_active_broadcast_result) = tokio::join!(
            self.repo.find_message_by_id(req.message_id),
            self.repo.is_active_broadcast(req.broadcast_id),
        );

        let message = message_result?.ok_or(ChatError::NotFound)?;

        if !is_active_broadcast_result? {
            return Err(ChatError::BroadcastNotLive);
        }
        if req.sender_id != message.sender_id {
            return Err(ChatError::NotSender);
        }
        if !message.can_be_edited() {
            return Err(ChatError::EditWindowExpired);
        }

        let row = self
            .repo
            .update_message(req.message_id, req.sender_id, &req.content)
            .await?
            .ok_or(ChatError::NotFound)?;

        let response = ChatMessageResponse::from(row);

        self.cache.invalidate_chat(req.broadcast_id).await;

        let b_id = req.broadcast_id;
        let response_clone = response.clone();
        let broadcast_service = app.broadcast.service.clone();
        let ws = self.ws.clone();
        tokio::spawn(async move {
            let payload = WsPayload::edited_message(response_clone);
            if let Ok(ids) = broadcast_service.get_participants_ids(b_id).await {
                ws.send_to_users(&ids, payload).await;
            }
        });

        tracing::info!(message_id = %req.message_id, "Chat message edited");

        Ok(response)
    }

    #[tracing::instrument(
        name = "chat.delete_message",
        skip(self, app, req),
        fields(broadcast_id = %req.broadcast_id, message_id = %req.message_id, sender_id = %req.sender_id)
    )]
    pub async fn delete_message(
        &self,
        app: &MenoState,
        req: &DeleteMessageRequest,
    ) -> Result<(), ChatError> {
        let deleted = self
            .repo
            .soft_delete_message(req.message_id, req.sender_id)
            .await?;
        if !deleted {
            return Err(ChatError::NotSender);
        }

        self.cache.invalidate_chat(req.broadcast_id).await;

        let b_id = req.broadcast_id;
        let message_id = req.message_id;
        let broadcast_service = app.broadcast.service.clone();
        let ws = self.ws.clone();
        tokio::spawn(async move {
            let payload = WsPayload::deleted_message(b_id, message_id);
            if let Ok(ids) = broadcast_service.get_participants_ids(b_id).await {
                ws.send_to_users(&ids, payload).await
            }
        });

        tracing::info!(message_id = %req.message_id, "Chat message deleted");
        Ok(())
    }

    #[tracing::instrument(
        name   = "chat.send_reaction",
        skip   (self, app),
        fields (broadcast_id = %req.broadcast_id, sender_id = %req.sender_id)
    )]
    pub async fn send_reaction(
        &self,
        app: &MenoState,
        req: &SendReactionRequest,
    ) -> Result<ChatReactionResponse, ChatError> {
        let (is_active_broadcast, is_participant_result) = tokio::join!(
            self.repo.is_active_broadcast(req.broadcast_id),
            self.repo
                .is_broadcast_participant(req.broadcast_id, req.sender_id),
        );

        if !is_active_broadcast? {
            return Err(ChatError::BroadcastNotLive);
        }
        if !is_participant_result? {
            return Err(ChatError::NotParticipant);
        }

        let row = self
            .repo
            .create_reaction(req.broadcast_id, req.sender_id, &req.content)
            .await?;
        let response = ChatReactionResponse::from(row);

        let b_id = req.broadcast_id;
        let broadcast_service = app.broadcast.service.clone();
        let ws = self.ws.clone();
        let response_clone = response.clone();
        tokio::spawn(async move {
            let payload = WsPayload::new_reaction(&response_clone);
            if let Ok(ids) = broadcast_service.get_participants_ids(b_id).await {
                ws.send_to_users(&ids, payload).await
            }
        });

        tracing::info!(
            broadcast_id = %req.broadcast_id,
            reaction_id  = %response.id,
            "Chat reaction sent"
        );

        Ok(response)
    }
}
