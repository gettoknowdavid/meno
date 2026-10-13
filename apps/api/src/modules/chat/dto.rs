//! The chat wire contract (§9.5).
//!
//! # Casing
//!
//! `camelCase`, like `broadcast` and `profile`: `broadcastId`, `isEdited`,
//! `nextCursor`. `master`'s chat responses were the one place in the API that emitted
//! `snake_case` for these fields, which meant a Dart client needed a second naming rule
//! for one screen. Both the HTTP page and the WebSocket frames use the same spelling.
//!
//! # Dates
//!
//! Every timestamp is RFC 3339 with `#[serde(with = "time::serde::rfc3339")]`, so a date
//! cannot become a nine-number tuple by accident; `apps/api/tests/wire_dates.rs`
//! enforces it for this module like every other.
//!
//! # No request bodies
//!
//! The only HTTP endpoint is a read, so there is no `Validatable` here and no field
//! errors to build. Its `?cursor=` is decoded by [`meno_core::Cursor`] and its page size
//! by [`meno_core::CursorParams`], both of which refuse a bad value at the boundary.
//! Everything that *writes* arrives over the socket, and validates against the WS frame
//! types in [`crate::infrastructure::ws::dto`].

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::model::{ChatMessage, ChatReaction, SenderSummary};

/// `GET /broadcasts/{id}/chat/messages` — the page controls, and nothing else.
///
/// The broadcast id is a path parameter, not a query one. `master` took it as a query
/// field *and* documented the route with it in the path, so half the callers sent one
/// and half the other, and the query's copy was what actually selected the rows.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageQuery {
    /// `?cursor=` and `?limit=`, flattened so the keys sit at the top level of the query
    /// string rather than under a `pagination` object.
    #[serde(flatten)]
    pub pagination: meno_core::CursorParams,
}

/// One message, as both the HTTP page and the `newMessage` / `editedMessage` frames
/// carry it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageResponse {
    /// The message id.
    pub id: Uuid,
    /// The broadcast it belongs to.
    pub broadcast_id: Uuid,
    /// The body.
    pub content: String,
    /// Who wrote it.
    pub sender: SenderSummary,
    /// When it was written.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When it was last edited, omitted when it never was.
    #[serde(
        with = "time::serde::rfc3339::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_at: Option<OffsetDateTime>,
    /// Whether it has been edited — `updated_at.is_some()`, sent explicitly so a client
    /// does not have to infer it from a missing key.
    pub is_edited: bool,
}

impl From<ChatMessage> for ChatMessageResponse {
    fn from(message: ChatMessage) -> Self {
        // Read the derived flag before moving the fields out — `is_edited` is computed
        // from `updated_at`, and `sender` is moved into the response on the next line.
        let is_edited = message.is_edited();

        Self {
            id: message.id,
            broadcast_id: message.broadcast_id,
            content: message.content,
            sender: message.sender,
            created_at: message.created_at,
            updated_at: message.updated_at,
            is_edited,
        }
    }
}

/// One reaction, as the `newReaction` frame and the HTTP answer to a reaction carry it.
///
/// `senderId` rather than a full sender summary: a reaction renders as an emoji under a
/// message whose sender the client already has, so the extra join would be payload for
/// nothing.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatReactionResponse {
    /// The reaction id.
    pub id: Uuid,
    /// The broadcast it was sent in.
    pub broadcast_id: Uuid,
    /// Who reacted.
    pub sender_id: Uuid,
    /// The emoji or short label.
    pub content: String,
    /// When it was sent.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl From<ChatReaction> for ChatReactionResponse {
    fn from(reaction: ChatReaction) -> Self {
        Self {
            id: reaction.id,
            broadcast_id: reaction.broadcast_id,
            sender_id: reaction.sender_id,
            content: reaction.content,
            created_at: reaction.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    //! The wire shape, asserted key by key.

    use super::*;
    use crate::modules::chat::model::{EDIT_WINDOW_SECONDS, MESSAGE_MAX_CHARS, REACTION_MAX_CHARS};

    fn message() -> ChatMessage {
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
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000)
                .expect("a plausible fixture instant"),
            updated_at: None,
        }
    }

    #[test]
    fn a_message_uses_camel_case_keys() {
        let json = serde_json::to_value(ChatMessageResponse::from(message()))
            .expect("the response serialises");

        for key in ["broadcastId", "isEdited", "createdAt", "sender"] {
            assert!(json.get(key).is_some(), "missing `{key}` in {json}");
        }
        assert!(
            !json.to_string().contains("broadcast_id"),
            "a snake_case key leaked: {json}"
        );
    }

    #[test]
    fn a_date_is_a_string_not_a_tuple() {
        // §4.5: without `serde(with = "rfc3339")` this is `[year, ordinal, hour, …]`.
        let json = serde_json::to_value(ChatMessageResponse::from(message()))
            .expect("the response serialises");

        assert!(
            json["createdAt"].is_string(),
            "createdAt must be an RFC 3339 string: {json}"
        );
    }

    #[test]
    fn an_unedited_message_omits_updated_at_and_is_edited_is_false() {
        let json = serde_json::to_value(ChatMessageResponse::from(message()))
            .expect("the response serialises");

        assert!(
            json.get("updatedAt").is_none(),
            "`updatedAt` must be absent, not null: {json}"
        );
        assert_eq!(json["isEdited"], serde_json::json!(false));
    }

    #[test]
    fn an_edited_message_carries_updated_at_and_is_edited_is_true() {
        let mut edited = message();
        edited.updated_at = Some(edited.created_at + time::Duration::seconds(10));

        let json = serde_json::to_value(ChatMessageResponse::from(edited))
            .expect("the response serialises");

        assert!(json["updatedAt"].is_string(), "{json}");
        assert_eq!(json["isEdited"], serde_json::json!(true));
    }

    #[test]
    fn an_absent_avatar_is_omitted_rather_than_null() {
        // The client distinguishes "no avatar" from "avatar is the string null"; the
        // contract says omitted.
        let json = serde_json::to_value(ChatMessageResponse::from(message()))
            .expect("the response serialises");

        assert!(json["sender"].get("avatarUrl").is_none(), "{json}");
        assert!(json["sender"].get("bio").is_none(), "{json}");
    }

    #[test]
    fn a_reaction_carries_its_sender_id_and_no_summary() {
        let reaction = ChatReaction {
            id: Uuid::from_u128(1),
            broadcast_id: Uuid::from_u128(2),
            sender_id: Uuid::from_u128(3),
            content: "👏".to_owned(),
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000)
                .expect("a plausible fixture instant"),
        };

        let json = serde_json::to_value(ChatReactionResponse::from(reaction)).expect("serialises");

        assert_eq!(json["senderId"], Uuid::from_u128(3).to_string());
        assert_eq!(json["content"], "👏");
        assert!(json.get("sender").is_none(), "{json}");
    }

    #[test]
    fn the_query_reads_cursor_and_limit_at_the_top_level() {
        // `#[serde(flatten)]`: without it a client would have to send
        // `?pagination[cursor]=…`, which nothing does.
        //
        // `limit` is a *string* because `CursorParams` is `DisplayFromStr` — it is shared
        // with every other paginated endpoint, and it exists to be decoded out of a query
        // string, where every value is text. A JSON number here is refused, which is the
        // rule this test pins.
        let query: ChatMessageQuery =
            serde_json::from_str(r#"{"cursor":"abc","limit":"5"}"#).expect("a valid query");

        assert_eq!(query.pagination.limit, Some(5));
        assert_eq!(
            query.pagination.cursor.as_ref().map(|c| c.0.as_str()),
            Some("abc")
        );

        assert!(
            serde_json::from_str::<ChatMessageQuery>(r#"{"limit":5}"#).is_err(),
            "the wire form is the query string's, so a bare number is not accepted"
        );
    }

    #[test]
    fn an_absent_query_is_a_first_page_of_the_default_size() {
        // `?limit=` and no query at all must both work: `CursorParams` implements
        // `Default`, and a chat client that just opened a room sends nothing.
        let query = ChatMessageQuery::default();

        assert!(query.pagination.cursor.is_none());
        assert_eq!(query.pagination.limit(), 20);
        assert_eq!(query.pagination.limit_plus_one(), 21);
    }

    #[test]
    fn the_limits_mirror_the_columns_they_protect() {
        // The migration's `VARCHAR(256)` and `VARCHAR(32)` are the reason these are what
        // they are; if a migration widens a column this test is what notices.
        assert_eq!(MESSAGE_MAX_CHARS, 256);
        assert_eq!(REACTION_MAX_CHARS, 32);
        assert_eq!(EDIT_WINDOW_SECONDS, 900);
    }
}
