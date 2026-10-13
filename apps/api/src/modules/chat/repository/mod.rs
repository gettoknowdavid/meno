//! What the chat module needs from Postgres, and nothing else.
//!
//! Same split as the other modules: [`pg`] is Postgres, [`memory`] is the §10 double,
//! and [`ChatRepo`] is the only name the service knows. `state.rs` chooses between them
//! (§5.6), so neither the service nor a handler can reach for `sqlx::PgPool`.
//!
//! # Errors
//!
//! Every method returns [`MenoError`]. A `sqlx::Error` never crosses this boundary: it
//! becomes [`MenoError::Internal`] with the operation named, so driver text reaches the
//! log and not the client (§4.2).
//!
//! # `master`'s repository, and what changed
//!
//! - **It returned DTOs.** `find_messages` mapped rows straight into
//!   `ChatMessageResponse`, so the wire shape was decided in the storage layer and the
//!   service had nothing to say about it. Every method here returns a
//!   [`ChatMessage`](super::model::ChatMessage) — the module's own shape — and
//!   `dto::ChatMessageResponse` is built once, in the service.
//! - **The write guards moved into the statements.** `master` asked
//!   `is_active_broadcast`, then `is_broadcast_participant`, then inserted — three round
//!   trips, and a window between the second and the third in which a broadcast could
//!   end or a participant be removed, with the message landing anyway.
//!   [`ChatRepo::insert_message`] is one statement whose `INSERT … SELECT … WHERE
//!   EXISTS` writes nothing unless both conditions hold *at the moment of the write*.
//! - **`is_broadcast_participant` and `is_active_broadcast` are kept**, but only for the
//!   refusal path: when the guarded insert writes nothing, the service asks which
//!   precondition failed so the client gets `NOT_PARTICIPANT` or the conflict rather
//!   than a shrug. The happy path never calls them.
//! - **The Redis list cache is gone.** `master` cached every page under
//!   `chat:{id}:msgs:…` with a TTL chosen by whether the broadcast looked live, and
//!   invalidated it by `SCAN`-ing for the pattern after every write. That is a
//!   full-keyspace scan per message for a cache whose fill was already asynchronous —
//!   so an invalidation racing a fill could leave a page stale for the whole TTL. The
//!   read it replaced is one keyset query on `idx_chat_messages_cursor_desc`, which §9.4
//!   covers without help; `broadcast`'s catalogue made the same call for the same
//!   reason. Nothing here pretends to cache.

pub mod memory;
pub mod pg;

use async_trait::async_trait;
use meno_core::{Error as MenoError, Order};
use time::OffsetDateTime;
use uuid::Uuid;

#[cfg(any(test, feature = "test-support"))]
pub use memory::InMemoryChatRepo;
pub use pg::PgChatRepo;

use super::model::{ChatMessage, ChatReaction, NewMessage, NewReaction};

/// The chat module's storage.
#[async_trait]
pub trait ChatRepo: Send + Sync + std::fmt::Debug {
    /// Store a message — but only if its sender may post in the broadcast right now.
    ///
    /// Returns `Ok(None)` when the write was refused because the broadcast is not live or
    /// the sender is not in the room. A `None` rather than an error because the two
    /// cases have different answers for the client, and only the service knows which
    /// one to give; see [`super::service::ChatService::send_message`].
    ///
    /// The check and the insert are one statement on purpose: `master`'s read-then-write
    /// could admit a message to a broadcast that ended in between.
    async fn insert_message(&self, new: NewMessage<'_>) -> Result<Option<ChatMessage>, MenoError>;

    /// A live message by id, or `None` when it does not exist or was deleted.
    async fn find_message(&self, message_id: Uuid) -> Result<Option<ChatMessage>, MenoError>;

    /// Replace a message's body, if its author still owns it.
    ///
    /// Guarded on `sender_id` and `deleted_at IS NULL` in the statement rather than by a
    /// read first, for the same reason as [`Self::insert_message`]. The edit *window* is
    /// not checked here: it is a rule of the module, it needs the row's `created_at`,
    /// and the service has already loaded the row by then.
    ///
    /// Returns `None` when nothing matched, so the caller can explain why.
    async fn update_message(
        &self,
        message_id: Uuid,
        sender_id: Uuid,
        content: &str,
    ) -> Result<Option<ChatMessage>, MenoError>;

    /// Soft-delete a message its author owns. `false` when nothing matched.
    async fn soft_delete_message(
        &self,
        message_id: Uuid,
        sender_id: Uuid,
    ) -> Result<bool, MenoError>;

    /// One page of a broadcast's live messages, newest first unless the filter says
    /// otherwise.
    ///
    /// Read with `limit + 1` rows so the caller can tell whether another page exists
    /// without a second `COUNT` query.
    async fn list_messages(&self, filter: MessageFilter) -> Result<Vec<ChatMessage>, MenoError>;

    /// Store a reaction, guarded like [`Self::insert_message`].
    async fn insert_reaction(
        &self,
        new: NewReaction<'_>,
    ) -> Result<Option<ChatReaction>, MenoError>;

    /// Whether the broadcast is live right now: not ended, not deleted.
    ///
    /// Used only to explain a refused write.
    async fn is_live_broadcast(&self, broadcast_id: Uuid) -> Result<bool, MenoError>;

    /// Whether this account is currently in the room.
    ///
    /// Used only to explain a refused write.
    async fn is_participant(&self, broadcast_id: Uuid, user_id: Uuid) -> Result<bool, MenoError>;
}

/// The filter a page of messages is read with, already decoded.
///
/// The cursor arrives as a `(timestamp, id)` pair rather than as the opaque string a
/// client sent, because decoding happens once at the edge: a malformed cursor is then a
/// `400 INVALID_CURSOR` from the boundary and never a `WHERE` clause built from text.
#[derive(Debug, Clone, Copy)]
pub struct MessageFilter {
    /// The broadcast whose messages to read.
    pub broadcast_id: Uuid,
    /// The page size, plus the one probe row.
    pub limit_plus_one: i64,
    /// The exclusive keyset boundary from the previous page.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
    /// Sort direction. `Desc` is newest-first, which is what a chat client scrolls.
    pub order: Order,
}

impl MessageFilter {
    /// A first page of `limit + 1` messages for `broadcast_id`, newest first.
    #[must_use]
    pub fn first_page(broadcast_id: Uuid, limit_plus_one: i64) -> Self {
        Self {
            broadcast_id,
            limit_plus_one,
            cursor: None,
            order: Order::Desc,
        }
    }
}
