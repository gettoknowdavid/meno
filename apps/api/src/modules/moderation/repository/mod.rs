//! What the moderation module needs from storage, and nothing else.
//!
//! [`pg`] is Postgres, [`memory`] is the §10 double, and [`ModerationRepo`] is the only
//! name the service knows — `state.rs` chooses between them (§5.6).
//!
//! # Why the block checks live here and not in the modules that honour them
//!
//! `subscribers` and `chat` enforce blocks with their own `EXISTS` against
//! `user_blocks`, the same way `chat` reads `broadcasts` rather than importing that
//! module's repository (see [`crate::modules`]): a sideways import would make a follow
//! depend on this module's internals, and a shared service both call is the god object
//! §9.3 rules out. The duplication is one `EXISTS` clause per site.

pub mod pg;

// The in-memory repository is a real implementation of this module's trait, not test
// scaffolding: it backs the `tests/` integration suite, which links `meno_api` as an
// external crate where `#[cfg(test)]` is false.
#[cfg(any(test, feature = "test-support"))]
pub mod memory;

#[cfg(any(test, feature = "test-support"))]
pub use memory::InMemoryModerationRepo;
pub use pg::PgModerationRepo;

use async_trait::async_trait;
use meno_core::Error as MenoError;
use time::OffsetDateTime;
use uuid::Uuid;

use super::model::{BlockedUser, NewReport, Report, ReportStatus};

/// The moderation module's storage.
#[async_trait]
pub trait ModerationRepo: Send + Sync + std::fmt::Debug {
    /// Record a block, and answer whether a row was inserted.
    ///
    /// `false` means the block already existed — which is a success, not a conflict:
    /// blocking twice is one row and one decision.
    async fn block(&self, blocker_id: Uuid, blocked_id: Uuid) -> Result<bool, MenoError>;

    /// Remove a block, and answer whether a row was removed.
    ///
    /// `false` means there was nothing to remove, which [`Self::block`]'s doc explains is
    /// the state the caller asked for.
    async fn unblock(&self, blocker_id: Uuid, blocked_id: Uuid) -> Result<bool, MenoError>;

    /// Every account `blocker_id` has blocked, newest first.
    async fn blocked(&self, blocker_id: Uuid) -> Result<Vec<BlockedUser>, MenoError>;

    /// Whether a live account carries this id.
    async fn user_exists(&self, user_id: Uuid) -> Result<bool, MenoError>;

    /// Whether a broadcast (deleted or not) carries this id.
    ///
    /// Deleted included: a report about a broadcast that has since been removed is still
    /// a report somebody filed, and refusing it would lose the evidence.
    async fn broadcast_exists(&self, broadcast_id: Uuid) -> Result<bool, MenoError>;

    /// Store a report and return the row the database wrote.
    async fn create_report(&self, new: NewReport<'_>) -> Result<Report, MenoError>;

    /// One page of reports, newest first, optionally filtered to one status.
    async fn reports(&self, filter: ReportFilter) -> Result<Vec<Report>, MenoError>;

    /// One report by id, in whatever state it is.
    ///
    /// A separate read rather than a filtered page: an operator resolves an id from the
    /// queue and must be answered with that row, which no `LIMIT` on the newest reports
    /// can guarantee once the queue is longer than a page.
    async fn report(&self, id: Uuid) -> Result<Option<Report>, MenoError>;

    /// Move a report out of `open`, and answer whether a row matched.
    ///
    /// `false` means no report carries the id.
    async fn resolve_report(&self, id: Uuid, status: ReportStatus) -> Result<bool, MenoError>;
}

/// The queue a page of reports is read with, already decoded.
#[derive(Debug, Clone, Copy)]
pub struct ReportFilter {
    /// Only this status, or every status.
    pub status: Option<ReportStatus>,
    /// The page size, plus the one probe row.
    pub limit_plus_one: i64,
    /// The exclusive keyset boundary from the previous page.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
}
