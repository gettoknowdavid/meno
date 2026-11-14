//! Connection-pool sizing, and why the old numbers were wrong.
//!
//! # The defect this fixes
//!
//! `master`'s pool was hard-coded to `max_connections: 20` with a `max_lifetime` of 30
//! minutes. Both are wrong for *any* managed Postgres reached through a **connection
//! pooler** — Neon, RDS Proxy, Cloud SQL Auth Proxy, Supabase's pooler, or a PgBouncer in
//! front of a container. Plan §3.5 names the numbers and the reason:
//!
//! > With PgBouncer, use `max_connections: 5–10` and a *short* `max_lifetime` (~5 min) so
//! > the pooler can recycle connections.
//!
//! Both halves matter, and for opposite reasons:
//!
//! - **`max_connections` too high** — a pooler multiplexes many client connections onto a
//!   small number of real ones. A client pool of 20 in front of it is not 20 connections'
//!   worth of throughput; it is 20 contenders for the pooler's server-side queue, and on a
//!   small plan that queue is what returns "too many connections".
//! - **`max_lifetime` too long** — a 30-minute connection is held open across the pooler's
//!   own recycling window, so the pool keeps handing out connections the pooler has
//!   already discarded. The symptom is an intermittent error on the first query after a
//!   quiet period, which is exactly the kind of bug that gets filed as "the API randomly
//!   500s".
//!
//! # Why this is vendor-agnostic on purpose
//!
//! The sizing is a property of **the endpoint in front of the pooler**, not of who
//! operates it. Nothing here names a provider, and the connection string — including which
//! host, port and pooler it points at — comes from `DATABASE_URL` alone. Moving to a
//! different managed Postgres is a change to one environment variable, with no code
//! change, which is the property that makes the constants safe to hard-code.
//!
//! That is why these are constants and not environment variables: they describe the shape
//! of the endpoint, not this installation. A self-hosted Postgres reached **directly**
//! (no pooler in the path) would want a larger pool and a longer lifetime — and that is a
//! genuinely different deployment shape, which is why [`PoolSettings`] is a struct with a
//! public constructor rather than a pile of `const`s scattered through the connector.
//!
//! [`PoolSettings::pooled`] is the default. The alternative is one line at the call site.

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

/// How the pool is sized.
///
/// The fields mirror the underlying `PgPoolOptions` knobs one-for-one; this type exists to
/// give them a name, a home, and an invariant.
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
    ///
    /// Kept short so the pooler underneath can recycle connections without the client
    /// pool holding one open across that window.
    pub max_lifetime: Duration,

    /// Whether a connection is verified with a ping before being handed out.
    pub test_before_acquire: bool,
}

impl PoolSettings {
    /// The largest `max_connections` that suits a pooler-backed endpoint.
    ///
    /// §3.5's 5–10 band, named so the invariant below can state it without repeating the
    /// number.
    pub const MAX_CONNECTIONS_LIMIT: u32 = 10;

    /// The longest `max_lifetime` that suits a pooler-backed endpoint.
    ///
    /// §3.5's "~5 min".
    pub const MAX_LIFETIME_TARGET: Duration = Duration::from_secs(300);

    /// Sizing for a Postgres endpoint behind a connection pooler.
    ///
    /// This is the default and the right answer for every managed Postgres that hands out
    /// a pooled connection string — which is the normal way to deploy one.
    ///
    /// `max_connections` sits at the top of §3.5's 5–10 band rather than the middle: the
    /// worker binary runs fan-out jobs that would otherwise queue behind a small pool, and
    /// 10 is still comfortably inside a pooler's budget.
    ///
    /// `acquire_timeout` is 5 seconds rather than `master`'s 3. With a deliberately small
    /// pool, 3 seconds is short enough that a burst of concurrency fails requests that
    /// would have been served a moment later — the pool is small by design, so waiting is
    /// the correct behaviour, not a fault.
    #[must_use]
    pub const fn pooled() -> Self {
        Self {
            max_connections: 10,
            min_connections: 2,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Self::MAX_LIFETIME_TARGET,
            // Kept on. `master` had it and it is the difference between a stale pooled
            // connection failing a request and being discarded first; the short
            // `max_lifetime` is what keeps that check cheap.
            test_before_acquire: true,
        }
    }

    /// Sizing for a Postgres endpoint reached directly, with no pooler in the path.
    ///
    /// Offered so the default is a choice rather than an assumption. A direct connection
    /// *is* the connection, so there is nothing to multiplex and the pool may be as large
    /// as the server tolerates; `max_lifetime` can also be longer, because there is no
    /// intermediary recycling connections out from under us.
    ///
    /// Use this only when `DATABASE_URL` points at the database itself. If you are not
    /// sure, you are probably on [`Self::pooled`].
    #[must_use]
    pub const fn direct() -> Self {
        Self {
            max_connections: 20,
            min_connections: 2,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
            test_before_acquire: true,
        }
    }

    /// Whether these settings are consistent and within the pooler budget.
    ///
    /// Exposed so the invariant is asserted in a test rather than trusted to a comment
    /// that nobody re-reads when a number changes. Note this is deliberately a property of
    /// the *settings*, not of a provider: [`Self::direct`] is a legitimate configuration
    /// and is expected to fail it, because it sits outside the pooler budget on purpose.
    #[must_use]
    pub fn fits_pooler(&self) -> bool {
        self.max_connections <= Self::MAX_CONNECTIONS_LIMIT
            && self.min_connections <= self.max_connections
            && self.max_lifetime <= Self::MAX_LIFETIME_TARGET
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
    /// [`Self::pooled`], so a bare `PoolSettings::default()` cannot quietly resurrect the
    /// oversized pool.
    fn default() -> Self {
        Self::pooled()
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the pool sizing. These are what stop §3.5's correction being undone by
    //! someone "restoring" `master`'s numbers.

    use super::*;

    #[test]
    fn the_default_profile_fits_a_pooler() {
        assert!(
            PoolSettings::default().fits_pooler(),
            "the default must be safe for a pooled endpoint"
        );
    }

    #[test]
    fn max_connections_is_inside_the_documented_band() {
        let settings = PoolSettings::pooled();

        assert!(
            (5..=10).contains(&settings.max_connections),
            "5-10 connections for a pooled endpoint, got {}",
            settings.max_connections
        );
        assert!(settings.max_connections <= PoolSettings::MAX_CONNECTIONS_LIMIT);
    }

    #[test]
    fn max_lifetime_is_five_minutes_not_thirty() {
        // The regression this file exists to prevent: `master`'s 30 minutes kept
        // connections alive across the pooler's recycling window.
        let settings = PoolSettings::pooled();

        assert_eq!(settings.max_lifetime, Duration::from_secs(300));
        assert!(
            settings.max_lifetime < Duration::from_secs(30 * 60),
            "a 30-minute lifetime is what a pooler punishes"
        );
    }

    #[test]
    fn the_minimum_never_exceeds_the_maximum() {
        // Not hypothetical: a pool with `min > max` silently behaves like `max`, so the
        // minimum would be a lie rather than a setting.
        for settings in [PoolSettings::pooled(), PoolSettings::direct()] {
            assert!(settings.min_connections <= settings.max_connections);
        }
    }

    #[test]
    fn acquiring_has_enough_room_for_a_deliberately_small_pool() {
        // 3 seconds with a small pool turns a burst of concurrency into request failures
        // that a slightly larger pool would have absorbed.
        assert!(PoolSettings::pooled().acquire_timeout >= Duration::from_secs(5));
    }

    #[test]
    fn the_default_is_the_pooled_profile() {
        // `Default` exists so a bare `PoolSettings::default()` cannot quietly resurrect the
        // oversized pool.
        assert_eq!(PoolSettings::default(), PoolSettings::pooled());
    }

    #[test]
    fn the_two_profiles_differ_where_the_pooler_demands_it() {
        // A direct connection may be wider and longer-lived; a pooled one may not. This is
        // the difference the whole file is about.
        let pooled = PoolSettings::pooled();
        let direct = PoolSettings::direct();

        assert!(direct.max_connections > pooled.max_connections);
        assert!(direct.max_lifetime > pooled.max_lifetime);
    }

    #[test]
    fn the_oversized_pool_is_rejected_by_the_invariant() {
        // The exact `master` configuration, asserted to fail against the pooled profile.
        // If this ever passes, the invariant is not doing its job.
        let master = PoolSettings {
            max_connections: 20,
            min_connections: 2,
            acquire_timeout: Duration::from_secs(3),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
            test_before_acquire: true,
        };

        assert!(
            !master.fits_pooler(),
            "20 connections and a 30-minute lifetime is wrong behind a pooler"
        );
    }

    #[test]
    fn a_direct_profile_is_rejected_by_the_pooler_invariant_and_that_is_expected() {
        // Documents that `direct()` failing `fits_pooler()` is not a bug: the invariant is
        // about the pooled shape, and `direct()` is deliberately outside it.
        assert!(!PoolSettings::direct().fits_pooler());
        assert!(
            PoolSettings::direct().min_connections <= PoolSettings::direct().max_connections,
            "even outside the budget, the profile must be internally consistent"
        );
    }

    #[test]
    fn a_pool_whose_minimum_exceeds_its_maximum_is_rejected() {
        let inverted = PoolSettings {
            max_connections: 4,
            min_connections: 8,
            ..PoolSettings::pooled()
        };

        assert!(!inverted.fits_pooler());
    }
}
