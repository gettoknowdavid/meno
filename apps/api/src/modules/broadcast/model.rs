//! Broadcast rows, and the four words the domain uses to describe one.
//!
//! Ported from `master`'s `modules/broadcast/model.rs`, with the changes the monorepo
//! requires:
//!
//! - **`EndReason` and `ParticipantRole` encode/decode through one place.** `master`
//!   hand-wrote `Type`/`Decode`/`Encode` for `EndReason` and gave `ParticipantRole` a
//!   `From<String>`, so the two directions of the same mapping existed twice and could
//!   disagree — `From<String> for EndReason` mapped `"anything else"` to `Normal`.
//!   Every variant is now a `const` string, so the wire value and the SQL value are
//!   the same constant.
//! - **Every enum variant is documented**, because the workspace `missing_docs` lint is
//!   a warning and this file is the reference for what a `status` column may hold.
//! - **`Broadcast::state()` is a pure function of the row** (§7.6's neighbourhood): the
//!   status column and the timestamps are the only inputs, and a client that disagrees
//!   about "is this live" is the bug the `state` field on the response exists to
//!   prevent.
//!
//! # `status` versus `state`
//!
//! `status` is the database column: `inactive` or `active`, and nothing else — it is
//! constrained by `broadcasts_status_check` in migration 0004. `state` is what a client
//! renders, and it is derived: a broadcast with no `end_time` and a future `start_time`
//! is *scheduled* even though its status is `inactive`. The column is the source of
//! truth and the derived value is never written back, so the two cannot drift.

use sqlx::{FromRow, Type};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::infrastructure::livekit::dto::LivekitRole;

/// A `broadcasts` row, as the module reads it.
#[derive(Debug, Clone, FromRow)]
pub struct Broadcast {
    /// Primary key.
    pub id: Uuid,
    /// What the broadcast is called.
    pub title: String,
    /// Optional description, capped at 244 characters by the column.
    pub description: Option<String>,
    /// `inactive` or `active` — the database column.
    pub status: BroadcastStatus,
    /// Who owns it, and who alone may end it.
    pub creator_id: Uuid,
    /// IANA zone the creator picked, e.g. `Africa/Lagos`.
    pub time_zone: Option<String>,
    /// Resolved image URL.
    pub image_url: Option<String>,
    /// Storage key of the image.
    pub image_id: Option<String>,
    /// Token the media server last issued for the host, if any.
    pub broadcast_token: Option<String>,
    /// All-time join count. §7.6's drifting counter — see the repository.
    pub total_participants: i64,
    /// When it was scheduled to start.
    pub start_time: Option<OffsetDateTime>,
    /// When it actually ended.
    pub end_time: Option<OffsetDateTime>,
    /// Whether a recording may be kept.
    pub recording_enabled: bool,
    /// Storage key of the recording.
    pub recording_key: Option<String>,
    /// Resolved recording URL.
    pub recording_url: Option<String>,
    /// When it went live.
    pub published_at: Option<OffsetDateTime>,
    /// Why it ended. Null while it never has.
    pub end_reason: Option<EndReason>,
    /// Row creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
    /// Soft-delete marker; every read filters on it.
    pub deleted_at: Option<OffsetDateTime>,
}

impl Broadcast {
    /// Whether the broadcast is live right now.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.status == BroadcastStatus::Active && self.end_time.is_none()
    }

    /// What a client renders, derived from the row alone.
    ///
    /// `Reconnecting` is *not* derived here: it needs a fact from Redis (whether the
    /// host's grace key exists), so it belongs to the service, which is the only layer
    /// that can ask. Everything else is a pure function of this row.
    #[must_use]
    pub fn state(&self) -> BroadcastState {
        if self.is_live() {
            return BroadcastState::Live;
        }
        if self.end_time.is_some() || self.status == BroadcastStatus::Ended {
            return BroadcastState::Ended;
        }
        if self.start_time.is_some_and(|start| start > now()) {
            return BroadcastState::Scheduled;
        }
        BroadcastState::Draft
    }

    /// Whether `start_time` is still in the future.
    ///
    /// Separate from [`Self::state`] because this is a *rule* — "cannot schedule into
    /// the past" — and it is checked at creation and at update, not on every read.
    #[must_use]
    pub fn is_scheduled_for_the_future(&self) -> bool {
        self.start_time.is_some_and(|start| start > now())
    }
}

/// The clock, as one call site.
///
/// Every "is this in the past" decision goes through here so a test can reason about
/// the boundary without constructing a timestamp by hand, and so the comparison is
/// always against UTC — a local-time comparison would make "in the past" mean something
/// different on two replicas.
#[must_use]
pub fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// The live/inactive column, exactly as `broadcasts_status_check` allows it.
///
/// Three variants because the constraint has three values. This module only ever
/// *writes* `inactive` and `active` — "ended" is expressed by `end_time`, which is the
/// column a cursor on the ended feed can sort by — but a row may legally carry `ended`
/// from another writer, and an enum that cannot decode a row the database accepts turns
/// that into a 500 on a read path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BroadcastStatus {
    /// Not live.
    Inactive,
    /// Live.
    Active,
    /// Finished. Carried for decoding; this module writes `inactive` + `end_time`.
    Ended,
}

impl BroadcastStatus {
    /// The value stored in the column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Active => "active",
            Self::Ended => "ended",
        }
    }
}

impl std::fmt::Display for BroadcastStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a participant may do in a room.
///
/// Ordered by privilege and serialised in the lowercase spelling the `role` column and
/// the client both use. `None` is "not in this broadcast", which is a real state and not
/// a missing value: a detail response carries it so the client does not have to infer
/// it from the absence of a cohost entry.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    Type,
    serde::Serialize,
    serde::Deserialize,
)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ParticipantRole {
    /// May publish and moderate, but cannot end the broadcast.
    Cohost,
    /// Owns the broadcast. Full room admin.
    Host,
    /// May subscribe only.
    Participant,
    /// Not in this broadcast.
    #[default]
    None,
}

impl ParticipantRole {
    /// The value stored in the column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Cohost => "cohost",
            Self::Participant => "participant",
            Self::None => "none",
        }
    }

    /// Parse a stored value.
    ///
    /// Returns `None` for anything unrecognised rather than defaulting to
    /// [`Self::Participant`]: `master`'s `From<String>` mapped an unknown string to
    /// `Participant`, which silently downgraded an unknown host into a listener and made
    /// a permission bug look like a data bug.
    #[must_use]
    pub fn from_wire(value: &str) -> Self {
        match value {
            "host" => Self::Host,
            "cohost" => Self::Cohost,
            "participant" => Self::Participant,
            _ => Self::None,
        }
    }

    /// The role the media server is told about.
    ///
    /// `None` is not a media role, so it maps to `Participant` — the least-privileged
    /// grant. That mapping is deliberate and is the reason the cast is explicit rather
    /// than a `match` with an `unreachable!()` arm: a value that reaches the media server
    /// must have a defined permission, and the defined permission for "we do not know"
    /// is the one that can listen and nothing more.
    #[must_use]
    pub const fn to_livekit(self) -> LivekitRole {
        match self {
            Self::Host => LivekitRole::Host,
            Self::Cohost => LivekitRole::Cohost,
            Self::Participant | Self::None => LivekitRole::Participant,
        }
    }
}

impl std::fmt::Display for ParticipantRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a broadcast ended, exactly as `broadcasts_end_reason_check` allows it.
/// `#[derive(Type)]` rather than hand-written `Decode`/`Encode` for the same reason
/// [`ParticipantRole`] has one: the derive generates them from the *same* `rename_all`
/// as the serde attribute, so the SQL value and the wire value cannot drift. A hand-
/// written pair is two more copies of the mapping — which is how `master` ended up
/// mapping an unknown string to `Normal`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Type, serde::Serialize, serde::Deserialize,
)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// The host ended it.
    #[default]
    Normal,
    /// The host's socket dropped past the grace period.
    HostDisconnected,
    /// An administrator ended it.
    AdminForced,
    /// The recording or participant quota was reached.
    QuotaExceeded,
    /// Still live, or the row predates the column being populated.
    None,
}

impl EndReason {
    /// The value stored in the column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::HostDisconnected => "host_disconnected",
            Self::AdminForced => "admin_forced",
            Self::QuotaExceeded => "quota_exceeded",
            Self::None => "none",
        }
    }

    /// Parse a stored value.
    ///
    /// `None` for an unrecognised string, because the column is nullable and `NULL`
    /// means "has not ended"; inventing a reason for a row that has not ended would put
    /// a false statement in a client's summary.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(Self::Normal),
            "host_disconnected" => Some(Self::HostDisconnected),
            "admin_forced" => Some(Self::AdminForced),
            "quota_exceeded" => Some(Self::QuotaExceeded),
            _ => None,
        }
    }
}

impl std::fmt::Display for EndReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a client renders, derived from a row plus at most one fact from Redis.
///
/// Five states, because the client branches on this and a missing case is a client that
/// shows "live" for a broadcast that ended an hour ago.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastState {
    /// Live, with the host connected.
    Live,
    /// Live, with the host's socket in its grace window.
    Reconnecting,
    /// Ended.
    Ended,
    /// Scheduled for a future `start_time`.
    Scheduled,
    /// Created, with nothing scheduled and no end.
    Draft,
}

impl BroadcastState {
    /// The wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Reconnecting => "reconnecting",
            Self::Ended => "ended",
            Self::Scheduled => "scheduled",
            Self::Draft => "draft",
        }
    }
}

impl std::fmt::Display for BroadcastState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `broadcast_participants` row.
#[derive(Debug, Clone, FromRow)]
pub struct BroadcastParticipant {
    /// Owning broadcast.
    pub broadcast_id: Uuid,
    /// The participant.
    pub participant_id: Uuid,
    /// What they may do. Hosts and cohosts are rows here too, so "who is in this
    /// broadcast" is one query rather than a join of three tables.
    pub role: ParticipantRole,
    /// First join. The table has one row per `(broadcast, participant)` and no
    /// "last joined" column, so a return visit updates `left_at` rather than adding a
    /// row — which is why the all-time counter needs the explicit bump in
    /// [`JoinOutcome::Rejoined`].
    pub joined_at: OffsetDateTime,
    /// When they left, if they have.
    pub left_at: Option<OffsetDateTime>,
    /// Resume position, in seconds, for "continue listening".
    pub last_listen_position_seconds: i32,
    /// When that position was last written.
    pub last_listened_at: Option<OffsetDateTime>,
}

impl BroadcastParticipant {
    /// Whether this row represents someone currently in the room.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.left_at.is_none()
    }
}

/// A `broadcast_cohosts` row.
#[derive(Debug, Clone, FromRow)]
pub struct BroadcastCohost {
    /// Owning broadcast.
    pub broadcast_id: Uuid,
    /// The cohost.
    pub cohost_id: Uuid,
    /// Who invited them.
    pub invited_by: Uuid,
    /// When.
    pub invited_at: OffsetDateTime,
    /// When they were removed, if they were.
    pub removed_at: Option<OffsetDateTime>,
}

impl BroadcastCohost {
    /// Whether this row represents a current cohost.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.removed_at.is_none()
    }
}

/// A user, reduced to what a broadcast response needs.
///
/// Not `modules::auth::dto::UserResponse`: that is *auth's* wire contract, and a module
/// may not reach sideways into another module (§ the layer rule in `modules/mod.rs`).
/// This is the projection the broadcast surface needs, and it is assembled from the
/// repository rather than from the auth module, which is also why a broadcast response
/// cannot start leaking an auth-only field.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, FromRow)]
pub struct UserSummary {
    /// Account id.
    pub id: Uuid,
    /// Display name.
    pub full_name: String,
    /// Avatar storage key.
    pub avatar_id: Option<String>,
    /// Resolved avatar URL.
    pub avatar_url: Option<String>,
}

/// The fields a new `broadcasts` row needs.
#[derive(Debug, Clone)]
pub struct NewBroadcast {
    /// Who is creating it.
    pub creator_id: Uuid,
    /// Title, already validated.
    pub title: String,
    /// Optional description.
    pub description: Option<String>,
    /// Optional IANA zone.
    pub time_zone: Option<String>,
    /// Optional image key.
    pub image_id: Option<String>,
    /// Optional resolved image URL.
    pub image_url: Option<String>,
    /// Optional scheduled start.
    pub start_time: Option<OffsetDateTime>,
    /// Whether the recording may be kept.
    pub recording_enabled: bool,
    /// Cohosts to attach in the same transaction.
    pub cohosts: Vec<Uuid>,
}

/// The fields an update may change.
///
/// A struct of `Option`s rather than a set of setters, because the repository builds one
/// `UPDATE` with `COALESCE` from it: a field that is absent leaves the column alone, and
/// a field present and `NULL` is the difference the repository cannot express any other
/// way. `cohosts` is the full replacement list when present — add and remove are
/// separate endpoints, and a "patch" that silently removes people is the wrong default.
#[derive(Debug, Clone, Default)]
pub struct BroadcastPatch {
    /// New title.
    pub title: Option<String>,
    /// New description.
    pub description: Option<String>,
    /// New IANA zone.
    pub time_zone: Option<String>,
    /// New image key.
    pub image_id: Option<String>,
    /// New image URL.
    pub image_url: Option<String>,
    /// New scheduled start.
    pub start_time: Option<OffsetDateTime>,
    /// Whether the recording may be kept.
    pub recording_enabled: Option<bool>,
    /// The complete cohost list, when the caller is replacing it.
    pub cohosts: Option<Vec<Uuid>>,
}

impl BroadcastPatch {
    /// Whether the patch would change nothing.
    ///
    /// Checked before the write so an empty `PATCH` is a 400 rather than a round trip
    /// that reports success for having done nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.time_zone.is_none()
            && self.image_id.is_none()
            && self.image_url.is_none()
            && self.start_time.is_none()
            && self.recording_enabled.is_none()
            && self.cohosts.is_none()
    }
}

/// A participant joining, as the repository is told about it.
///
/// §7.6 in one type: the row to write and the counter to fix are the same fact, so they
/// travel together and the repository has no way to do one without the other.
#[derive(Debug, Clone)]
pub struct JoinRequest {
    /// The broadcast.
    pub broadcast_id: Uuid,
    /// Who is joining.
    pub participant_id: Uuid,
    /// What they may do — the service decides, never the caller.
    pub role: ParticipantRole,
}

/// What a join produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinOutcome {
    /// The participant was not in the room; the row was created.
    Joined,
    /// The participant was already in the room and stayed.
    AlreadyPresent,
    /// The participant had left and came back; the row was re-opened and the
    /// all-time counter incremented.
    Rejoined,
}

/// The outcome of a leave, which the service reports rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveOutcome {
    /// A live row was closed.
    Left,
    /// The participant was not in the room.
    NotPresent,
    /// The host tried to leave their own broadcast.
    IsHost,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(days: i64) -> OffsetDateTime {
        now() + time::Duration::days(days)
    }

    fn broadcast() -> Broadcast {
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
            total_participants: 0,
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

    #[test]
    fn a_new_broadcast_is_a_draft() {
        // The state a client renders before anything has happened to it.
        assert_eq!(broadcast().state(), BroadcastState::Draft);
    }

    #[test]
    fn a_future_start_time_makes_it_scheduled_not_draft() {
        // The distinction the `status` column cannot express: both rows are
        // `inactive`, and the client must still render them differently.
        let mut row = broadcast();
        row.start_time = Some(at(1));
        assert_eq!(row.state(), BroadcastState::Scheduled);
        assert_eq!(row.status, BroadcastStatus::Inactive);
        assert!(row.is_scheduled_for_the_future());
    }

    #[test]
    fn an_active_row_is_live_until_it_has_an_end_time() {
        let mut row = broadcast();
        row.status = BroadcastStatus::Active;
        assert_eq!(row.state(), BroadcastState::Live);
        assert!(row.is_live());

        row.end_time = Some(now());
        assert!(!row.is_live(), "an ended row is never live");
        assert_eq!(
            row.state(),
            BroadcastState::Ended,
            "status alone must not keep a finished broadcast looking live"
        );
    }

    #[test]
    fn a_past_start_time_is_not_scheduled() {
        let mut row = broadcast();
        row.start_time = Some(at(-1));
        assert_eq!(row.state(), BroadcastState::Draft);
        assert!(!row.is_scheduled_for_the_future());
    }

    #[test]
    fn an_unrecognised_role_reads_as_absent_not_as_a_participant() {
        // The `master` bug: an unknown string defaulted to `Participant`, silently
        // downgrading a host into a listener and turning a permission problem into an
        // unexplainable data problem.
        assert_eq!(ParticipantRole::from_wire("host"), ParticipantRole::Host);
        assert_eq!(
            ParticipantRole::from_wire("admin"),
            ParticipantRole::None,
            "an unknown role must not become a real one"
        );
    }

    #[test]
    fn every_role_round_trips_through_its_wire_value() {
        for role in [
            ParticipantRole::Host,
            ParticipantRole::Cohost,
            ParticipantRole::Participant,
            ParticipantRole::None,
        ] {
            assert_eq!(
                ParticipantRole::from_wire(role.as_str()),
                role,
                "{} must survive a database round trip",
                role.as_str()
            );
        }
    }

    #[test]
    fn an_absent_role_gets_the_least_privileged_media_grant() {
        // A role reaching the media server must have a defined permission, and the
        // defined permission for "we do not know" is listen-only.
        assert!(!ParticipantRole::None.to_livekit().is_admin());
        assert!(!ParticipantRole::None.to_livekit().can_publish());
        assert!(ParticipantRole::Host.to_livekit().is_admin());
        assert!(ParticipantRole::Cohost.to_livekit().can_publish());
    }

    #[test]
    fn an_unknown_end_reason_is_absent_rather_than_normal() {
        // Inventing `Normal` for a row that has not ended puts a false statement in a
        // client's summary.
        assert_eq!(EndReason::from_wire("normal"), Some(EndReason::Normal));
        assert_eq!(
            EndReason::from_wire("host_disconnected"),
            Some(EndReason::HostDisconnected)
        );
        assert_eq!(EndReason::from_wire("whatever"), None);
    }

    #[test]
    fn every_enum_serialises_to_the_spelling_the_column_holds() {
        // The value on the wire and the value in the database are the same constant, so
        // this is the test that keeps the two spellings from drifting apart.
        for role in [
            ParticipantRole::Host,
            ParticipantRole::Cohost,
            ParticipantRole::Participant,
            ParticipantRole::None,
        ] {
            assert_eq!(
                serde_json::to_value(role).expect("serialisable"),
                serde_json::Value::String(role.as_str().to_owned())
            );
        }
        assert_eq!(
            serde_json::to_value(BroadcastState::Scheduled).expect("serialisable"),
            "scheduled"
        );
    }

    #[test]
    fn an_empty_patch_is_recognised_before_it_is_written() {
        // Checked by the service, so a `PATCH` with no fields is a 400 rather than a
        // round trip that reports success for having done nothing.
        assert!(BroadcastPatch::default().is_empty());
        assert!(
            !BroadcastPatch {
                title: Some("New".to_owned()),
                ..Default::default()
            }
            .is_empty()
        );
    }
}
