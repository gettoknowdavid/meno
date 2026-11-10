//! Background work: the sweeps that must run without a request (§6).
//!
//! # What runs here, and why it cannot run in a handler
//!
//! - **Auth retention** — expired refresh tokens, one-time codes and sessions past their
//!   lifetime. Nothing in the request path can delete them: a login only ever *reads*
//!   expiry to refuse an old credential, and the rows accumulate forever.
//! - **Notes retention** — soft-deleted notes and folders, hard-deleted after a grace
//!   period. `NotesService::purge_stale` existed with no caller; this is the caller.
//! - **The broadcast abandonment sweep** — a host whose phone died cannot make the
//!   request that would end their broadcast, so nobody would.
//!
//! # Why the worker, and only the worker
//!
//! [`crate::bootstrap::AppRole`] already splits the two binaries: the web process holds
//! sockets and serves requests, the worker exists to run this. Sweeping from both would
//! be duplicate work against the same tables, and on the free tier it is the same
//! shared CPU. The consequence is worth stating plainly: a deployment that runs only
//! `meno-api` gets no retention and no abandonment sweep. That is the contract of
//! running a worker.
//!
//! # A scheduler, not a queue
//!
//! Every job here is idempotent and derives its work from the tables, so there is
//! nothing to enqueue and nothing to lose if the process dies mid-sweep — the next tick
//! recomputes the same answer. That is why this is an interval loop rather than apalis
//! over a job table: a queue would add a migration, a claim protocol and a retry policy
//! to solve a problem these jobs do not have. If a job that *must* run once per item
//! arrives (a scheduled broadcast reminder, a recording finalisation), the queue belongs
//! here as a second kind of job and these loops stay as they are.
//!
//! # Starting from now
//!
//! `tokio::time::interval`'s first tick is immediate, so every job runs once at startup
//! rather than after its first period. A worker that restarts hourly still sweeps.

use std::time::Duration;

use time::OffsetDateTime;
use tokio::task::JoinSet;
use tokio::time::{MissedTickBehavior, interval};

use crate::modules::auth::state::AuthState;
use crate::modules::broadcast::reaper::{BroadcastReaper, SWEEP_INTERVAL};
use crate::modules::notes::state::NotesState;
use crate::state::MenoState;

/// How often expired auth rows are swept.
///
/// Hourly, not daily: these are the tables on the path of every authenticated request,
/// and the rows are cheap to remove in a pass that finds nothing.
pub const AUTH_RETENTION_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// How often soft-deleted notes and folders are hard-deleted.
///
/// Daily: this is irreversible deletion of user data, so it is not something to run more
/// often than the retention window needs.
pub const NOTES_RETENTION_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// How long a soft-deleted note is kept before it is hard-deleted.
///
/// Thirty days: long enough that a delete made on the wrong device is recoverable by
/// support, short enough that a busy account does not accumulate deleted rows forever.
pub const NOTES_RETENTION_DAYS: i64 = 30;

/// The background jobs, wired and ready.
///
/// Built from [`MenoState`] by the worker binary. Not `Clone`: it is consumed by
/// [`Self::run`], which is the only thing that should own the loops.
pub struct JobRunner {
    /// The auth module, for expired-row retention.
    auth: AuthState,
    /// The notes module, for soft-delete retention.
    notes: NotesState,
    /// The broadcast sweep, when a media server is configured.
    reaper: Option<BroadcastReaper>,
    /// A session older than the refresh-token lifetime cannot have a live token, so it
    /// is safe to delete. Kept as seconds because that is what
    /// [`time::Duration`] wants and the config exposes.
    session_cutoff_secs: i64,
}

impl std::fmt::Debug for JobRunner {
    /// Names the jobs and stops: the state holds a pool and a Redis handle, and a `{:?}`
    /// of either is not worth what it could print.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobRunner")
            .field("auth_retention", &AUTH_RETENTION_INTERVAL)
            .field("notes_retention", &NOTES_RETENTION_INTERVAL)
            .field("broadcast_sweep", &self.reaper.is_some())
            .field("session_cutoff_secs", &self.session_cutoff_secs)
            .finish()
    }
}

impl JobRunner {
    /// Take the pieces the runner needs from the assembled application.
    ///
    /// The reaper is `None` when LiveKit is not configured: a deployment without it
    /// cannot put a broadcast live in the first place, so every sweep would be a row of
    /// failed media calls.
    #[must_use]
    pub fn new(state: MenoState) -> Self {
        let session_cutoff_secs =
            i64::try_from(state.config.refresh_token_expiration.as_secs()).unwrap_or(i64::MAX);
        let reaper = state
            .config
            .livekit
            .as_ref()
            .map(|_| state.broadcast.reaper.clone());

        Self {
            auth: state.auth,
            notes: state.notes,
            reaper,
            session_cutoff_secs,
        }
    }

    /// Run every job until the task is aborted.
    ///
    /// There is no shutdown future here on purpose: the worker owns the process's stop.
    /// It aborts this task, which drops the [`JoinSet`] and with it every loop. A job
    /// that panics is logged and does not take the others down.
    pub async fn run(self) {
        let mut jobs = JoinSet::new();

        jobs.spawn(run_auth_retention(self.auth, self.session_cutoff_secs));
        jobs.spawn(run_notes_retention(self.notes));

        match self.reaper {
            Some(reaper) => {
                jobs.spawn(async move { reaper.run(SWEEP_INTERVAL).await });
                tracing::info!(
                    interval_secs = SWEEP_INTERVAL.as_secs(),
                    "broadcast abandonment sweep scheduled"
                );
            }
            None => {
                tracing::info!("no LIVEKIT settings; broadcast abandonment sweep not scheduled");
            }
        }

        while let Some(joined) = jobs.join_next().await {
            if let Err(error) = joined {
                tracing::error!(%error, "a background job stopped unexpectedly");
            }
        }
    }
}

/// Sweep expired auth rows on [`AUTH_RETENTION_INTERVAL`].
async fn run_auth_retention(auth: AuthState, session_cutoff_secs: i64) {
    let mut ticker = interval(AUTH_RETENTION_INTERVAL);
    // A sweep that ran long must not be followed by a burst of catch-up sweeps: the next
    // one is the one that matters, and the rows are still there either way.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;

        let now = OffsetDateTime::now_utc();
        let session_cutoff = now - time::Duration::seconds(session_cutoff_secs);

        match auth.service.purge_expired(now, session_cutoff).await {
            Ok(report) if report.total() > 0 => tracing::info!(
                sessions = report.sessions,
                refresh_tokens = report.refresh_tokens,
                one_time_codes = report.otps,
                "auth retention sweep removed expired rows"
            ),
            Ok(_) => tracing::debug!("auth retention sweep found nothing to remove"),
            Err(error) => tracing::warn!(%error, "auth retention sweep failed"),
        }
    }
}

/// Hard-delete soft-deleted notes and folders past [`NOTES_RETENTION_DAYS`].
async fn run_notes_retention(notes: NotesState) {
    let mut ticker = interval(NOTES_RETENTION_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;

        let cutoff = OffsetDateTime::now_utc() - time::Duration::days(NOTES_RETENTION_DAYS);

        match notes.service.purge_stale(cutoff).await {
            Ok((0, 0)) => tracing::debug!("notes retention sweep found nothing to remove"),
            Ok((notes_removed, folders_removed)) => tracing::info!(
                notes = notes_removed,
                folders = folders_removed,
                retention_days = NOTES_RETENTION_DAYS,
                "notes retention sweep hard-deleted soft-deleted rows"
            ),
            Err(error) => tracing::warn!(%error, "notes retention sweep failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The schedules, and the reason a job cannot silently stop being scheduled.

    use super::*;

    #[test]
    fn every_interval_is_long_enough_to_be_maintenance_and_short_enough_to_matter() {
        // The two failure directions: a zero interval is a busy loop against the
        // database, and a day-long auth sweep leaves a revocation table growing between
        // passes. Checked at compile time so a bad constant fails the build rather than
        // one test.
        const {
            assert!(AUTH_RETENTION_INTERVAL.as_secs() >= 60);
            assert!(AUTH_RETENTION_INTERVAL.as_secs() <= 24 * 60 * 60);
            assert!(NOTES_RETENTION_INTERVAL.as_secs() >= AUTH_RETENTION_INTERVAL.as_secs());
            // The retention window has to outlive a mistake, not a request.
            assert!(NOTES_RETENTION_DAYS >= 7);
        }
    }
}
