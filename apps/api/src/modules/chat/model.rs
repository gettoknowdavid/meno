//! The chat module's rows, and the sender projection a message carries.
//!
//! # What changed from `master`
//!
//! - **One message type, not two.** `master` had a `ChatMessage` row *and* a flat
//!   `ChatMessageRow` joined with the sender's columns, then converted between them in
//!   the DTO layer. Here [`ChatMessage`] is the joined shape — it is the only shape any
//!   caller ever wants, since every chat query either returns a message to a client or
//!   needs its sender — and the Postgres adapter maps the flat row into it privately
//!   (`repository::pg`), so the intermediate type is not part of this module's surface.
//! - **The edit window is a method that takes `now`.** `master`'s `can_be_edited`
//!   called `OffsetDateTime::now_utc()` inside the check, which made the rule
//!   untestable without waiting a quarter of an hour. [`ChatMessage::editable_at`] takes
//!   the instant, so the boundary is asserted in a test instead of assumed.
//! - **`is_edited` is derived, not stored.** `updated_at.is_some()` is the fact;
//!   `master` carried a redundant field that could disagree with it.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// How long after it was sent a message may still be edited, in seconds.
///
/// Fifteen minutes, unchanged from `master`'s
/// `shared::constants::CHAT_MESSAGE_EDIT_WINDOW_SECONDS`. It lives here rather than in
/// `infrastructure::constants` because exactly one module obeys it — a constant shared
/// by one caller is a constant that has been filed in the wrong place.
pub const EDIT_WINDOW_SECONDS: i64 = 900;

/// The largest message this API accepts, in characters.
///
/// Not a policy choice: `chat_messages.content` is `VARCHAR(256)`, so a longer body
/// would be a truncation error from the driver rather than a validation message a
/// client can act on. Stated once, next to the column it mirrors.
pub const MESSAGE_MAX_CHARS: usize = 256;

/// The largest reaction this API accepts, in characters.
///
/// `chat_reactions.content` is `VARCHAR(32)`: enough for an emoji (four code points) and
/// a short label, short enough that a reaction cannot become a message.
pub const REACTION_MAX_CHARS: usize = 32;

/// A message as the chat module reads it: the row, plus who sent it.
///
/// One type for both the read and the write path, so a message the client just sent
/// looks exactly like the same message on the next page fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// Primary key.
    pub id: Uuid,
    /// The broadcast the message belongs to.
    pub broadcast_id: Uuid,
    /// Who wrote it.
    pub sender: SenderSummary,
    /// The body, 1–[`MESSAGE_MAX_CHARS`] characters.
    pub content: String,
    /// When it was written. Never null.
    pub created_at: OffsetDateTime,
    /// When it was last edited, or `None` if it never was.
    ///
    /// The column is maintained by migration 0010's `set_updated_at` trigger, so the
    /// value here is the database's answer and not the application's.
    pub updated_at: Option<OffsetDateTime>,
}

impl ChatMessage {
    /// Whether this message has been edited.
    #[must_use]
    pub const fn is_edited(&self) -> bool {
        self.updated_at.is_some()
    }

    /// Whether its author may still edit it at `now`.
    ///
    /// Takes `now` rather than reading the clock, so the boundary is a test's input:
    /// `editable_at(created_at + 900s)` is true and one second later is false, asserted
    /// without a sleep.
    ///
    /// A *future* `created_at` is not editable. It cannot happen through this module —
    /// the column defaults to `now()` — but a clock skew between the application and the
    /// database would otherwise make the window arbitrarily wide.
    #[must_use]
    pub fn editable_at(&self, now: OffsetDateTime) -> bool {
        if self.created_at > now {
            return false;
        }
        (now - self.created_at).whole_seconds() <= EDIT_WINDOW_SECONDS
    }
}

/// A reaction as the chat module reads it.
///
/// Reactions are append-only and never edited or deleted (§2's tree), so there is no
/// `updated_at` and no window.
///
/// `FromRow`-able directly, unlike [`ChatMessage`]: a reaction carries its sender's id
/// and nothing else, so there is no join to flatten.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ChatReaction {
    /// Primary key.
    pub id: Uuid,
    /// The broadcast the reaction was sent in.
    pub broadcast_id: Uuid,
    /// Who reacted.
    pub sender_id: Uuid,
    /// The emoji or short label, 1–[`REACTION_MAX_CHARS`] characters.
    pub content: String,
    /// When it was sent.
    pub created_at: OffsetDateTime,
}

/// Who wrote a message, as the wire carries them.
///
/// This module's own projection rather than `broadcast::model::UserSummary`, because the
/// layer rule in [`crate::modules`] does not let one module reach sideways into another
/// — and because chat needs `bio`, which a broadcast's creator summary does not carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SenderSummary {
    /// Account id.
    pub id: Uuid,
    /// Display name.
    pub full_name: String,
    /// Optional biography. Omitted from the wire rather than sent as `null`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bio: Option<String>,
    /// Avatar storage key, when one was uploaded.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub avatar_id: Option<String>,
    /// Resolved avatar URL, when the deployment publishes one.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub avatar_url: Option<String>,
}

/// The fields a new `chat_messages` row needs.
///
/// Borrowed content rather than an owned `String`: the caller has it until the insert
/// returns, and a message body is up to 256 bytes that there is no reason to copy.
#[derive(Debug, Clone, Copy)]
pub struct NewMessage<'a> {
    /// The broadcast to post in.
    pub broadcast_id: Uuid,
    /// Who is posting.
    pub sender_id: Uuid,
    /// The body, already validated by the boundary.
    pub content: &'a str,
}

/// The fields a new `chat_reactions` row needs.
#[derive(Debug, Clone, Copy)]
pub struct NewReaction<'a> {
    /// The broadcast to react in.
    pub broadcast_id: Uuid,
    /// Who is reacting.
    pub sender_id: Uuid,
    /// The reaction, already validated by the boundary.
    pub content: &'a str,
}

#[cfg(test)]
mod tests {
    //! The edit-window rule, at its boundary.

    use super::*;

    fn message(created_at: OffsetDateTime) -> ChatMessage {
        ChatMessage {
            id: Uuid::from_u128(1),
            broadcast_id: Uuid::from_u128(2),
            sender: SenderSummary {
                id: Uuid::from_u128(3),
                full_name: "Ada Lovelace".to_owned(),
                bio: None,
                avatar_id: None,
                avatar_url: None,
            },
            content: "hello".to_owned(),
            created_at,
            updated_at: None,
        }
    }

    fn at(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("a plausible fixture instant")
            + time::Duration::seconds(seconds)
    }

    #[test]
    fn a_message_is_editable_inside_the_window_and_not_after_it() {
        let sent = at(0);
        let message = message(sent);

        assert!(
            message.editable_at(sent + time::Duration::seconds(EDIT_WINDOW_SECONDS)),
            "the last second of the window is inside it"
        );
        assert!(
            !message.editable_at(sent + time::Duration::seconds(EDIT_WINDOW_SECONDS + 1)),
            "one second past the window is outside it"
        );
    }

    #[test]
    fn a_message_is_editable_the_moment_it_is_sent() {
        let sent = at(0);
        assert!(message(sent).editable_at(sent));
    }

    #[test]
    fn a_message_from_the_future_is_never_editable() {
        // Not reachable through this module, but a clock skew between the application
        // and the database must not widen the window without bound.
        let sent = at(60);
        assert!(!message(sent).editable_at(at(0)));
    }

    #[test]
    fn is_edited_is_derived_from_updated_at() {
        let mut message = message(at(0));
        assert!(!message.is_edited());

        message.updated_at = Some(at(10));
        assert!(message.is_edited());
    }
}
