//! Request and response bodies for the broadcast endpoints (plan §9.5).
//!
//! # Validate at the boundary, in code
//!
//! Every request type has a `validate` method and every handler calls it as its first
//! statement, so a handler can assume its arguments are usable. This mirrors
//! [`crate::modules::auth::dto`] deliberately: `master` expressed the same intent with
//! `validator`'s derive attributes, which put the field name in a string and produced a
//! second error shape (§4.2's `data` is a `HashMap<String, Vec<String>>`, which is not
//! what `ValidationErrors` serialises to). Hand-written `validate` returns the one shape
//! the envelope carries, and a renamed field is a compile error instead of a silent
//! mismatch.
//!
//! # What is *not* validated here
//!
//! Anything that needs a database or a clock. Whether a title is unique, whether the
//! broadcast is live, whether the caller is the creator — all of that is the service's
//! question. This file answers only "is this shape acceptable".
//!
//! # Dates on the wire
//!
//! Every field typed as a date carries `#[serde(with = "time::serde::rfc3339")]` — on
//! an `Option<T>` it is `time::serde::rfc3339::option`, which maps `None` to `null`.
//! `time`'s own `Serialize` emits a nine-number tuple instead, which no client can read;
//! `apps/api/tests/wire_dates.rs` fails the suite if one appears here. The reasoning is
//! in [`crate::modules::auth::dto`], which wrote it first.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::error::FieldErrors;
use super::model::{
    Broadcast, BroadcastCohost, BroadcastState, BroadcastStatus, EndReason, ParticipantRole,
    UserSummary, now,
};
use meno_core::Error as MenoError;

/// The most cohosts one broadcast may have.
///
/// In `dto` rather than in the service so that the *validator* and the error message
/// quote the same number — the two drifting apart is how a client ends up sending four
/// cohosts and getting a message about a limit it was never told.
pub const MAX_COHOSTS: usize = 3;

/// The shortest acceptable title, in characters.
const TITLE_MIN: usize = 3;

/// The longest acceptable title, in characters.
const TITLE_MAX: usize = 100;

/// The longest acceptable description, in characters.
///
/// Matches the column's `VARCHAR(244)`, so a description that passes validation cannot
/// be refused by the database with a 500.
const DESCRIPTION_MAX: usize = 244;

/// A request that can check itself before a handler looks at it.
pub trait Validatable {
    /// Run every rule, accumulating rather than short-circuiting.
    ///
    /// # Errors
    ///
    /// [`MenoError::Validation`] listing every field that failed and why. Returning on
    /// the first failure would make a client with two mistakes need two round trips.
    fn validate(&self) -> Result<(), MenoError>;
}

/// `POST /broadcasts`.
#[derive(Debug, Deserialize)]
pub struct CreateBroadcastRequest {
    /// What the broadcast is called.
    pub title: String,
    /// Optional description.
    pub description: Option<String>,
    /// Storage key of a cover image.
    pub image_id: Option<String>,
    /// Resolved cover image URL.
    pub image_url: Option<String>,
    /// IANA zone, e.g. `Africa/Lagos`.
    pub time_zone: Option<String>,
    /// When it should go live. Must be in the future.
    #[serde(with = "time::serde::rfc3339::option", default)]
    pub start_time: Option<OffsetDateTime>,
    /// Whether a recording may be kept.
    pub recording_enabled: Option<bool>,
    /// Cohosts to invite at creation.
    pub cohosts: Option<Vec<Uuid>>,
}

impl Validatable for CreateBroadcastRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        validate_title(&self.title, &mut fields);
        validate_description(self.description.as_deref(), &mut fields);
        validate_cohosts(self.cohosts.as_deref(), &mut fields);

        // The clock rule is a *shape* rule here and an existence rule in the service:
        // "in the past" needs no database, so it is refused before the handler looks at
        // anything. The service re-checks it, because a request validated at `t` and
        // handled at `t + 1s` has moved.
        if self.start_time.is_some_and(|start| start <= now()) {
            fields.push("startTime", "startTime must be in the future");
        }

        fields.into_result()
    }
}

/// `PATCH /broadcasts/{id}`.
///
/// Every field optional, because a patch that requires `title` is a lie about what the
/// verb means.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateBroadcastRequest {
    /// New title.
    pub title: Option<String>,
    /// New description.
    pub description: Option<String>,
    /// New cover image key.
    pub image_id: Option<String>,
    /// New resolved cover image URL.
    pub image_url: Option<String>,
    /// New scheduled start.
    #[serde(with = "time::serde::rfc3339::option", default)]
    pub start_time: Option<OffsetDateTime>,
    /// New IANA zone.
    pub time_zone: Option<String>,
    /// New recording preference.
    pub recording_enabled: Option<bool>,
    /// The complete cohost list. Absent means "leave the cohosts alone".
    pub cohosts: Option<Vec<Uuid>>,
}

impl Validatable for UpdateBroadcastRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        if let Some(title) = &self.title {
            validate_title(title, &mut fields);
        }
        validate_description(self.description.as_deref(), &mut fields);
        validate_cohosts(self.cohosts.as_deref(), &mut fields);

        if self.start_time.is_some_and(|start| start <= now()) {
            fields.push("startTime", "startTime must be in the future");
        }

        // A patch that changes nothing is a 422 rather than a round trip that reports
        // success for having done nothing: the client sent a request with no intent, and
        // saying so is more useful than a 200.
        if self.is_empty() && fields.is_empty() {
            fields.push("body", "A patch must change at least one field");
        }

        fields.into_result()
    }
}

impl UpdateBroadcastRequest {
    /// Whether the body carries no field at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.image_id.is_none()
            && self.image_url.is_none()
            && self.start_time.is_none()
            && self.time_zone.is_none()
            && self.recording_enabled.is_none()
            && self.cohosts.is_none()
    }
}

/// `POST /broadcasts/{id}/cohosts`.
#[derive(Debug, Deserialize)]
pub struct AddCohostRequest {
    /// Who to add.
    pub cohost: Uuid,
}

/// `DELETE /broadcasts/{id}/cohosts/{cohostId}`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveCohostRequest {
    /// Whether to also evict them from the live room.
    ///
    /// Default `true` when the body is absent: leaving someone in a room they no longer
    /// have rights in is the failure this flag exists to prevent, so the safe reading is
    /// the default one.
    #[serde(default = "default_true")]
    pub remove_from_room: bool,
}

/// The default for [`RemoveCohostRequest::remove_from_room`].
const fn default_true() -> bool {
    true
}

/// `GET /broadcasts` — the filter set, already decoded.
///
/// Deliberately small. `master` carried thirteen filters (eight time-range bounds, a
/// keyword hash, three sort keys, two booleans) across a query struct that flattened
/// pagination into itself, which made the *sort* of the feed depend on a field the
/// cursor had to encode. This keeps what a keyset cursor can carry — `creator_id`,
/// `status` and a direction — and [`Order`] decides everything else.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastQuery {
    /// Only this creator's broadcasts.
    pub creator_id: Option<Uuid>,
    /// Only broadcasts in this status.
    pub status: Option<BroadcastStatus>,
    /// Sort direction. Defaults to newest first.
    pub order: Option<meno_core::Order>,
    /// The embedded `?cursor=` / `?limit=`.
    #[serde(flatten)]
    pub pagination: meno_core::CursorParams,
}

impl BroadcastQuery {
    /// Validated page size, clamped to [1, 100].
    #[must_use]
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }

    /// `limit + 1` — the probe row that says whether another page exists.
    #[must_use]
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }

    /// The opaque cursor from the previous page.
    #[must_use]
    pub fn cursor(&self) -> Option<&meno_core::Cursor> {
        self.pagination.cursor.as_ref()
    }

    /// The direction, defaulted to newest-first.
    #[must_use]
    pub fn effective_order(&self) -> meno_core::Order {
        self.order.unwrap_or_default()
    }
}

/// `GET /broadcasts/{id}/participants`.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantQuery {
    /// Only participants holding this role.
    pub role: Option<ParticipantRole>,
    /// Sort direction. Defaults to earliest join first, which is the order a host
    /// moderating a room wants.
    pub order: Option<meno_core::Order>,
    /// The embedded `?cursor=` / `?limit=`.
    #[serde(flatten)]
    pub pagination: meno_core::CursorParams,
}

impl ParticipantQuery {
    /// Validated page size, clamped to [1, 100].
    #[must_use]
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }

    /// `limit + 1`.
    #[must_use]
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }

    /// The opaque cursor from the previous page.
    #[must_use]
    pub fn cursor(&self) -> Option<&meno_core::Cursor> {
        self.pagination.cursor.as_ref()
    }

    /// The direction, defaulted to earliest-first.
    #[must_use]
    pub fn effective_order(&self) -> meno_core::Order {
        // Ascending by default, and this is the one place the default is inverted
        // relative to the rest of the API: a participant list is read as a roster, and
        // "who arrived first" is the question, not "who arrived most recently".
        match self.order.unwrap_or_default() {
            meno_core::Order::Desc => meno_core::Order::Asc,
            asc => asc,
        }
    }
}

/// A broadcast, as the detail endpoint returns it.
///
/// One shape for one purpose: a client renders a card from this and a sheet from this,
/// and two shapes would mean the card could show a field the sheet does not.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastResponse {
    /// The broadcast id.
    pub id: Uuid,
    /// What it is called.
    pub title: String,
    /// Optional description.
    pub description: Option<String>,
    /// The creator's IANA zone.
    pub time_zone: Option<String>,
    /// Resolved cover image URL.
    pub image_url: Option<String>,
    /// Cover image storage key.
    pub image_id: Option<String>,

    /// When it was created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When it is scheduled to start.
    #[serde(with = "time::serde::rfc3339::option")]
    pub start_time: Option<OffsetDateTime>,
    /// When it ended.
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_time: Option<OffsetDateTime>,
    /// When it went live.
    #[serde(with = "time::serde::rfc3339::option")]
    pub published_at: Option<OffsetDateTime>,
    /// How long it ran, once it has.
    pub duration_seconds: Option<i64>,

    /// The database column.
    pub status: BroadcastStatus,
    /// What a client renders.
    pub state: BroadcastState,
    /// What the caller may do here.
    pub participant_role: ParticipantRole,
    /// Whether the caller is currently in the room.
    pub is_joined: bool,
    /// Whether the caller has bookmarked it.
    pub is_bookmarked: bool,

    /// How many people are in the room right now.
    pub live_participants_count: i64,
    /// How many have ever joined.
    pub total_participants: i64,

    /// Whether a recording may be kept.
    pub recording_enabled: bool,
    /// Resolved recording URL, once one exists.
    pub recording_url: Option<String>,
    /// Why it ended. `none` while it has not.
    pub end_reason: EndReason,

    /// Where the caller left off, for "continue listening".
    pub time_remaining_seconds: Option<i64>,

    /// Who owns it.
    pub creator: UserSummary,
    /// The current cohosts.
    pub cohosts: Vec<UserSummary>,
}

impl BroadcastResponse {
    /// Assemble a response from a row and the facts only the service can know.
    ///
    /// # Errors
    ///
    /// [`super::error::user_not_found`] when the caller asked for a participant's role
    /// and that user is not a live account. It cannot be defaulted to
    /// [`ParticipantRole::None`]: that would render "you are not in this broadcast" for
    /// a caller who *is*.
    pub fn new(
        broadcast: &Broadcast,
        creator: UserSummary,
        cohosts: Vec<UserSummary>,
        viewer: Option<&Viewer>,
    ) -> Result<Self, MenoError> {
        let duration_seconds = match (broadcast.published_at, broadcast.end_time) {
            (Some(start), Some(end)) => Some(end.unix_timestamp() - start.unix_timestamp()),
            _ => None,
        };

        let live_participants_count = viewer.map_or(0, |v| v.live_participants_count);
        let total_participants = broadcast.total_participants;
        let role = viewer.map_or(ParticipantRole::None, |v| v.role);

        Ok(Self {
            id: broadcast.id,
            title: broadcast.title.clone(),
            description: broadcast.description.clone(),
            time_zone: broadcast.time_zone.clone(),
            image_url: broadcast.image_url.clone(),
            image_id: broadcast.image_id.clone(),
            created_at: broadcast.created_at,
            start_time: broadcast.start_time,
            end_time: broadcast.end_time,
            published_at: broadcast.published_at,
            duration_seconds,
            status: broadcast.status,
            state: broadcast.state(),
            is_joined: viewer.is_some_and(|v| v.is_joined),
            participant_role: role,
            is_bookmarked: viewer.is_some_and(|v| v.is_bookmarked),
            live_participants_count,
            total_participants,
            recording_enabled: broadcast.recording_enabled,
            recording_url: broadcast.recording_url.clone(),
            end_reason: broadcast.end_reason.unwrap_or_default(),
            time_remaining_seconds: viewer.and_then(|v| v.time_remaining_seconds),
            creator,
            cohosts,
        })
    }
}

/// What only the service knows about the *caller's* view of a broadcast.
///
/// Not optional and not defaulted: each field is a fact that costs a query, and a caller
/// who is not in the room has none of them. `Option<&Viewer>` says exactly that, so a
/// response for a signed-out reader is `participantRole: "none"` rather than a set of
/// zeros that look like measurements.
#[derive(Debug, Clone, Copy)]
pub struct Viewer {
    /// What the caller may do.
    pub role: ParticipantRole,
    /// Whether they are in the room now.
    pub is_joined: bool,
    /// Whether they bookmarked it.
    pub is_bookmarked: bool,
    /// How many are in the room.
    pub live_participants_count: i64,
    /// Their resume position, in seconds.
    pub time_remaining_seconds: Option<i64>,
}

/// One row in `GET /broadcasts`.
///
/// A projection, not a [`BroadcastResponse`]: the list has to render fifty cards
/// without fifty creator joins, so the creator's name comes along in the same row and
/// nothing else does.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastListItem {
    /// The broadcast id.
    pub id: Uuid,
    /// What it is called.
    pub title: String,
    /// Optional description.
    pub description: Option<String>,
    /// The creator's IANA zone.
    pub time_zone: Option<String>,
    /// Resolved cover image URL.
    pub image_url: Option<String>,
    /// Cover image storage key.
    pub image_id: Option<String>,
    /// The database column.
    pub status: BroadcastStatus,
    /// What a client renders.
    pub state: BroadcastState,
    /// When it was created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When it is scheduled to start.
    #[serde(with = "time::serde::rfc3339::option")]
    pub start_time: Option<OffsetDateTime>,
    /// When it ended.
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_time: Option<OffsetDateTime>,
    /// How many have ever joined.
    pub total_participants: i64,
    /// Who owns it.
    pub creator_id: Uuid,
    /// The creator's display name.
    pub creator_name: String,
    /// The creator's avatar key.
    pub creator_avatar_id: Option<String>,
    /// The creator's resolved avatar URL.
    pub creator_avatar_url: Option<String>,
}

/// One row in `GET /broadcasts/{id}/participants`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantListItem {
    /// The user id.
    pub id: Uuid,
    /// Display name.
    pub full_name: String,
    /// Avatar key.
    pub avatar_id: Option<String>,
    /// Resolved avatar URL.
    pub avatar_url: Option<String>,
    /// What they may do.
    pub role: ParticipantRole,
    /// When they joined.
    #[serde(with = "time::serde::rfc3339")]
    pub joined_at: OffsetDateTime,
    /// Whether they are still in the room.
    pub is_joined: bool,
}

/// Returned by go-live and join: the broadcast plus the only token a client gets.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastSessionResponse {
    /// The broadcast, in the same shape `GET /broadcasts/{id}` returns.
    pub broadcast: BroadcastResponse,
    /// A short-lived media-server JWT scoped to this room and role.
    ///
    /// The *only* place this token is ever sent. Minting it needs no database, so a
    /// client that reconnects refreshes the token rather than re-joining.
    pub token: String,
}

/// `POST /broadcasts/{id}/leave`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveBroadcastResponse {
    /// Always `true` — the endpoint answers 200 or an error, never `false`.
    pub success: bool,
    /// The broadcast that was left.
    pub broadcast_id: Uuid,
    /// Who left.
    pub user_id: Uuid,
    /// When.
    #[serde(with = "time::serde::rfc3339")]
    pub left_at: OffsetDateTime,
}

/// `POST /broadcasts/{id}/end`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndBroadcastResponse {
    /// The broadcast that ended.
    pub broadcast_id: Uuid,
    /// Its title, so the client's summary does not need a second request.
    pub broadcast_title: String,
    /// Its cover image URL, for the same reason.
    pub broadcast_image_url: Option<String>,
    /// Who owned it.
    pub creator_id: Uuid,
    /// Why it ended.
    pub ended_reason: EndReason,
    /// When.
    #[serde(with = "time::serde::rfc3339")]
    pub ended_at: OffsetDateTime,
    /// How long it ran, in seconds.
    pub duration_secs: i64,
    /// How many ever joined.
    pub total_participants: i64,
    /// Whether a recording was kept.
    pub recording_enabled: bool,
}

/// A cohost as the add/remove endpoints report it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CohostResponse {
    /// Who was added or removed.
    pub user: UserSummary,
    /// How many cohosts the broadcast now has.
    pub cohost_count: usize,
}

/// Shared title rule, so create and update cannot disagree about what a title is.
fn validate_title(title: &str, fields: &mut FieldErrors) {
    let length = title.trim().chars().count();
    if length < TITLE_MIN {
        fields.push(
            "title",
            format!("Title must be at least {TITLE_MIN} characters"),
        );
    } else if length > TITLE_MAX {
        fields.push(
            "title",
            format!("Title must be at most {TITLE_MAX} characters"),
        );
    }
}

/// Shared description rule.
fn validate_description(description: Option<&str>, fields: &mut FieldErrors) {
    if description.is_some_and(|text| text.chars().count() > DESCRIPTION_MAX) {
        fields.push(
            "description",
            format!("Description must be at most {DESCRIPTION_MAX} characters"),
        );
    }
}

/// The cohost list rule: a limit, and no duplicates in the same call.
///
/// Duplicates are refused here rather than deduplicated silently, because "you invited
/// Grace twice" is a client's bug and the response should say so rather than quietly
/// applying it once.
fn validate_cohosts(cohosts: Option<&[Uuid]>, fields: &mut FieldErrors) {
    let Some(cohosts) = cohosts else {
        return;
    };

    if cohosts.len() > MAX_COHOSTS {
        fields.push(
            "cohosts",
            format!("A broadcast can have at most {MAX_COHOSTS} cohosts"),
        );
    }

    let mut seen = std::collections::HashSet::with_capacity(cohosts.len());
    for id in cohosts {
        if !seen.insert(*id) {
            fields.push("cohosts", "The same user was listed twice");
        }
    }
}

/// Build a [`UserSummary`] from a row's user columns.
#[must_use]
pub fn user_summary(
    id: Uuid,
    full_name: String,
    avatar_id: Option<String>,
    avatar_url: Option<String>,
) -> UserSummary {
    UserSummary {
        id,
        full_name,
        avatar_id,
        avatar_url,
    }
}

/// Project a cohost row into a response payload, given the user it points at.
#[must_use]
pub fn cohost_response(
    cohost: &BroadcastCohost,
    user: UserSummary,
    count: usize,
) -> CohostResponse {
    debug_assert_eq!(cohost.cohost_id, user.id);
    CohostResponse {
        user,
        cohost_count: count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain inactive row: created, nothing scheduled, nothing ended.
    fn row() -> Broadcast {
        Broadcast {
            id: Uuid::from_u128(1),
            title: "Ada's show".to_owned(),
            description: None,
            status: BroadcastStatus::Inactive,
            creator_id: Uuid::from_u128(2),
            time_zone: None,
            image_url: None,
            image_id: None,
            broadcast_token: None,
            total_participants: 4,
            start_time: None,
            end_time: None,
            recording_enabled: false,
            recording_key: None,
            recording_url: None,
            published_at: None,
            end_reason: None,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        }
    }

    fn valid_create() -> CreateBroadcastRequest {
        CreateBroadcastRequest {
            title: "Ada's show".to_owned(),
            description: Some("Every Thursday".to_owned()),
            image_id: None,
            image_url: None,
            time_zone: Some("Africa/Lagos".to_owned()),
            start_time: None,
            recording_enabled: Some(true),
            cohosts: None,
        }
    }

    #[test]
    fn a_well_formed_request_passes() {
        assert!(valid_create().validate().is_ok());
    }

    #[test]
    fn every_bad_field_is_reported_in_one_pass() {
        // Short-circuiting would make a form with three mistakes need three round trips.
        let error = CreateBroadcastRequest {
            title: "A".to_owned(),
            description: Some("x".repeat(DESCRIPTION_MAX + 1)),
            cohosts: Some(vec![Uuid::from_u128(1); MAX_COHOSTS + 1]),
            ..valid_create()
        }
        .validate()
        .expect_err("refused");

        let MenoError::Validation { fields } = error else {
            panic!("expected a validation failure");
        };
        assert!(fields.contains_key("title"), "{fields:?}");
        assert!(fields.contains_key("description"), "{fields:?}");
        assert!(fields.contains_key("cohosts"), "{fields:?}");
    }

    #[test]
    fn a_short_title_is_refused_for_both_verbs() {
        // One shared rule, so create and update cannot drift apart.
        let short = "ab";
        let create = CreateBroadcastRequest {
            title: short.to_owned(),
            ..valid_create()
        };
        let update = UpdateBroadcastRequest {
            title: Some(short.to_owned()),
            ..Default::default()
        };

        assert!(create.validate().is_err());
        assert!(update.validate().is_err());
    }

    #[test]
    fn a_past_start_time_is_refused_before_the_service_runs() {
        let error = CreateBroadcastRequest {
            start_time: Some(now() - time::Duration::hours(1)),
            ..valid_create()
        }
        .validate()
        .expect_err("refused");

        let MenoError::Validation { fields } = error else {
            panic!("expected a validation failure");
        };
        assert!(fields.contains_key("startTime"), "{fields:?}");
    }

    #[test]
    fn a_future_start_time_is_accepted() {
        let request = CreateBroadcastRequest {
            start_time: Some(now() + time::Duration::hours(1)),
            ..valid_create()
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn a_duplicate_cohost_is_refused_rather_than_silently_deduplicated() {
        let id = Uuid::from_u128(9);
        let error = CreateBroadcastRequest {
            cohosts: Some(vec![id, id]),
            ..valid_create()
        }
        .validate()
        .expect_err("refused");

        let MenoError::Validation { fields } = error else {
            panic!("expected a validation failure");
        };
        assert!(fields.contains_key("cohosts"), "{fields:?}");
    }

    #[test]
    fn a_patch_with_no_fields_is_refused() {
        // Otherwise it is a round trip that answers 200 for having done nothing.
        let error = UpdateBroadcastRequest::default()
            .validate()
            .expect_err("refused");
        let MenoError::Validation { fields } = error else {
            panic!("expected a validation failure");
        };
        assert!(fields.contains_key("body"), "{fields:?}");
    }

    #[test]
    fn a_patch_carrying_one_field_is_accepted() {
        let request = UpdateBroadcastRequest {
            title: Some("A new title".to_owned()),
            ..Default::default()
        };
        assert!(request.validate().is_ok());
        assert!(!request.is_empty());
    }

    #[test]
    fn every_response_date_is_an_rfc3339_string() {
        // The regression this whole module's date policy exists for: a bare
        // `OffsetDateTime` serialises as `time`'s nine-number tuple. Asserted on the
        // rendered JSON rather than on the derive, because the derive is what is wrong.
        let broadcast = row();
        let response = BroadcastResponse::new(
            &broadcast,
            user_summary(Uuid::from_u128(2), "Ada".to_owned(), None, None),
            Vec::new(),
            Some(&Viewer {
                role: ParticipantRole::Participant,
                is_joined: true,
                is_bookmarked: false,
                live_participants_count: 3,
                time_remaining_seconds: Some(12),
            }),
        )
        .expect("assembling");

        let json = serde_json::to_value(&response).expect("serialisable");
        assert!(json["createdAt"].is_string(), "{json}");
        assert!(
            json["startTime"].is_null(),
            "an absent optional date is null: {json}"
        );
        assert_eq!(
            json["createdAt"],
            serde_json::Value::String(
                time::OffsetDateTime::format(
                    broadcast.created_at,
                    &time::format_description::well_known::Rfc3339
                )
                .expect("formattable")
            )
        );
    }

    #[test]
    fn a_viewer_with_no_facts_renders_as_absent_rather_than_zero() {
        // `participantRole: "none"` is a statement; `liveParticipantsCount: 0` for a
        // room with twelve people in it would be a lie, so an anonymous read reports no
        // viewer at all.
        let broadcast = row();
        let response = BroadcastResponse::new(
            &broadcast,
            user_summary(Uuid::from_u128(2), "Ada".to_owned(), None, None),
            Vec::new(),
            None,
        )
        .expect("assembling");

        assert_eq!(response.participant_role, ParticipantRole::None);
        assert!(!response.is_joined);
        assert_eq!(response.live_participants_count, 0);
    }

    #[test]
    fn a_duration_only_appears_once_the_broadcast_has_ended() {
        let mut broadcast = row();
        let response = |b: &Broadcast| {
            BroadcastResponse::new(
                b,
                user_summary(Uuid::from_u128(2), "Ada".to_owned(), None, None),
                Vec::new(),
                None,
            )
            .expect("assembling")
            .duration_seconds
        };

        broadcast.published_at = Some(now());
        assert_eq!(response(&broadcast), None, "live: no duration");

        broadcast.end_time = Some(now() + time::Duration::minutes(90));
        assert_eq!(response(&broadcast), Some(5400), "ended: the real length");
    }

    #[test]
    fn removing_a_cohost_defaults_to_taking_them_out_of_the_room() {
        // The absent body has to mean the safe thing: evict, do not leave a
        // de-cohosted participant publishing in a live room.
        let absent: RemoveCohostRequest = serde_json::from_str("{}").expect("decodes");
        assert!(absent.remove_from_room);

        let explicit: RemoveCohostRequest =
            serde_json::from_str(r#"{"removeFromRoom": false}"#).expect("decodes");
        assert!(!explicit.remove_from_room);
    }

    #[test]
    fn a_query_defaults_to_newest_first_and_the_participants_to_oldest_first() {
        let broadcast = BroadcastQuery::default();
        assert_eq!(broadcast.effective_order(), meno_core::Order::Desc);

        let participants = ParticipantQuery::default();
        assert_eq!(
            participants.effective_order(),
            meno_core::Order::Asc,
            "a roster reads as a queue, not as a feed"
        );
    }

    #[test]
    fn a_query_decodes_camel_case_and_clamps_its_limit() {
        let query: BroadcastQuery = serde_json::from_str(
            r#"{"creatorId":"11111111-1111-1111-1111-111111111111","order":"asc","limit":"5"}"#,
        )
        .expect("decodes");

        assert_eq!(query.limit(), 5);
        assert_eq!(query.effective_order(), meno_core::Order::Asc);

        // `limit` is carried on the wire as a query parameter and therefore a string
        // (the cursor module decodes it via `DisplayFromStr`), so a string is what the
        // API accepts — and it must still be clamped to the ceiling like every other list.
        let absurd: BroadcastQuery = serde_json::from_str(r#"{"limit":"10000"}"#).expect("decodes");
        assert_eq!(absurd.limit(), 100, "clamped like every other list");
    }

    #[test]
    fn a_status_filter_only_accepts_a_column_value() {
        let query: BroadcastQuery = serde_json::from_str(r#"{"status":"active"}"#).expect("ok");
        assert_eq!(query.status, Some(BroadcastStatus::Active));

        // "live" is a `state`, not a `status`. Accepting it would mean one filter with
        // two vocabularies and a client that cannot tell which half is wrong.
        assert!(serde_json::from_str::<BroadcastQuery>(r#"{"status":"live"}"#).is_err());
    }
}
