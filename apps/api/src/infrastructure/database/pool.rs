//! Connection-pool sizing, and why the old numbers were wrong (§3.5).
//!
//! # The defect
//!
//! `master`'s [`create_postgres_pool`] hard-coded:
//!
//! ```text
//! max_connections: 20, max_lifetime: 30 min
//! ```
//!
//! §3.5 calls that out by name:
//!
//! > Consequence: the pool config in `database.rs` (`max_connections: 20`,
//! > `max_lifetime: 30 min`) is wrong for Neon. With PgBouncer, use `max_connections:
//! > 5–10` and a *short* `max_lifetime` (~5 min) so the pooler can recycle connections.
//!
//! Both halves matter, and for opposite reasons:
//!
//! - **`max_connections` too high** — Neon is reached through PgBouncer, which multiplexes
//!   client connections onto a small number of real ones. A client pool of 20 in front of
//!   it is not 20 connections' worth of throughput; it is 20 contenders for the pooler's
//!   server-side queue, and on the free tier that queue is what returns "too many
//!   connections".
//! - **`max_lifetime` too long** — a 30-minute connection is held open across PgBouncer's
//!   own recycling window and across Neon's idle disconnects, so the pool keeps handing out
//!   connections the pooler has already discarded. The symptom is an intermittent error on
//!   the first query after a quiet period, which is exactly the kind of bug that gets
//!   filed as "the API randomly 500s".
//!
//! [`PoolSettings`] makes both explicit and testable rather than three magic numbers in a
//! chain of builder calls.
//!
//! [`create_postgres_pool`]: super::create_postgres_pool

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

/// How the pool is sized.
///
/// Every value is a constant rather than an environment variable: they are properties of
/// the deployment target, not of this installation, and §3.5 fixes them for Neon. Making
/// them configurable would invite a value that works locally and starves production.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolSettings {
    /// Upper bound on client-side connections.
    pub max_connections: u32,

    /// Connections kept open even when idle.
    pub min_connections: u32,

    /// How long to wait for a connection before giving up.
    pub acquire_timeout: Duration,

    /// How long a connection may sit idle before being closed.
    pub idle_timeout: Duration,

    /// Maximum age of a connection before it is replaced.
    pub max_lifetime: Duration,

    /// Whether a connection is verified with a ping before being handed out.
    pub test_before_acquire: bool,
}

impl PoolSettings {
    /// §3.5's ceiling for Neon behind PgBouncer.
    ///
    /// Named so the test can assert the invariant without repeating the number.
    pub const MAX_CONNECTIONS_LIMIT: u32 = 10;

    /// §3.5's target maximum lifetime.
    pub const TARGET_MAX_LIFETIME: Duration = Duration::from_secs(300);

    /// The values this deployment uses.
    ///
    /// `max_connections` sits at the top of §3.5's 5–10 band rather than the middle: the
    /// worker binary runs fan-out jobs that would otherwise queue behind a small pool,
    /// and 10 is still comfortably inside the pooler's budget.
    ///
    /// `acquire_timeout` is 5 seconds rather than `master`'s 3. With a deliberately small
    /// pool, 3 seconds is short enough that a burst of concurrency fails requests that
    /// would have been served a moment later — the pool is small by design, so waiting is
    /// the correct behaviour, not a fault.
    #[must_use]
    pub const fn for_neon() -> Self {
        Self {
            max_connections: 10,
            min_connections: 2,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Self::TARGET_MAX_LIFETIME,
            // Kept on. `master` had it and it is the difference between a stale pooled
            // connection failing a request and being discarded first; §3.5's short
            // `max_lifetime` is what keeps that check cheap.
            test_before_acquire: true,
        }
    }

    /// Whether these settings fit inside the Neon/PgBouncer budget.
    ///
    /// Exposed so the invariant is asserted in a test rather than trusted to a comment that
    /// nobody re-reads when the number changes.
    #[must_use]
    pub fn fits_neon(&self) -> bool {
        self.max_connections <= Self::MAX_CONNECTIONS_LIMIT
            && self.max_connections >= self.min_connections
            && self.max_lifetime <= Self::TARGET_MAX_LIFETIME
    }

    /// Apply these settings to a pool builder.
    #[must_use]
    pub fn apply(&self, options: PgPoolOptions) -> PgPoolOptions {
        options
            .max_connections(self.max_connections)
            .min_connections(self.min_connections)
            .acquire_timeout(self.acquire_timeout)
            .idle_timeout(self.idle_timeout)
            .max_lifetime(self.max_lifetime)
            .test_before_acquire(self.test_before_acquire)
    }
}

impl Default for PoolSettings {
    /// [`Self::for_neon`], so `PoolSettings::default()` is never accidentally the old
    /// too-large pool.
    fn default() -> Self {
        Self::for_neon()
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the pool sizing. These are the tests that keep §3.5's correction from
    //! being undone by someone "restoring" `master`'s numbers.

    use super::*;

    #[test]
    fn the_pool_fits_the_neon_budget() {
        assert!(
            PoolSettings::for_neon().fits_neon(),
            "§3.5 requires max_connections of 5-10 and a max_lifetime of ~5 min"
        );
    }

    #[test]
    fn max_connections_is_inside_the_documented_band() {
        let settings = PoolSettings::for_neon();

        assert!(
            (5..=10).contains(&settings.max_connections),
            "§3.5 says 5-10, got {}",
            settings.max_connections
        );
        assert!(settings.max_connections <= PoolSettings::MAX_CONNECTIONS_LIMIT);
    }

    #[test]
    fn max_lifetime_is_five_minutes_not_thirty() {
        // The regression this file exists to prevent: `master`'s 30 minutes kept
        // connections alive across PgBouncer's recycling window.
        let settings = PoolSettings::for_neon();

        assert_eq!(settings.max_lifetime, Duration::from_secs(300));
        assert!(
            settings.max_lifetime < Duration::from_secs(30 * 60),
            "a 30-minute lifetime is what §3.5 flags"
        );
    }

    #[test]
    fn the_minimum_never_exceeds_the_maximum() {
        // Not a hypothetical: a pool with `min > max` silently behaves like `max`, so the
        // minimum would be a lie rather than a setting.
        let settings = PoolSettings::for_neon();

        assert!(settings.min_connections <= settings.max_connections);
    }

    #[test]
    fn acquiring_has_enough_room_for_a_deliberately_small_pool() {
        // 3 seconds with a 10-connection pool turns a burst of concurrency into request
        // failures that a slightly larger pool would have absorbed.
        assert!(PoolSettings::for_neon().acquire_timeout >= Duration::from_secs(5));
    }

    #[test]
    fn the_default_is_the_neon_profile() {
        // `Default` exists so a bare `PoolSettings::default()` cannot quietly resurrect the
        // oversized pool.
        assert_eq!(PoolSettings::default(), PoolSettings::for_neon());
    }

    #[test]
    fn the_oversized_pool_is_rejected_by_the_invariant() {
        // The exact `master` configuration, asserted to fail. If this ever passes, the
        // invariant is not doing its job.
        let master = PoolSettings {
            max_connections: 20,
            min_connections: 2,
            acquire_timeout: Duration::from_secs(3),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
            test_before_acquire: true,
        };

        assert!(
            !master.fits_neon(),
            "§3.5 says 20 connections and 30 minutes is wrong for Neon"
        );
    }

    #[test]
    fn a_pool_whose_minimum_exceeds_its_maximum_is_rejected() {
        let inverted = PoolSettings {
            max_connections: 4,
            min_connections: 8,
            ..PoolSettings::for_neon()
        };

        assert!(!inverted.fits_neon());
    }
}
