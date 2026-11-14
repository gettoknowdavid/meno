//! The in-memory double of [`ChatRepo`] (§10).
//!
//! The point of this file is that it is *not* more permissive than the SQL. The guarded
//! inserts refuse for the same two reasons (`broadcasts.status = 'active'` with no
//! `end_time`, and a `broadcast_participants` row with no `left_at`), a soft delete
//! removes a message from every read exactly as `deleted_at IS NULL` does, and
//! `updated_at` is stamped on update the way the migration's trigger stamps it. A test
//! that passes here describes something the statements really do.
//!
//! # One thing only the double can do
//!
//! [`InMemoryChatRepo::seed_message`] writes a message with a timestamp of the caller's
//! choosing. The SQL cannot be asked to backdate a row, and without it the fifteen-minute
//! edit window could only be tested by waiting fifteen minutes — so the fixture exists on
//! the double and is documented as a fixture, not as behaviour.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

use time::OffsetDateTime;
use uuid::Uuid;

use super::{ChatRepo, MessageFilter};
use crate::modules::chat::error;
use crate::modules::chat::model::{
    ChatMessage, ChatReaction, NewMessage, NewReaction, SenderSummary,
};
use meno_core::{Error as MenoError, Order};

/// Everything the double remembers, behind one lock.
#[derive(Debug, Default)]
struct State {
    /// Accounts a message's sender can be joined to.
    users: HashMap<Uuid, SenderSummary>,
    /// Broadcasts currently live.
    live: HashSet<Uuid>,
    /// `(broadcast, participant)` pairs currently in a room.
    participants: HashSet<(Uuid, Uuid)>,
    /// Every message ever written, deleted ones included — as the table has them.
    messages: Vec<ChatMessage>,
    /// Which of those are soft-deleted.
    deleted: HashSet<Uuid>,
    /// Every reaction, append-only.
    reactions: Vec<ChatReaction>,
}

/// The double the chat suite runs against.
#[derive(Debug, Default)]
pub struct InMemoryChatRepo {
    state: Mutex<State>,
}

impl InMemoryChatRepo {
    /// An empty repository: no accounts, no live broadcasts, no messages.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an account a message can be joined to.
    pub fn add_user(&self, user: SenderSummary) {
        self.lock().users.insert(user.id, user);
    }

    /// Mark a broadcast live (`status = 'active'`, `end_time IS NULL`).
    pub fn start_broadcast(&self, broadcast_id: Uuid) {
        self.lock().live.insert(broadcast_id);
    }

    /// End a broadcast, so the guarded writes start refusing.
    pub fn end_broadcast(&self, broadcast_id: Uuid) {
        self.lock().live.remove(&broadcast_id);
    }

    /// Put an account in a room.
    pub fn add_participant(&self, broadcast_id: Uuid, user_id: Uuid) {
        self.lock().participants.insert((broadcast_id, user_id));
    }

    /// Take an account out of a room (`left_at` set).
    pub fn remove_participant(&self, broadcast_id: Uuid, user_id: Uuid) {
        self.lock().participants.remove(&(broadcast_id, user_id));
    }

    /// Write a message with a timestamp of the caller's choosing, bypassing the guard.
    ///
    /// A fixture, not behaviour: it exists so the edit window can be tested at its
    /// boundary without a sleep, and so pagination can be tested against known instants.
    pub fn seed_message(&self, message: ChatMessage) {
        self.lock().messages.push(message);
    }

    /// A message by id, deleted or not — for assertions that do not go through the trait.
    #[must_use]
    pub fn message(&self, message_id: Uuid) -> Option<ChatMessage> {
        self.lock()
            .messages
            .iter()
            .find(|message| message.id == message_id)
            .cloned()
    }

    /// How many messages the double holds, deleted ones included.
    #[must_use]
    pub fn stored_message_count(&self) -> usize {
        self.lock().messages.len()
    }

    /// Everything that was announced as a reaction, for assertions.
    #[must_use]
    pub fn reactions(&self) -> Vec<ChatReaction> {
        self.lock().reactions.clone()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether the write guard would admit a message from `sender_id` right now.
    fn admits(state: &State, broadcast_id: Uuid, sender_id: Uuid) -> bool {
        state.live.contains(&broadcast_id)
            && state.participants.contains(&(broadcast_id, sender_id))
    }

    /// The sender a message is joined to, or an error naming the missing row.
    ///
    /// The SQL equivalent is the `JOIN users` — which the foreign key guarantees to find
    /// a row for, since `chat_messages.sender_id` references `users`. A missing row here
    /// is therefore the double's own wiring mistake, and it says so loudly rather than
    /// inventing a sender.
    fn sender(state: &State, sender_id: Uuid) -> Result<SenderSummary, MenoError> {
        state.users.get(&sender_id).cloned().ok_or_else(|| {
            error::internal(
                "insert_message",
                "the sender has no user row in the double; call `add_user` first",
            )
        })
    }
}

#[async_trait::async_trait]
impl ChatRepo for InMemoryChatRepo {
    async fn insert_message(&self, new: NewMessage<'_>) -> Result<Option<ChatMessage>, MenoError> {
        let mut state = self.lock();
        if !Self::admits(&state, new.broadcast_id, new.sender_id) {
            return Ok(None);
        }

        let sender = Self::sender(&state, new.sender_id)?;
        let message = ChatMessage {
            id: Uuid::new_v4(),
            broadcast_id: new.broadcast_id,
            sender,
            content: new.content.to_owned(),
            created_at: OffsetDateTime::now_utc(),
            updated_at: None,
        };
        state.messages.push(message.clone());

        Ok(Some(message))
    }

    async fn find_message(&self, message_id: Uuid) -> Result<Option<ChatMessage>, MenoError> {
        let state = self.lock();
        if state.deleted.contains(&message_id) {
            return Ok(None);
        }
        Ok(state
            .messages
            .iter()
            .find(|message| message.id == message_id)
            .cloned())
    }

    async fn update_message(
        &self,
        message_id: Uuid,
        sender_id: Uuid,
        content: &str,
    ) -> Result<Option<ChatMessage>, MenoError> {
        let mut state = self.lock();
        if state.deleted.contains(&message_id) {
            return Ok(None);
        }

        let Some(message) = state
            .messages
            .iter_mut()
            .find(|message| message.id == message_id && message.sender.id == sender_id)
        else {
            return Ok(None);
        };

        message.content = content.to_owned();
        // The migration's `set_updated_at` trigger, reproduced: the UPDATE stamps it.
        message.updated_at = Some(OffsetDateTime::now_utc());

        Ok(Some(message.clone()))
    }

    async fn soft_delete_message(
        &self,
        message_id: Uuid,
        sender_id: Uuid,
    ) -> Result<bool, MenoError> {
        let mut state = self.lock();
        let owned = state
            .messages
            .iter()
            .any(|message| message.id == message_id && message.sender.id == sender_id);

        if !owned {
            return Ok(false);
        }
        // `WHERE deleted_at IS NULL`: a second delete affects no rows.
        Ok(state.deleted.insert(message_id))
    }

    async fn list_messages(&self, filter: MessageFilter) -> Result<Vec<ChatMessage>, MenoError> {
        let state = self.lock();

        let mut rows: Vec<ChatMessage> = state
            .messages
            .iter()
            .filter(|message| message.broadcast_id == filter.broadcast_id)
            .filter(|message| !state.deleted.contains(&message.id))
            .filter(|message| match filter.cursor {
                None => true,
                // The same row-value comparison the SQL emits: timestamp and id move
                // together, so a timestamp that is not unique cannot drop a row.
                Some((ts, id)) => match filter.order {
                    Order::Desc => (message.created_at, message.id) < (ts, id),
                    Order::Asc => (message.created_at, message.id) > (ts, id),
                },
            })
            .cloned()
            .collect();

        // The same ordering the SQL's `ORDER BY created_at, id` produces: the tuple
        // keeps the two columns together, so a timestamp that is not unique cannot
        // shuffle rows between pages.
        rows.sort_by_key(|message| (message.created_at, message.id));
        if filter.order == Order::Desc {
            rows.reverse();
        }
        rows.truncate(usize::try_from(filter.limit_plus_one).unwrap_or(usize::MAX));

        Ok(rows)
    }

    async fn insert_reaction(
        &self,
        new: NewReaction<'_>,
    ) -> Result<Option<ChatReaction>, MenoError> {
        let mut state = self.lock();
        if !Self::admits(&state, new.broadcast_id, new.sender_id) {
            return Ok(None);
        }

        let reaction = ChatReaction {
            id: Uuid::new_v4(),
            broadcast_id: new.broadcast_id,
            sender_id: new.sender_id,
            content: new.content.to_owned(),
            created_at: OffsetDateTime::now_utc(),
        };
        state.reactions.push(reaction.clone());

        Ok(Some(reaction))
    }

    async fn is_live_broadcast(&self, broadcast_id: Uuid) -> Result<bool, MenoError> {
        Ok(self.lock().live.contains(&broadcast_id))
    }

    async fn is_participant(&self, broadcast_id: Uuid, user_id: Uuid) -> Result<bool, MenoError> {
        Ok(self.lock().participants.contains(&(broadcast_id, user_id)))
    }
}
