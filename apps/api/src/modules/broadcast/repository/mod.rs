//! What the broadcast module needs from Postgres, and nothing else.
//!
//! # Trait objects, deliberately
//!
//! Same decision as [`crate::modules::auth::repository`], for the same reason: every
//! production call site uses [`PgBroadcastRepo`], so a generic parameter would be one
//! way of spelling a monomorphisation, and a `dyn` costs one pointer per call and buys
//! a compile error when a dependency is missing. `PgBroadcastRepo` and
//! [`InMemoryBroadcastRepo`] appear in exactly one file — [`super::state`] — so a
//! service or handler that reached for `sqlx::PgPool` instead of [`BroadcastRepo`]
//! would not fail review, it would fail to compile.
//!
//! # Errors
//!
//! Every method returns [`MenoError`]. A `sqlx::Error` never crosses this boundary: it
//! becomes [`MenoError::Internal`] with the operation named, so driver text reaches the
//! log and not the client (§4.2).
//!
//! # The three methods that carry a correctness decision
//!
//! - [`BroadcastRepo::join`] returns [`JoinOutcome`] rather than `bool`, because
//!   "already in the room", "here for the first time" and "came back" are three
//!   different states and the all-time counter (§7.6) must move for only two of them.
//! - [`BroadcastRepo::leave`] returns [`LeaveOutcome`], because "the host tried to
//!   leave" is a refusal the service reports and not a row update.
//! - [`BroadcastRepo::list`] takes a [`BroadcastFilter`] whose cursor is already
//!   decoded, so the SQL builder cannot accidentally apply the wrong direction.

mod pg;

// The in-memory repository is a real implementation of this module's trait, not test
// scaffolding: it backs the `tests/` integration suite, which links `meno_api` as an
// external crate where `#[cfg(test)]` is false. Gated on the feature so a production
// build still leaves it out.
#[cfg(any(test, feature = "test-support"))]
mod memory;

#[cfg(any(test, feature = "test-support"))]
pub use memory::InMemoryBroadcastRepo;
pub use pg::PgBroadcastRepo;

use async_trait::async_trait;
use meno_core::Error as MenoError;
use time::OffsetDateTime;
use uuid::Uuid;

use super::model::{
    Broadcast, BroadcastCohost, BroadcastParticipant, BroadcastPatch, BroadcastStatus, EndReason,
    JoinOutcome, JoinRequest, LeaveOutcome, NewBroadcast, ParticipantRole, UserSummary,
};

/// Postgres-backed storage for the broadcast module.
#[async_trait]
pub trait BroadcastRepo: Send + Sync + std::fmt::Debug {
    // ── broadcasts ─────────────────────────────────────────────────────────

    /// Create a broadcast with its cohosts, atomically.
    ///
    /// One transaction because the alternative is a live broadcast whose creator's
    /// invited cohosts are missing — which looks like a permissions bug to everyone
    /// except the person debugging it.
    async fn create(&self, new: NewBroadcast) -> Result<Broadcast, MenoError>;

    /// A live broadcast by id, or `None`.
    ///
    /// "Live" here means not soft-deleted, not "currently broadcasting": the module has
    /// to read a finished broadcast to render its ended state.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Broadcast>, MenoError>;

    /// Apply a patch, returning the row as written.
    async fn update(&self, id: Uuid, patch: BroadcastPatch) -> Result<Broadcast, MenoError>;

    /// Soft-delete: set `deleted_at`, leaving the row in place.
    ///
    /// Soft because `broadcast_participants` and `chat_messages` reference it, and the
    /// `update_user_broadcast_count` trigger only decrements a creator's count on the
    /// `deleted_at` transition — so a hard delete would leave that counter permanently
    /// high (§7.6's neighbourhood).
    async fn soft_delete(&self, id: Uuid) -> Result<bool, MenoError>;

    /// Mark a broadcast live, recording `published_at`.
    ///
    /// Returns [`None`] when the row is not in `inactive` state, so "go live" twice is
    /// an answer rather than a second `published_at`.
    async fn mark_live(&self, id: Uuid, at: OffsetDateTime)
    -> Result<Option<Broadcast>, MenoError>;

    /// Mark a broadcast ended, recording the reason.
    ///
    /// Returns [`None`] when it was not live, so a second end is reported rather than
    /// rewriting the first one's reason.
    async fn mark_ended(
        &self,
        id: Uuid,
        reason: EndReason,
        at: OffsetDateTime,
    ) -> Result<Option<Broadcast>, MenoError>;

    /// One page of broadcasts, newest first unless the filter says otherwise.
    async fn list(&self, filter: BroadcastFilter) -> Result<Vec<Broadcast>, MenoError>;

    /// How many people are in the room right now.
    async fn live_participant_count(&self, broadcast_id: Uuid) -> Result<i64, MenoError>;

    // ── participants (§7.6) ───────────────────────────────────────────────

    /// Record that `request.participant_id` is in the room.
    ///
    /// The row write and the all-time counter are one call because they are one fact: a
    /// counter that moves separately from the rows that justify it is §7.6's defect,
    /// where `total_participants` drifts permanently from the participants table.
    async fn join(&self, request: JoinRequest) -> Result<JoinOutcome, MenoError>;

    /// Close a participant's live row.
    async fn leave(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<LeaveOutcome, MenoError>;

    /// A participant's row, live or not.
    async fn find_participant(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<Option<BroadcastParticipant>, MenoError>;

    /// One page of the participant roster, earliest join first by default.
    async fn list_participants(
        &self,
        broadcast_id: Uuid,
        filter: ParticipantFilter,
    ) -> Result<Vec<BroadcastParticipant>, MenoError>;

    // ── cohosts ────────────────────────────────────────────────────────────

    /// The current cohosts of a broadcast.
    async fn list_cohosts(&self, broadcast_id: Uuid) -> Result<Vec<BroadcastCohost>, MenoError>;

    /// Replace the cohost list with `cohosts`.
    ///
    /// One statement's worth of work, so a client that sends the full list gets one
    /// consistent result rather than a window where the list is half-applied.
    async fn set_cohosts(
        &self,
        broadcast_id: Uuid,
        invited_by: Uuid,
        cohosts: &[Uuid],
    ) -> Result<Vec<BroadcastCohost>, MenoError>;

    // ── users ──────────────────────────────────────────────────────────────

    /// A live account, projected to what a broadcast response needs.
    async fn find_user(&self, id: Uuid) -> Result<Option<UserSummary>, MenoError>;

    /// Several accounts at once, for assembling a response without a query per user.
    ///
    /// One call rather than a loop on purpose: §9.4's "no query in a loop" is a
    /// rendering performance rule, and a detail response that joins one creator and
    /// three cohosts is four round trips unless it says so here.
    async fn find_users(&self, ids: &[Uuid]) -> Result<Vec<UserSummary>, MenoError>;
}

/// The filter a broadcast list is read with, already decoded.
///
/// The cursor arrives as a `(timestamp, id)` pair rather than as an opaque string,
/// because decoding happens once at the edge and every layer below works with values.
/// A malformed cursor is therefore a 400 at the handler and never a `WHERE` clause
/// built from a string.
#[derive(Debug, Clone, Default)]
pub struct BroadcastFilter {
    /// Only this creator's broadcasts.
    pub creator_id: Option<Uuid>,
    /// Only broadcasts in this status.
    pub status: Option<BroadcastStatus>,
    /// The page size, plus the one probe row.
    pub limit_plus_one: i64,
    /// The exclusive keyset boundary from the previous page.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
    /// Sort direction.
    pub order: meno_core::Order,
}

/// The filter a participant roster is read with.
#[derive(Debug, Clone, Default)]
pub struct ParticipantFilter {
    /// Only participants holding this role.
    pub role: Option<ParticipantRole>,
    /// Only those currently in the room.
    pub present_only: bool,
    /// The page size, plus the one probe row.
    pub limit_plus_one: i64,
    /// The exclusive keyset boundary from the previous page.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
    /// Sort direction.
    pub order: meno_core::Order,
}

/// Everything needed to build a [`BroadcastRepo`], in one place (plan §4.1).
///
/// A struct rather than a positional argument list, because a two-argument `new` is
/// where two same-typed dependencies get swapped at a call site and nothing complains.
#[derive(Debug, Clone)]
pub struct RepoDeps {
    /// The pool to read and write through.
    pub pool: sqlx::PgPool,
}
