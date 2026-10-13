//! The chat rules, as orchestration (§5.1's `service` row).
//!
//! # Validation is not here, on purpose
//!
//! §9.5 puts the boundary in charge of shape, and the boundary is where the frames are
//! parsed (`routes::ws` against [`crate::infrastructure::ws::dto`]). What *is* here are
//! the rules that need state: is this broadcast live, is the caller in the room, is the
//! edit window still open, does this message belong to the broadcast the caller named.
//! Those cannot be decided from a payload.
//!
//! # The rule that is checked before the write, and the one that is inside it
//!
//! "Is this broadcast live, and is the caller in the room" is enforced *by the insert
//! statement* ([`ChatRepo::insert_message`]), so there is no window in which a broadcast
//! ends between the check and the write. Re-reading it in this file would be a second
//! home for a rule that already has one, and the second home is the one that goes stale.
//!
//! The edit window is the opposite case: it needs the row's `created_at`, so the service
//! has already loaded the row and the check belongs with the other decisions that need it.
//!
//! # Every mutation announces itself
//!
//! A stored message that nobody hears about is a message only its sender can see, so
//! each mutating method publishes through [`ChatPublisher`] after the write succeeds —
//! and, just as deliberately, publishes *nothing* when a gate refused. The negative half
//! is asserted in `tests/chat_service.rs`.

use std::sync::Arc;

use time::OffsetDateTime;
use uuid::Uuid;

use super::dto::{ChatMessageResponse, ChatReactionResponse};
use super::error;
use super::model::{NewMessage, NewReaction};
use super::publisher::ChatPublisher;
use super::repository::{ChatRepo, MessageFilter};
use crate::infrastructure::ws::dto::WsPayload;
use meno_core::{Cursor, CursorPage, CursorParams, Error as MenoError, Order};

/// The broadcast, and the account acting in it.
///
/// One value rather than two `Uuid` parameters, because both are ids and a call site that
/// swapped them would compile. It is also the reason `sender_id` is never deserialised
/// from a frame: the identity is filled in by the transport from the authenticated
/// socket, so a client cannot claim to be somebody else by adding a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatMember {
    /// The broadcast being acted on.
    pub broadcast_id: Uuid,
    /// The authenticated account acting on it.
    pub user_id: Uuid,
}

/// The chat module's rules.
#[derive(Clone)]
pub struct ChatService {
    repo: Arc<dyn ChatRepo>,
    publisher: Arc<dyn ChatPublisher>,
}

impl std::fmt::Debug for ChatService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatService").finish_non_exhaustive()
    }
}

impl ChatService {
    /// Build from its collaborators. `state.rs` supplies the concrete ones.
    #[must_use]
    pub fn new(repo: Arc<dyn ChatRepo>, publisher: Arc<dyn ChatPublisher>) -> Self {
        Self { repo, publisher }
    }

    /// A page of a broadcast's chat history.
    ///
    /// Readable by anybody in the room, live or not: a broadcast's chat is part of what
    /// it produced, and hiding it the moment the host ends the broadcast would throw away
    /// the conversation the listeners just had.
    ///
    /// # Errors
    ///
    /// [`MenoError::Forbidden`] with `NOT_PARTICIPANT` for a caller who is not in the
    /// room; [`MenoError::BadRequest`] with `INVALID_CURSOR` for a `?cursor=` that cannot
    /// be decoded; a repository failure.
    #[tracing::instrument(
        name = "chat.messages",
        skip(self, pagination),
        fields(broadcast_id = %member.broadcast_id, viewer_id = %member.user_id)
    )]
    pub async fn messages(
        &self,
        member: ChatMember,
        pagination: &CursorParams,
    ) -> Result<CursorPage<ChatMessageResponse>, MenoError> {
        if !self
            .repo
            .is_participant(member.broadcast_id, member.user_id)
            .await?
        {
            return Err(error::not_participant());
        }

        let limit = pagination.limit();
        let cursor = match &pagination.cursor {
            None => None,
            // A bad cursor is the client's to fix; `CursorError` converts to
            // `400 INVALID_CURSOR` at the crate boundary, so the detail stays internal.
            Some(cursor) => Some(cursor.to_timestamp_id()?),
        };

        let filter = MessageFilter {
            broadcast_id: member.broadcast_id,
            limit_plus_one: pagination.limit_plus_one(),
            cursor,
            order: Order::Desc,
        };

        let rows = self.repo.list_messages(filter).await?;
        let responses: Vec<ChatMessageResponse> = rows.into_iter().map(Into::into).collect();

        // `from_rows` drops the probe row and advertises a cursor only when another page
        // really exists, so the client never follows a cursor into an empty page.
        Ok(CursorPage::from_rows(responses, limit, |message| {
            Cursor::from_timestamp_id(message.created_at, message.id)
        }))
    }

    /// Store a message and announce it to the room.
    ///
    /// # Errors
    ///
    /// `409 CONFLICT` when the broadcast is not live,
    /// `403 NOT_PARTICIPANT` when the sender is not in the room, a repository failure.
    #[tracing::instrument(
        name = "chat.send_message",
        skip(self, content),
        fields(broadcast_id = %member.broadcast_id, sender_id = %member.user_id)
    )]
    pub async fn send_message(
        &self,
        member: ChatMember,
        content: &str,
    ) -> Result<ChatMessageResponse, MenoError> {
        let inserted = self
            .repo
            .insert_message(NewMessage {
                broadcast_id: member.broadcast_id,
                sender_id: member.user_id,
                content,
            })
            .await?;

        let Some(message) = inserted else {
            return Err(self.refusal_for(member).await);
        };

        let response = ChatMessageResponse::from(message);
        self.publisher
            .publish(member.broadcast_id, WsPayload::new_message(&response))
            .await;

        tracing::info!(
            broadcast_id = %member.broadcast_id,
            message_id = %response.id,
            "chat message sent"
        );

        Ok(response)
    }

    /// Replace the body of a message its author still owns.
    ///
    /// # Errors
    ///
    /// `404 NOT_FOUND` when the message does not exist, is deleted, or belongs to a
    /// different broadcast; `409 CONFLICT` when the broadcast is not live;
    /// `403 FORBIDDEN` when the caller did not write it; `400 BAD_REQUEST` once the edit
    /// window has closed.
    #[tracing::instrument(
        name = "chat.edit_message",
        skip(self, content),
        fields(broadcast_id = %member.broadcast_id, sender_id = %member.user_id, message_id = %message_id)
    )]
    pub async fn edit_message(
        &self,
        member: ChatMember,
        message_id: Uuid,
        content: &str,
    ) -> Result<ChatMessageResponse, MenoError> {
        // Two independent reads, one round trip's worth of latency.
        let (found, live) = tokio::join!(
            self.repo.find_message(message_id),
            self.repo.is_live_broadcast(member.broadcast_id),
        );

        let message = found?.ok_or_else(error::message_not_found)?;
        if !live? {
            return Err(error::broadcast_not_live());
        }

        // `master` never compared the message's broadcast to the one in the request, so a
        // member of one room could edit their own message *in another room's name* — the
        // edit was applied and announced to a room that had never seen the message.
        if message.broadcast_id != member.broadcast_id {
            return Err(error::message_not_found());
        }
        if message.sender.id != member.user_id {
            return Err(error::not_sender());
        }
        if !message.editable_at(OffsetDateTime::now_utc()) {
            return Err(error::edit_window_expired());
        }

        let updated = self
            .repo
            .update_message(message_id, member.user_id, content)
            .await?;
        // Deleted or reassigned between the read above and this write.
        let Some(updated) = updated else {
            return Err(error::message_not_found());
        };

        let response = ChatMessageResponse::from(updated);
        self.publisher
            .publish(member.broadcast_id, WsPayload::edited_message(&response))
            .await;

        tracing::info!(
            broadcast_id = %member.broadcast_id,
            message_id = %message_id,
            "chat message edited"
        );

        Ok(response)
    }

    /// Soft-delete a message its author owns.
    ///
    /// Allowed after the broadcast ends, unlike sending and editing: a host clearing
    /// something they regret should not need the room to be live to do it.
    ///
    /// # Errors
    ///
    /// `404 NOT_FOUND` when there is no such live message or it belongs to another
    /// broadcast; `403 FORBIDDEN` when somebody else wrote it.
    #[tracing::instrument(
        name = "chat.delete_message",
        skip(self),
        fields(broadcast_id = %member.broadcast_id, sender_id = %member.user_id, message_id = %message_id)
    )]
    pub async fn delete_message(
        &self,
        member: ChatMember,
        message_id: Uuid,
    ) -> Result<(), MenoError> {
        let deleted = self
            .repo
            .soft_delete_message(message_id, member.user_id)
            .await?;

        if !deleted {
            // One extra read, only on the refusal path, to say *which* refusal it was:
            // `master` answered `NOT_MESSAGE_SENDER` for every case, including a message
            // that had already been deleted, which is a message the caller cannot act on.
            let Some(message) = self.repo.find_message(message_id).await? else {
                return Err(error::message_not_found());
            };
            if message.broadcast_id != member.broadcast_id {
                return Err(error::message_not_found());
            }
            return Err(error::not_sender());
        }

        self.publisher
            .publish(
                member.broadcast_id,
                WsPayload::deleted_message(member.broadcast_id, message_id),
            )
            .await;

        tracing::info!(
            broadcast_id = %member.broadcast_id,
            message_id = %message_id,
            "chat message deleted"
        );

        Ok(())
    }

    /// Store a reaction and announce it to the room.
    ///
    /// # Errors
    ///
    /// `409 CONFLICT` when the broadcast is not live,
    /// `403 NOT_PARTICIPANT` when the sender is not in the room, a repository failure.
    #[tracing::instrument(
        name = "chat.send_reaction",
        skip(self, content),
        fields(broadcast_id = %member.broadcast_id, sender_id = %member.user_id)
    )]
    pub async fn send_reaction(
        &self,
        member: ChatMember,
        content: &str,
    ) -> Result<ChatReactionResponse, MenoError> {
        let inserted = self
            .repo
            .insert_reaction(NewReaction {
                broadcast_id: member.broadcast_id,
                sender_id: member.user_id,
                content,
            })
            .await?;

        let Some(reaction) = inserted else {
            return Err(self.refusal_for(member).await);
        };

        let response = ChatReactionResponse::from(reaction);
        self.publisher
            .publish(member.broadcast_id, WsPayload::new_reaction(&response))
            .await;

        tracing::info!(
            broadcast_id = %member.broadcast_id,
            reaction_id = %response.id,
            "chat reaction sent"
        );

        Ok(response)
    }

    /// Say *why* a guarded write was refused.
    ///
    /// The guarded statements write nothing for two reasons, and only this module knows
    /// which answer to give — so this is one extra read on a path that is already an
    /// error, rather than a check on the happy path that could go stale between the read
    /// and the write.
    ///
    /// A failed check is reported as itself: the client is told the truth ("we could not
    /// tell") rather than a refusal it might act on wrongly.
    async fn refusal_for(&self, member: ChatMember) -> MenoError {
        match self.repo.is_live_broadcast(member.broadcast_id).await {
            Ok(true) => error::not_participant(),
            Ok(false) => error::broadcast_not_live(),
            Err(failure) => failure,
        }
    }
}
