use crate::modules::broadcast::model::{
    BroadcastState, BroadcastStatus, EndReason, ParticipantRole,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;
use validator::Validate;

// ==================== REQUESTS ====================
#[derive(Debug, Deserialize, Validate)]
pub struct CreateBroadcastRequest {
    #[validate(length(min = 3, max = 100, message = "Title: min-3, max-100"))]
    pub title: String,

    #[validate(length(max = 244, message = "Description length exceeded (244 max)"))]
    pub description: String,

    pub image_id: Option<String>,
    pub image_url: Option<String>,

    pub time_zone: Option<String>,

    pub start_time: Option<OffsetDateTime>,

    pub recording_enabled: Option<bool>,

    #[validate(length(max = 3, message = "You cannot add more than 3 cohosts"))]
    pub cohosts: Option<Vec<Uuid>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateBroadcastRequest {
    #[validate(length(min = 3, max = 100, message = "Title: min-3, max-100"))]
    pub title: Option<String>,

    #[validate(length(max = 244, message = "Description length exceeded (244 max)"))]
    pub description: Option<String>,

    pub image_id: Option<String>,
    pub image_url: Option<String>,

    pub start_time: Option<OffsetDateTime>,

    pub time_zone: Option<String>,

    pub recording_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct AddCohostsRequest {
    #[validate(length(min = 1, max = 3, message = "You cannot add more than 3 cohosts"))]
    pub cohosts: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct AcceptCohostInvitationRequest {
    pub invitation_id: Uuid,
}

#[derive(Debug, Default, Deserialize)]
pub struct BroadcastParams {
    pub id: Option<Uuid>,
    pub creator_id: Option<Uuid>,
    pub exclude_creator_id: Option<Uuid>,
    pub status: Option<BroadcastStatus>,
    pub keywords: Option<String>,
    pub only_subscriptions: bool,

    pub start_time_gt: Option<OffsetDateTime>,
    pub start_time_lt: Option<OffsetDateTime>,
    pub start_time_gte: Option<OffsetDateTime>,
    pub start_time_lte: Option<OffsetDateTime>,

    pub end_time_gt: Option<OffsetDateTime>,
    pub end_time_lt: Option<OffsetDateTime>,
    pub end_time_gte: Option<OffsetDateTime>,
    pub end_time_lte: Option<OffsetDateTime>,

    pub start_time_exists: Option<bool>,
    pub end_time_exists: Option<bool>,

    pub sort_by: Option<BroadcastSortBy>,
    pub order: Option<BroadcastOrderBy>,

    pub page: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ParticipantParams {
    pub page: i64,

    pub limit: i64,

    /// If set to `true`, this handler will return only the participants
    /// currently live in the broadcast
    pub show_only_live: bool,
}

// ==================== RESPONSES ====================
#[derive(Clone, Debug, Serialize)]
pub struct BroadcastResponse {
    // Identity
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub time_zone: String,
    pub image_url: Option<String>,
    pub image_id: Option<String>,

    // Timestamps
    pub created_at: OffsetDateTime,
    pub start_time: Option<OffsetDateTime>,
    pub end_time: Option<OffsetDateTime>,
    pub published_at: Option<OffsetDateTime>,
    pub duration_seconds: Option<i64>,

    // State signals (FE switches on these)
    pub role: ParticipantRole,
    pub status: BroadcastStatus,
    pub broadcast_state: BroadcastState,
    pub is_subscribed_to_creator: bool,
    pub is_bookmarked: bool,

    // Counts
    pub live_participants_count: i64,
    pub total_participants: i64,

    // Recording
    pub recording_enabled: bool,
    pub recording_url: Option<String>,
    pub end_reason: Option<EndReason>,

    // Continue listening context
    pub time_remaining_seconds: Option<i64>,
    pub last_listened_at: Option<OffsetDateTime>,

    // Relations (conditionally populated)
    pub creator: ParticipantSummary,
    pub cohosts: Option<Vec<ParticipantSummary>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ParticipantSummary {
    pub id: Uuid,
    pub full_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BroadcastSessionResponse {
    /// The full [BroadcastResponse] data transfer object
    pub broadcast: BroadcastResponse,

    /// Livekit Token
    pub token: String,
}

#[derive(Clone, Debug, Serialize)]
/// Cohost-specific session (includes their specific token)
pub struct CohostSessionResponse {
    /// The full [ParticipantSummary] data transfer object
    pub user: ParticipantSummary,

    /// Livekit Token
    pub token: String,
}

#[derive(Clone, Debug, Serialize)]
/// Payload data sent through websocket when a broadcast ends.
pub struct BroadcastEndedPayload {
    pub broadcast_id: Uuid,
    pub reason: EndReason,
}

// ==================== ENUMS ====================
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastConnectionState {
    Live,
    Reconnecting,
    Ended,
}
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastSortBy {
    #[default]
    Title,
    StartTime,
    EndTime,
    TotalParticipants,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastOrderBy {
    Asc,

    #[default]
    Desc,
}
