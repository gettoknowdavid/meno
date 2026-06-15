use crate::shared::constants::CHAT_MESSAGE_EDIT_WINDOW_SECONDS;
use crate::shared::pagination::CursorParams;
use crate::shared::types::dto::UserSummary;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::{OffsetDateTime, serde::rfc3339};
use uuid::Uuid;
use validator::Validate;

// #################### REQUESTS ####################
#[derive(Debug, Deserialize, Validate)]
pub struct SendMessageRequest {
    #[validate(length(min = 1, max = 256, message = "Message must be 1–256 characters"))]
    pub content: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct EditMessageRequest {
    #[validate(length(min = 1, max = 256, message = "Message must be 1–256 characters"))]
    pub content: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SendReactionRequest {
    #[validate(length(min = 1, max = 32, message = "Reaction must be 1–32 characters"))]
    pub content: String,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct ChatMessageQuery {
    #[serde(flatten)]
    pub pagination: CursorParams,
}
impl ChatMessageQuery {
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }
    pub fn cursor(&self) -> Option<&crate::shared::pagination::Cursor> {
        self.pagination.cursor.as_ref()
    }
}

// #################### RESPONSES ####################

/// Full message shape returned to clients and sent over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatMessageResponse {
    pub id: Uuid,
    pub content: String,
    pub broadcast_id: Uuid,
    pub sender: UserSummary,
    #[serde(with = "rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
    pub is_edited: bool,
}
impl From<ChatMessageRow> for ChatMessageResponse {
    fn from(row: ChatMessageRow) -> Self {
        Self {
            id: row.id,
            content: row.content,
            broadcast_id: row.broadcast_id,
            sender: UserSummary {
                id: row.sender_id,
                full_name: row.sender_name,
                bio: row.sender_bio,
                avatar_id: row.sender_avatar_id,
                avatar_url: row.sender_avatar_url,
            },
            created_at: row.created_at,
            updated_at: row.updated_at,
            is_edited: row.updated_at.is_some(),
        }
    }
}
/// Reaction shape — sent over WS and returned from the POST endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatReactionResponse {
    pub id: Uuid,
    pub content: String,
    pub broadcast_id: Uuid,
    pub sender_id: Uuid,
    #[serde(with = "rfc3339")]
    pub created_at: OffsetDateTime,
}
impl From<ChatReactionRow> for ChatReactionResponse {
    fn from(v: ChatReactionRow) -> Self {
        Self {
            id: v.id,
            content: v.content,
            broadcast_id: v.broadcast_id,
            sender_id: v.sender_id,
            created_at: v.created_at,
        }
    }
}

/// Flat DB row joined with sender info — used internally by the repo.
#[derive(Debug, FromRow)]
pub struct ChatMessageRow {
    pub id: Uuid,
    pub content: String,
    pub broadcast_id: Uuid,
    pub created_at: OffsetDateTime,
    pub updated_at: Option<OffsetDateTime>,
    pub deleted_at: Option<OffsetDateTime>,
    pub sender_id: Uuid,
    pub sender_name: String,
    pub sender_bio: Option<String>,
    pub sender_avatar_id: Option<String>,
    pub sender_avatar_url: Option<String>,
}
impl ChatMessageRow {
    pub fn can_be_edited(&self) -> bool {
        let now = OffsetDateTime::now_utc();

        if self.created_at > now {
            return false;
        }

        let age = (now - self.created_at).whole_seconds();
        age <= CHAT_MESSAGE_EDIT_WINDOW_SECONDS
    }
}

#[derive(Debug, FromRow)]
pub struct ChatReactionRow {
    pub id: Uuid,
    pub content: String,
    pub broadcast_id: Uuid,
    pub created_at: OffsetDateTime,
    pub sender_id: Uuid,
}
