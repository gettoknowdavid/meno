use crate::modules::chat::cache::ChatCache;
use crate::modules::chat::dto::{ChatMessageQuery, ChatMessageResponse, ChatReactionResponse};
use crate::modules::chat::errors::ChatError;
use crate::modules::chat::repository::ChatRepository;
use crate::shared::constants::{TTL_10_SECS, TTL_60_SECS};
use crate::shared::pagination::{Cursor, CursorPage};
use crate::shared::services::redis::RedisService;
use crate::shared::services::redis::coalescing::coalesce_cache;
use crate::shared::services::ws::WsService;
use crate::shared::services::ws::dto::WsPayload;
use crate::state::MenoState;
use uuid::Uuid;

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
        skip(self, app),
        fields(broadcast_id = %broadcast_id, sender_id = %sender_id)
    )]
    pub async fn send_message(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        sender_id: Uuid,
        content: &str,
    ) -> Result<ChatMessageResponse, ChatError> {
        let (is_active_broadcast_result, is_participant_result) = tokio::join!(
            self.repo.is_active_broadcast(broadcast_id),
            self.repo.is_broadcast_participant(broadcast_id, sender_id)
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
            .create_message(broadcast_id, sender_id, content)
            .await?;

        self.cache.invalidate_chat(broadcast_id).await;

        let response = ChatMessageResponse::from(row);

        let response_clone = response.clone();
        let broadcast_service = app.broadcast.service.clone();
        let ws = self.ws.clone();
        tokio::spawn(async move {
            let payload = WsPayload::new_message(response_clone);
            if let Ok(ids) = broadcast_service.get_participants_ids(broadcast_id).await {
                ws.send_to_users(&ids, payload).await;
            }
        });

        tracing::info!(
            broadcast_id = %broadcast_id,
            message_id   = %response.id,
            "Chat message sent"
        );

        Ok(response)
    }

    #[tracing::instrument(
        name = "chat.get_messages",
        skip(self, query),
        fields(broadcast_id = %broadcast_id)
    )]
    pub async fn get_messages(
        &self,
        broadcast_id: Uuid,
        query: &ChatMessageQuery,
    ) -> Result<CursorPage<ChatMessageResponse>, ChatError> {
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
        skip(self, app),
        fields(message_id = %message_id, sender_id = %sender_id)
    )]
    pub async fn edit_message(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        message_id: Uuid,
        sender_id: Uuid,
        content: &str,
    ) -> Result<ChatMessageResponse, ChatError> {
        let (message_result, is_active_broadcast_result) = tokio::join!(
            self.repo.find_message_by_id(message_id),
            self.repo.is_active_broadcast(broadcast_id),
        );

        let message = message_result?.ok_or(ChatError::NotFound)?;

        if !is_active_broadcast_result? {
            return Err(ChatError::BroadcastNotLive);
        }
        if sender_id != message.sender_id {
            return Err(ChatError::NotSender);
        }
        if !message.can_be_edited() {
            return Err(ChatError::EditWindowExpired);
        }

        let row = self
            .repo
            .update_message(message_id, sender_id, content)
            .await?
            .ok_or(ChatError::NotFound)?;

        let response = ChatMessageResponse::from(row);

        self.cache.invalidate_chat(broadcast_id).await;

        let response_clone = response.clone();
        let broadcast_service = app.broadcast.service.clone();
        let ws = self.ws.clone();
        tokio::spawn(async move {
            let payload = WsPayload::edited_message(response_clone);
            if let Ok(ids) = broadcast_service.get_participants_ids(broadcast_id).await {
                ws.send_to_users(&ids, payload).await;
            }
        });

        tracing::info!(message_id = %message_id, "Chat message edited");

        Ok(response)
    }

    #[tracing::instrument(
        name = "chat.delete_message",
        skip(self, app),
        fields(broadcast_id = %broadcast_id, message_id = %message_id, sender_id = %sender_id)
    )]
    pub async fn delete_message(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        message_id: Uuid,
        sender_id: Uuid,
    ) -> Result<(), ChatError> {
        let deleted = self.repo.soft_delete_message(message_id, sender_id).await?;
        if !deleted {
            return Err(ChatError::NotSender);
        }

        self.cache.invalidate_chat(broadcast_id).await;

        let broadcast_service = app.broadcast.service.clone();
        let ws = self.ws.clone();
        tokio::spawn(async move {
            let payload = WsPayload::deleted_message(broadcast_id, message_id);
            if let Ok(ids) = broadcast_service.get_participants_ids(broadcast_id).await {
                ws.send_to_users(&ids, payload).await
            }
        });

        tracing::info!(message_id = %message_id, "Chat message deleted");
        Ok(())
    }

    #[tracing::instrument(
        name   = "chat.send_reaction",
        skip   (self, app),
        fields (broadcast_id = %broadcast_id, sender_id = %sender_id)
    )]
    pub async fn send_reaction(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        sender_id: Uuid,
        content: &str,
    ) -> Result<ChatReactionResponse, ChatError> {
        let (is_active_broadcast, is_participant_result) = tokio::join!(
            self.repo.is_active_broadcast(broadcast_id),
            self.repo.is_broadcast_participant(broadcast_id, sender_id),
        );

        if !is_active_broadcast? {
            return Err(ChatError::BroadcastNotLive);
        }
        if !is_participant_result? {
            return Err(ChatError::NotParticipant);
        }

        let row = self
            .repo
            .create_reaction(broadcast_id, sender_id, content)
            .await?;
        let response = ChatReactionResponse::from(row);

        let broadcast_service = app.broadcast.service.clone();
        let ws = self.ws.clone();
        let response_clone = response.clone();
        tokio::spawn(async move {
            let payload = WsPayload::new_reaction(&response_clone);
            if let Ok(ids) = broadcast_service.get_participants_ids(broadcast_id).await {
                ws.send_to_users(&ids, payload).await
            }
        });

        tracing::info!(
            broadcast_id = %broadcast_id,
            reaction_id  = %response.id,
            "Chat reaction sent"
        );

        Ok(response)
    }
}
