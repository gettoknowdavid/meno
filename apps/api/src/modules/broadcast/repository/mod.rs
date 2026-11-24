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
    Broadcast, BroadcastCohost, BroadcastParticipant, BroadcastPatch, BroadcastStatus,
    CohostInvitation, EndReason, InvitationStatus, JoinOutcome, JoinRequest, LeaveOutcome,
    NewBroadcast, ParticipantRole, PendingCohostInvitation, UserSummary,
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

    /// The broadcast `creator_id` currently has on air, or `None`.
    ///
    /// One account may have at most one live broadcast — `0017_broadcast_liveness.sql`
    /// enforces it with a partial unique index — and this is the read that turns a
    /// second attempt into a refusal before any media or row change happens.
    async fn find_live_by_creator(&self, creator_id: Uuid) -> Result<Option<Broadcast>, MenoError>;

    /// The broadcast `creator_id` has scheduled and not yet resolved, or `None`.
    ///
    /// One account may have one scheduled broadcast at a time: a schedule is a promise
    /// the host still has to keep, and two open promises make "which one is starting?"
    /// a question the client cannot answer. The slot is held until the promise is
    /// resolved — cancelled (a soft delete) or ended after going live — not merely until
    /// its start time passes: a broadcast that is 10 AM tomorrow is still the next one
    /// at 10:01 tomorrow if nobody has gone live or cancelled it.
    ///
    /// `exclude` lets an edit re-state the time it already has without colliding with
    /// itself.
    async fn find_open_scheduled_by_creator(
        &self,
        creator_id: Uuid,
        exclude: Option<Uuid>,
    ) -> Result<Option<Broadcast>, MenoError>;

    /// Every broadcast currently on air, reduced to what abandonment is decided from.
    ///
    /// The reaper asks this every tick. Deliberately not a page: a room left behind is
    /// a room that holds a LiveKit slot until it is closed, so a live broadcast past the
    /// first page still has to be found.
    async fn list_live(&self) -> Result<Vec<LiveBroadcast>, MenoError>;

    /// Record, or clear, the moment a live broadcast became abandonable.
    ///
    /// Only applies while the broadcast is still live, so a stale sweep cannot write a
    /// clock onto a row somebody else has already ended.
    async fn set_abandoned_since(
        &self,
        id: Uuid,
        since: Option<OffsetDateTime>,
    ) -> Result<(), MenoError>;

    /// How many people are in the room right now.
    async fn live_participant_count(&self, broadcast_id: Uuid) -> Result<i64, MenoError>;

    /// Every account whose participant row on `broadcast_id` is still open.
    ///
    /// The room's audience as *this application* recorded it, which is not the same
    /// set the media server reports: a listener who joined over HTTP has a row whether
    /// or not their media client ever connected, and a fan-out that only asked LiveKit
    /// therefore missed exactly the people it exists for.
    ///
    /// Unpaginated on purpose — this feeds a notification recipient list, not a screen.
    async fn present_participant_ids(&self, broadcast_id: Uuid) -> Result<Vec<Uuid>, MenoError>;

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

    /// Close every open row on a broadcast, returning how many were closed.
    ///
    /// Called on every end path. Without it the live count keeps counting people who
    /// left when the room ended, and the next broadcast's all-time counter starts from a
    /// roster that never emptied.
    async fn close_participants(
        &self,
        broadcast_id: Uuid,
        at: OffsetDateTime,
    ) -> Result<u64, MenoError>;

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

    // ── cohost invitations (migration 0008) ───────────────────────────────

    /// Issue, or re-issue, an invitation to co-host.
    ///
    /// Upsert on `(broadcast_id, invitee_id)`: inviting somebody who already has a
    /// declined row makes it pending again rather than failing, because a creator who
    /// tries again should not have to know the row existed. An accepted row is refused by
    /// the service before this is called.
    async fn create_invitation(
        &self,
        broadcast_id: Uuid,
        inviter_id: Uuid,
        invitee_id: Uuid,
    ) -> Result<CohostInvitation, MenoError>;

    /// An invitation by id, whatever its status.
    async fn find_invitation(&self, id: Uuid) -> Result<Option<CohostInvitation>, MenoError>;

    /// The pending invitations `invitee_id` can still answer, newest first.
    ///
    /// Joined to the broadcast and the inviter because the card is shown from this read
    /// alone; an invitation whose broadcast has ended or been deleted is omitted, since
    /// there is nothing left to co-host.
    async fn list_pending_invitations(
        &self,
        invitee_id: Uuid,
    ) -> Result<Vec<PendingCohostInvitation>, MenoError>;

    /// Record the invitee's answer; `None` when the row does not exist.
    async fn respond_invitation(
        &self,
        id: Uuid,
        status: InvitationStatus,
        at: OffsetDateTime,
    ) -> Result<Option<CohostInvitation>, MenoError>;

    // ── bookmarks (migration 0007) ────────────────────────────────────────

    /// Save `broadcast_id` for `user_id`.
    ///
    /// Idempotent: an upsert on `(user_id, broadcast_id)`, so keeping the same
    /// broadcast twice leaves one row and answers `false` the second time. A
    /// repeated tap is a normal thing for a star to do, not an error.
    async fn add_bookmark(&self, user_id: Uuid, broadcast_id: Uuid) -> Result<bool, MenoError>;

    /// Forget `broadcast_id` for `user_id`; `true` when a row was removed.
    ///
    /// The service treats "was not kept" as success: the caller asked for it to
    /// be gone, and it is.
    async fn remove_bookmark(&self, user_id: Uuid, broadcast_id: Uuid) -> Result<bool, MenoError>;

    /// Whether `user_id` has kept `broadcast_id`.
    ///
    /// The read behind `isBookmarked` on a broadcast response, so a detail and a
    /// list cannot disagree about the same account's standing.
    async fn is_bookmarked(&self, user_id: Uuid, broadcast_id: Uuid) -> Result<bool, MenoError>;

    /// One page of `user_id`'s bookmarks, newest save first.
    ///
    /// Joined to `broadcasts`, so a deleted broadcast is not in the answer — the
    /// row outlives the soft delete, and a card for something that no longer
    /// exists is worse than a shorter page.
    async fn list_bookmarks(
        &self,
        filter: BookmarkFilter,
    ) -> Result<Vec<BookmarkedBroadcast>, MenoError>;

    // ── scheduled reminders (§8.2's start job) ────────────────────────────

    /// Scheduled broadcasts whose start time has arrived and whose reminder has not
    /// gone out.
    ///
    /// The reminder is a fact that must happen once: `scheduled_notified_at` is the
    /// marker, so a worker that restarts every minute does not notify the same
    /// subscribers repeatedly.
    async fn list_due_scheduled(&self, at: OffsetDateTime) -> Result<Vec<Broadcast>, MenoError>;

    /// Claim one broadcast's reminder. `true` means this caller sent it.
    async fn mark_scheduled_notified(
        &self,
        id: Uuid,
        at: OffsetDateTime,
    ) -> Result<bool, MenoError>;

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

/// A live broadcast, as the abandonment reaper reads it.
///
/// Two columns and an id rather than a whole [`Broadcast`]: the reaper's rules are about
/// presence and elapsed time, and a type that carried a title would invite a rule that
/// reads one.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LiveBroadcast {
    /// Which broadcast.
    pub id: Uuid,
    /// Whose it is — the identity that has to be in the room for it to be healthy.
    pub creator_id: Uuid,
    /// When it first had nobody, or no host and no cohost; `None` while healthy.
    pub abandoned_since: Option<OffsetDateTime>,
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
    /// Only broadcasts whose title or description match this term.
    ///
    /// When set, the list is a public search: drafts and broadcasts that have not
    /// been on air are excluded, because a row nobody has heard yet is not one a
    /// stranger should find by typing at it.
    pub q: Option<String>,
    /// The page size, plus the one probe row.
    pub limit_plus_one: i64,
    /// The exclusive keyset boundary from the previous page.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
    /// Sort direction.
    pub order: meno_core::Order,
}

/// The filter a bookmark page is read with.
///
/// The cursor is `(saved_at, broadcast_id)` rather than `(created_at, id)`,
/// because this page is ordered by when the account saved the broadcast — the
/// index migration 0007 created. Decoded at the edge like every other cursor, so
/// no `WHERE` clause is ever built from a string.
#[derive(Debug, Clone, Copy)]
pub struct BookmarkFilter {
    /// Whose bookmarks.
    pub user_id: Uuid,
    /// The page size, plus the one probe row.
    pub limit_plus_one: i64,
    /// The exclusive keyset boundary from the previous page.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
}

/// A bookmarked broadcast with the moment it was saved.
///
/// The pair, not a bare [`Broadcast`]: the page is ordered by `saved_at`, and a
/// projection that dropped it could not build the next page's cursor.
#[derive(Debug, Clone)]
pub struct BookmarkedBroadcast {
    /// The broadcast itself.
    pub broadcast: Broadcast,
    /// When the account saved it.
    pub saved_at: OffsetDateTime,
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
