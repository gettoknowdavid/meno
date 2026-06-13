use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ChatMessage {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub broadcast_id: Uuid,
    pub content: String,
    pub created_at: OffsetDateTime,
    pub updated_at: Option<OffsetDateTime>,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ChatReaction {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub broadcast_id: Uuid,
    pub content: String,
    pub created_at: OffsetDateTime,
}
