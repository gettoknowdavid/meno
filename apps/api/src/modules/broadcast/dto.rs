use crate::modules::auth;
use crate::modules::broadcast::model::{
    BroadcastState, BroadcastStatus, EndReason, ParticipantRole,
};
use serde::{Deserialize, Deserializer, Serialize, de};
use sqlx::FromRow;
use time::format_description::well_known::Rfc3339;
use time::serde::rfc3339;
use time::{Date, OffsetDateTime, PrimitiveDateTime, Time};
use uuid::Uuid;
use validator::Validate;

/// Maximum number of cohosts per broadcast. Defined here so it can be referenced
/// in both the validator attribute and the error message without duplication.
pub const MAX_COHOSTS: usize = 3;

// ==================== REQUESTS ====================
#[derive(Debug, Deserialize, Validate)]
pub struct CreateBroadcastRequest {
    #[validate(length(
        min = 3,
        max = 100,
        message = "Title must be between 3 and 100 characters"
    ))]
    pub title: String,

    #[validate(length(max = 244, message = "Description cannot exceed 244 characters"))]
    pub description: Option<String>,

    pub image_id: Option<String>,
    pub image_url: Option<String>,

    pub time_zone: Option<String>,

    pub start_time: Option<OffsetDateTime>,

    pub recording_enabled: Option<bool>,

    pub cohosts: Option<Vec<Uuid>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateBroadcastRequest {
    #[validate(length(
        min = 3,
        max = 100,
        message = "Title must be between 3 and 100 characters"
    ))]
    pub title: Option<String>,

    #[validate(length(max = 244, message = "Description cannot exceed 244 characters"))]
    pub description: Option<String>,

    pub image_id: Option<String>,
    pub image_url: Option<String>,

    pub start_time: Option<OffsetDateTime>,

    pub time_zone: Option<String>,

    pub recording_enabled: Option<bool>,

    pub cohosts: Option<Vec<Uuid>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct AddCohostRequest {
    pub cohost: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct RemoveCohostRequest {
    pub remove_from_room: Option<bool>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct AddCohostsRequest {
    pub cohosts: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct AcceptCohostInvitationRequest {
    pub invitation_id: Uuid,
}

#[derive(Debug, Deserialize, Validate)]
pub struct BroadcastTokenRefreshRequest {
    pub broadcast_id: Uuid,
}

// ==================== RESPONSES ====================
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BroadcastResponse {
    // Identity
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub time_zone: Option<String>,
    pub image_url: Option<String>,
    pub image_id: Option<String>,

    // Timestamps
    #[serde(with = "rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "rfc3339::option")]
    pub start_time: Option<OffsetDateTime>,
    #[serde(with = "rfc3339::option")]
    pub end_time: Option<OffsetDateTime>,
    #[serde(with = "rfc3339::option")]
    pub published_at: Option<OffsetDateTime>,
    pub duration_seconds: Option<i64>,

    // State signals (FE switches on these)
    pub status: BroadcastStatus,
    pub state: BroadcastState,
    pub participant_role: ParticipantRole,
    pub is_subscribed_to_creator: bool,
    pub is_bookmarked: bool,

    // Counts
    pub live_participants_count: i64,
    pub total_participants: i64,

    // Recording
    pub recording_enabled: bool,
    pub recording_url: Option<String>,
    pub end_reason: EndReason,

    // Continue listening context
    pub time_remaining_seconds: Option<i64>,
    #[serde(with = "rfc3339::option")]
    pub last_listened_at: Option<OffsetDateTime>,

    // Relations (conditionally populated)
    pub creator: UserSummary,
    pub cohosts: Vec<UserSummary>,
}

/// Returned by go-live and join endpoints. The only place a LiveKit token
/// is ever sent to the client.
#[derive(Clone, Debug, Serialize)]
pub struct BroadcastSessionResponse {
    pub broadcast: BroadcastResponse,

    /// Short-lived LiveKit JWT. TTL configured in constants (default 6 h).
    pub token: String,
}

/// Cohost-specific response after accepting an invitation while a broadcast
/// is already live. Carries the token needed to join the LiveKit room.
#[derive(Clone, Debug, Serialize)]
pub struct CohostSessionResponse {
    pub user: UserSummary,

    /// `None` if the broadcast is not currently live (cohost added pre-broadcast).
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct LeaveBroadcastResponse {
    pub success: bool,
    pub broadcast_id: Uuid,
    pub user_id: Uuid,
    pub left_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
pub struct BroadcastRefreshTokenResponse {
    pub broadcast_id: Uuid,
    pub token: String,
    pub expires_at: OffsetDateTime,
}

/// Compact user shape embedded inside broadcast responses.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserSummary {
    pub id: Uuid,
    pub full_name: String,
    pub avatar_id: Option<String>,
    pub avatar_url: Option<String>,
}
impl From<auth::model::User> for UserSummary {
    fn from(u: auth::model::User) -> Self {
        UserSummary {
            id: u.id,
            full_name: u.full_name,
            avatar_id: u.avatar_id,
            avatar_url: u.avatar_url,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
/// Payload data sent through websocket when a broadcast ends.
pub struct BroadcastEndedPayload {
    pub broadcast_id: Uuid,
    pub reason: EndReason,
}
impl BroadcastEndedPayload {
    pub fn normal_for(broadcast_id: Uuid) -> Self {
        Self {
            broadcast_id,
            reason: EndReason::Normal,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct EndBroadcastResponse {
    pub broadcast_id: Uuid,
    pub broadcast_title: String,
    pub broadcast_image_url: Option<String>,
    pub creator_id: Uuid,
    pub ended_reason: EndReason,
    #[serde(with = "rfc3339")]
    pub ended_at: OffsetDateTime,
    pub duration_secs: i64,
    pub total_participants: i64,
    pub recording_enabled: bool,
    pub recording_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BroadcastListItem {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub time_zone: Option<String>,
    pub image_url: Option<String>,
    pub image_id: Option<String>,

    pub status: BroadcastStatus,

    #[serde(with = "rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "rfc3339::option")]
    pub start_time: Option<OffsetDateTime>,
    #[serde(with = "rfc3339::option")]
    pub end_time: Option<OffsetDateTime>,

    pub total_participants: i64,

    pub creator_id: Uuid,
    pub creator_name: String,
    pub creator_avatar_url: Option<String>,
    pub creator_avatar_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
pub struct ParticipantListItem {
    pub id: Uuid,
    pub full_name: String,
    pub avatar_id: Option<String>,
    pub avatar_url: Option<String>,
    pub role: ParticipantRole,
    #[serde(with = "rfc3339")]
    pub joined_at: OffsetDateTime,
}

// ==================== ENUMS ====================
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastSortBy {
    #[default]
    Title,
    StartTime,
    EndTime,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantSortBy {
    #[default]
    Role,
    JoinedAt,
    Name,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrderBy {
    Asc,
    #[default]
    Desc,
}

// ==================== PARAMS ====================
#[derive(Debug, Default, Deserialize)]
pub struct BroadcastParams {
    pub id: Option<Uuid>,
    pub creator_id: Option<Uuid>,
    pub exclude_creator_id: Option<Uuid>,
    pub status: Option<BroadcastStatus>,
    pub keywords: Option<String>,
    pub only_subscriptions: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    pub start_time_gt: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    pub start_time_lt: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    pub start_time_gte: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    pub start_time_lte: Option<OffsetDateTime>,

    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    pub end_time_gt: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    pub end_time_lt: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    pub end_time_gte: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "deserialize_flexible_datetime")]
    pub end_time_lte: Option<OffsetDateTime>,

    pub start_time_exists: Option<bool>,
    pub end_time_exists: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_sort_by")]
    pub sort_by: Option<BroadcastSortBy>,

    #[serde(default, deserialize_with = "deserialize_order_by")]
    pub order_by: Option<OrderBy>,

    /// The current page number. Min 1
    pub page: Option<i64>,

    /// Results per page. Min 1, max 100. Defaults to 20.
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ParticipantParams {
    /// The current page number. Min 1
    pub page: Option<i64>,

    /// Results per page. Min 1, max 100. Defaults to 20.
    pub limit: Option<i64>,

    pub keywords: Option<String>,

    #[serde(default, deserialize_with = "deserialize_participant_role")]
    pub role: Option<ParticipantRole>,

    #[serde(default, deserialize_with = "deserialize_sort_by_participant")]
    pub sort_by: Option<ParticipantSortBy>,

    #[serde(default, deserialize_with = "deserialize_order_by")]
    pub order_by: Option<OrderBy>,
}

// ==================== PARAM CACHE KEYS ====================
/// Generates a compact, deterministic Redis key from `BroadcastParams`.
///
/// Design goals:
///   1. Deterministic — same logical query always maps to the same key.
///   2. Compact — short strings keep Redis memory low at scale.
///   3. Namespace-prefixed — easy to scan / invalidate by pattern.
///   4. Collision-safe — every param that changes the result set is included.
///
/// Format:
///   `bl:{segment1}:{segment2}:...`
///   where each segment is `{field_abbrev}={value}` for non-default values only.
///   Segments are always emitted in a fixed alphabetical order so that two
///   `BroadcastParams` with the same logical meaning but different field
///   insertion order produce identical keys.
///
/// Page + limit are included because they change which rows are returned.
pub struct BroadcastListCacheKey;
impl BroadcastListCacheKey {
    /// Returns `None` for param combinations that should never be cached
    /// (currently: viewer-specific `only_subscriptions=true` queries, because
    /// the result set differs per user and caching would require per-user keys
    /// which defeat the purpose of a shared broadcast-list cache).
    pub fn build(params: &BroadcastParams) -> Option<String> {
        // Don't cache personalised feeds — they'd need a per-user key
        if params.only_subscriptions == Some(true) {
            return None;
        }

        let mut parts: Vec<String> = Vec::with_capacity(16);

        // Fixed emission order — alphabetical by abbreviation.
        if let Some(id) = params.id {
            parts.push(format!("bi={}", id));
        }
        if let Some(cid) = params.creator_id {
            parts.push(format!("ci={}", cid));
        }
        if let Some(xcid) = params.exclude_creator_id {
            parts.push(format!("xci={}", xcid));
        }
        if let Some(ref kw) = params.keywords {
            // Hash the keyword to keep key short and avoid special-char issues.
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            kw.hash(&mut h);
            parts.push(format!("kw={:x}", h.finish()));
        }
        if let Some(ref s) = params.status {
            parts.push(format!("s={:?}", s));
        }

        // Time range filters — encode presence + value compactly
        macro_rules! push_time {
            ($field:expr, $abbrev:expr) => {
                if let Some(t) = $field {
                    parts.push(format!("{}={}", $abbrev, t.unix_timestamp()));
                }
            };
        }
        push_time!(params.start_time_gt, "stgt");
        push_time!(params.start_time_gte, "stge");
        push_time!(params.start_time_lt, "stlt");
        push_time!(params.start_time_lte, "stle");
        push_time!(params.end_time_gt, "etgt");
        push_time!(params.end_time_gte, "etge");
        push_time!(params.end_time_lt, "etlt");
        push_time!(params.end_time_lte, "etle");

        if let Some(e) = params.start_time_exists {
            parts.push(format!("ste={}", e as u8));
        }
        if let Some(e) = params.end_time_exists {
            parts.push(format!("ete={}", e as u8));
        }

        // Sort / order — only when non-default
        match params.sort_by {
            None | Some(BroadcastSortBy::StartTime) => {} // default, omit
            Some(ref s) => parts.push(format!("sb={:?}", s)),
        }
        match params.order_by {
            None | Some(OrderBy::Desc) => {} // default, omit
            Some(ref o) => parts.push(format!("ob={:?}", o)),
        }

        // Pagination — always include; different pages = different results.
        let page = params.page.unwrap_or(1).max(1);
        let limit = params.limit.unwrap_or(20).clamp(1, 100);
        parts.push(format!("pg={}", page));
        parts.push(format!("lm={}", limit));

        let key = if parts.is_empty() {
            "bl:all".to_string()
        } else {
            format!("bl:{}", parts.join(":"))
        };

        Some(key)
    }
}

pub struct ParticipantListCacheKey;
impl ParticipantListCacheKey {
    pub fn build(broadcast_id: Uuid, params: &ParticipantParams) -> Option<String> {
        let mut parts = vec![format!("pid:{}", broadcast_id)];

        if let Some(ref kw) = params.keywords {
            // Hash the keyword to keep key short and avoid special-char issues.
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            kw.hash(&mut h);
            parts.push(format!("kw={:x}", h.finish()));
        }

        if let Some(ref r) = params.role {
            parts.push(format!("s={:?}", r));
        }

        match params.sort_by {
            None | Some(ParticipantSortBy::Role) => {} // default, omit
            Some(ref s) => parts.push(format!("sb={:?}", s)),
        }

        match params.order_by {
            None | Some(OrderBy::Desc) => {} // default, omit
            Some(ref o) => parts.push(format!("ob={:?}", o)),
        }

        let page = params.page.unwrap_or(1).max(1);
        let limit = params.limit.unwrap_or(20).clamp(1, 100);
        parts.push(format!("pg={}", page));
        parts.push(format!("lm={}", limit));

        Some(format!("pl:{}", parts.join(":")))
    }
}

// ==================== DESERIALIZERS ====================
/// Flexible timestamp deserializer that accepts:
///   - Full RFC 3339: "2026-05-26T21:55:37Z"
///   - Full RFC 3339 with offset: "2026-05-26T21:55:37+01:00"
///   - Date only: "2026-05-26"  (treated as midnight UTC)
///   - Unix timestamp (i64): 1716768000
pub fn deserialize_flexible_datetime<'de, D>(de: D) -> Result<Option<OffsetDateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    struct FlexibleVisitor;

    impl<'de> de::Visitor<'de> for FlexibleVisitor {
        type Value = Option<OffsetDateTime>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a date string (YYYY-MM-DD), RFC 3339 datetime, or Unix timestamp")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            OffsetDateTime::from_unix_timestamp(v)
                .map(Some)
                .map_err(|_| E::custom(format!("Unix timestamp {} is out of range", v)))
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            self.visit_i64(v as i64)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            // Try RFC 3339 first (most common)
            if let Ok(dt) = OffsetDateTime::parse(v, &Rfc3339) {
                return Ok(Some(dt));
            }

            // Try date-only (YYYY-MM-DD) → midnight UTC
            if let Ok(date) = Date::parse(
                v,
                &time::format_description::parse("[year]-[month]-[day]").unwrap(),
            ) {
                let dt = PrimitiveDateTime::new(date, Time::MIDNIGHT).assume_utc();
                return Ok(Some(dt));
            }

            // Try common ISO 8601 without timezone: "2026-05-26T21:55:37"
            if let Ok(pdt) = PrimitiveDateTime::parse(
                v,
                &time::format_description::parse("[year]-[month]-[day]T[hour]:[minute]:[second]")
                    .unwrap(),
            ) {
                return Ok(Some(pdt.assume_utc()));
            }

            Err(E::custom(format!(
                "invalid datetime '{}': expected YYYY-MM-DD, RFC 3339, or Unix timestamp",
                v
            )))
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    de.deserialize_any(FlexibleVisitor)
}

/// Case-insensitive deserializer for BroadcastSortBy.
/// Accepts "title", "TITLE", "Title", "start_time", "StartTime", etc.
/// Returns a structured BroadcastError rather than a raw serde string.
fn deserialize_sort_by<'de, D>(de: D) -> Result<Option<BroadcastSortBy>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(de)?;
    match s.as_deref().map(str::to_lowercase).as_deref() {
        None | Some("") => Ok(None),
        Some("title") => Ok(Some(BroadcastSortBy::Title)),
        Some("start_time") => Ok(Some(BroadcastSortBy::StartTime)),
        Some("end_time") => Ok(Some(BroadcastSortBy::EndTime)),
        Some(other) => Err(de::Error::custom(format!(
            "unknown sort_by '{}': expected one of title, start_time, end_time",
            other
        ))),
    }
}

/// Order by deserializer
fn deserialize_order_by<'de, D>(de: D) -> Result<Option<OrderBy>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(de)?;
    match s.as_deref().map(str::to_lowercase).as_deref() {
        None | Some("") => Ok(None),
        Some("asc") => Ok(Some(OrderBy::Asc)),
        Some("desc") => Ok(Some(OrderBy::Desc)),
        Some(other) => Err(de::Error::custom(format!(
            "unknown order_by '{}': expected 'asc' or 'desc'",
            other
        ))),
    }
}

/// ParticipantRole deserializer — same pattern
fn deserialize_participant_role<'de, D>(de: D) -> Result<Option<ParticipantRole>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(de)?;
    match s.as_deref().map(str::to_lowercase).as_deref() {
        None | Some("") => Ok(None),
        Some("host") => Ok(Some(ParticipantRole::Host)),
        Some("cohost") => Ok(Some(ParticipantRole::Cohost)),
        Some("participant") => Ok(Some(ParticipantRole::Participant)),
        Some(other) => Err(de::Error::custom(format!(
            "unknown role '{}': expected host, cohost, or participant",
            other
        ))),
    }
}

fn deserialize_sort_by_participant<'de, D>(de: D) -> Result<Option<ParticipantSortBy>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(de)?;
    match s.as_deref().map(str::to_lowercase).as_deref() {
        None | Some("") => Ok(None),
        Some("role") => Ok(Some(ParticipantSortBy::Role)),
        Some("joined_at") => Ok(Some(ParticipantSortBy::JoinedAt)),
        Some("name") => Ok(Some(ParticipantSortBy::Name)),
        Some(other) => Err(de::Error::custom(format!(
            "unknown sort_by '{}': expected role, joined_at, or name",
            other
        ))),
    }
}
