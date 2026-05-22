use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Broadcast {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub status: BroadcastStatus,
    pub time_zone: String,
    pub creator_id: Uuid,
    pub image_url: Option<String>,
    pub image_id: Option<String>,
    pub broadcast_token: Option<String>,
    pub start_time: Option<OffsetDateTime>,
    pub end_time: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BroadcastParticipant {
    pub broadcast_id: Uuid,
    pub participant_id: Uuid,
    pub role: ParticipantRole,
    pub joined_at: OffsetDateTime,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BroadcastCohost {
    pub broadcast_id: Uuid,
    pub cohost_id: Uuid,
    pub invited_at: OffsetDateTime,
    pub removed_at: OffsetDateTime,
}

// ==================== ENUMS ====================
#[derive(Clone, Debug, Serialize, Deserialize, strum::Display, AsRefStr, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum BroadcastStatus {
    Active,
    Inactive,
}

#[derive(Clone, Debug, Serialize, Deserialize, strum::Display, AsRefStr, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum ParticipantRole {
    Host,
    Cohost,
    Listener,
}
