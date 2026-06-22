use serde::{Deserialize, Serialize};
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, FromRow, Postgres, Type};
use std::fmt::Display;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Broadcast {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: BroadcastStatus,
    pub creator_id: Uuid,
    pub time_zone: Option<String>,
    pub image_url: Option<String>,
    pub image_id: Option<String>,
    pub broadcast_token: Option<String>,
    pub total_participants: i64,
    pub start_time: Option<OffsetDateTime>,
    pub end_time: Option<OffsetDateTime>,
    pub recording_enabled: bool,
    pub recording_key: Option<String>,
    pub recording_url: Option<String>,
    pub published_at: Option<OffsetDateTime>,
    pub end_reason: EndReason,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}
impl Broadcast {
    pub fn can_be_scheduled(&self) -> bool {
        match self.start_time {
            Some(st) => st > OffsetDateTime::now_utc(),
            None => false,
        }
    }
    pub fn get_partial_state(&self) -> BroadcastState {
        match self.status {
            BroadcastStatus::Active => BroadcastState::Live,
            BroadcastStatus::Inactive => {
                if self.end_time.is_some() {
                    BroadcastState::Ended
                } else if self
                    .start_time
                    .is_some_and(|st| st > OffsetDateTime::now_utc())
                {
                    BroadcastState::Scheduled
                } else {
                    BroadcastState::Draft
                }
            }
        }
    }
    pub fn is_active(&self) -> bool {
        self.status == BroadcastStatus::Active
    }
    pub fn is_not_active(&self) -> bool {
        self.status != BroadcastStatus::Active
    }
}

/// A broadcast row joined with its creator + cohosts in one query.
/// Used by the `broadcast_to_response()` helper to avoid N+1 fetches.
#[derive(Debug, Clone, FromRow)]
pub struct BroadcastWithRelations {
    #[sqlx(flatten)]
    pub broadcast: Broadcast,
    pub creator_id_: Uuid,
    pub creator_full_name: String,
    pub creator_avatar_id: Option<String>,
    pub creator_avatar_url: Option<String>,
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
    pub removed_at: Option<OffsetDateTime>,
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

/// Context passed into `broadcast_to_response()` from the service layer.
/// Bundles all the participant-specific and Redis-sourced data that cannot be
/// derived from the DB row alone.
#[derive(Debug, Default)]
pub struct BroadcastContext {
    /// The authenticated participant's ID, if present.
    pub participant_id: Option<Uuid>,

    /// Whether the Redis host_grace key exists (broadcast is reconnecting).
    pub is_reconnecting: bool,

    /// Live participant count from Redis (0 if not live).
    pub live_count: i64,

    /// All-time participant count from the DB (total joins ever).
    pub total_count: i64,

    /// The participant's role in this broadcast.
    pub participant_role: ParticipantRole,

    /// Whether the participant is currently joined as a participant.
    pub participant_is_in_room: bool,

    /// Whether the participant subscribes to the creator.
    pub is_subscribed_to_creator: bool,

    /// Whether the participant has bookmarked this broadcast.
    pub is_bookmarked: bool,

    /// Populated only in the /continue-listening context.
    pub time_remaining_seconds: Option<i64>,

    /// When the participant last joined this broadcast.
    pub last_listened_at: Option<OffsetDateTime>,
}

// ==================== ENUMS ====================
// #[derive(Clone, Debug, Serialize, Deserialize, Type, strum::Display, AsRefStr, EnumString)]
#[derive(Debug, Clone, PartialEq, Type, Serialize, Deserialize)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BroadcastStatus {
    Active,
    Inactive,
}
impl From<String> for BroadcastStatus {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "active" => BroadcastStatus::Active,
            _ => BroadcastStatus::Inactive,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Type, Serialize, Deserialize)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ParticipantRole {
    Host,
    Cohost,
    Participant,
    #[default]
    None,
}
impl ParticipantRole {
    pub fn priority(&self) -> i64 {
        match self {
            ParticipantRole::Host => 0,
            ParticipantRole::Cohost => 1,
            ParticipantRole::Participant => 2,
            ParticipantRole::None => 3,
        }
    }
}
impl From<String> for ParticipantRole {
    fn from(s: String) -> Self {
        match s.as_str() {
            "host" => ParticipantRole::Host,
            "cohost" => ParticipantRole::Cohost,
            "participant" => ParticipantRole::Participant,
            _ => ParticipantRole::None,
        }
    }
}
impl From<ParticipantRole> for String {
    fn from(value: ParticipantRole) -> Self {
        match value {
            ParticipantRole::Host => "host".to_string(),
            ParticipantRole::Cohost => "cohost".to_string(),
            ParticipantRole::Participant => "participant".to_string(),
            ParticipantRole::None => "none".to_string(),
        }
    }
}
impl Display for ParticipantRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            ParticipantRole::Host => "host".to_string(),
            ParticipantRole::Cohost => "cohost".to_string(),
            ParticipantRole::Participant => "participant".to_string(),
            ParticipantRole::None => "none".to_string(),
        };
        write!(f, "{}", str)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationStatus {
    Pending,
    Accepted,
    Declined,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Normal,
    HostDisconnected,
    AdminForced,
    QuotaExceeded,
    #[default]
    None,
}

impl From<String> for EndReason {
    fn from(s: String) -> Self {
        match s.as_str() {
            "normal" => EndReason::Normal,
            "host_disconnected" => EndReason::HostDisconnected,
            "admin_forced" => EndReason::AdminForced,
            "quota_exceeded" => EndReason::QuotaExceeded,
            _ => EndReason::Normal,
        }
    }
}
impl Display for EndReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match &self {
            EndReason::Normal => "normal".to_string(),
            EndReason::HostDisconnected => "host_disconnected".to_string(),
            EndReason::AdminForced => "admin_forced".to_string(),
            EndReason::QuotaExceeded => "quota_exceeded".to_string(),
            EndReason::None => "none".to_string(),
        };
        write!(f, "{}", str)
    }
}
impl From<EndReason> for String {
    fn from(value: EndReason) -> Self {
        match value {
            EndReason::Normal => "normal".to_string(),
            EndReason::HostDisconnected => "host_disconnected".to_string(),
            EndReason::AdminForced => "admin_forced".to_string(),
            EndReason::QuotaExceeded => "quota_exceeded".to_string(),
            EndReason::None => "none".to_string(),
        }
    }
}
impl Type<Postgres> for EndReason {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <String as Type<Postgres>>::type_info()
    }
}
impl<'r> Decode<'r, Postgres> for EndReason {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let s: String = Decode::<Postgres>::decode(value)?;
        Ok(EndReason::from(s))
    }
}
impl<'q> Encode<'q, Postgres> for EndReason {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, BoxDynError> {
        let s = String::from(self.clone());
        <String as Encode<Postgres>>::encode(s, buf)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastState {
    /// status=active, no Redis grace key
    Live,

    /// status=active, Redis host_grace:{id} key exists
    Reconnecting,

    /// status=inactive, end_time is set
    Ended,

    /// status=inactive, start_time is in the future
    Scheduled,

    /// status=inactive, no start_time, no end_time
    Draft,
}
