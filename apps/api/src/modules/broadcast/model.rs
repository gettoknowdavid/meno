use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Broadcast {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub status: BroadcastStatus,
    pub creator_id: Uuid,
    pub time_zone: String,
    pub image_url: Option<String>,
    pub image_id: Option<String>,
    pub broadcast_token: Option<String>,
    pub start_time: Option<OffsetDateTime>,
    pub end_time: Option<OffsetDateTime>,
    pub recording_enabled: bool,
    pub recording_key: Option<String>,
    pub recording_url: Option<String>,
    pub published_at: Option<OffsetDateTime>,
    pub end_reason: Option<String>,
    pub is_draft: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, FromRow)]
pub struct BroadcastParticipant {
    pub broadcast_id: Uuid,
    pub participant_id: Uuid,
    pub role: ParticipantRole,
    pub joined_at: OffsetDateTime,
    pub left_at: Option<OffsetDateTime>,
    pub last_listen_position_seconds: i32,
    pub last_listened_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, FromRow)]
pub struct BroadcastCohost {
    pub broadcast_id: Uuid,
    pub cohost_id: Uuid,
    pub invited_by: Uuid,
    pub invited_at: OffsetDateTime,
    pub removed_at: OffsetDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct CohostInvitation {
    pub id: Uuid,
    pub broadcast_id: Uuid,
    pub inviter_id: Uuid,
    pub invitee_id: Uuid,
    pub status: InvitationStatus,
    pub created_at: OffsetDateTime,
    pub responded_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, FromRow)]
pub struct BroadcastBookmark {
    pub user_id: Uuid,
    pub broadcast_id: Uuid,
    pub saved_at: OffsetDateTime,
}

// ==================== ENUMS ====================
// #[derive(Clone, Debug, Serialize, Deserialize, Type, strum::Display, AsRefStr, EnumString)]
#[derive(Debug, Clone, PartialEq, Type, Serialize, Deserialize)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "snake_case")]
pub enum BroadcastStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Type, Serialize, Deserialize)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    Host,
    Cohost,
    Participant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationStatus {
    Pending,
    Accepted,
    Declined,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Normal,
    HostDisconnected,
    AdminForced,
    QuotaExceeded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastState {
    Live,
    Reconnecting,
    Ended,
    Scheduled,
    Draft,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ViewerRole {
    Host,
    Cohost,
    Listener,
    None,
}
