//! Tuning constants for the infrastructure adapters.
//!
//! Trimmed from `apps/api/src/shared/constants.rs` on `master` (`903c3ba`).
//!
//! # What was removed, and why it is not coming back
//!
//! The file also carried eleven `TTL_*_SECS` constants (`TTL_10_SECS` … `TTL_3600_SECS`),
//! four `*_CACHE_PREFIX` constants, `RATE_LIMIT_PREFIX` and `MAX_LOGIN_ATTEMPTS`. **None of
//! them had a single use**, and two of those groups contradicted the plan:
//!
//! - **The TTL zoo duplicated [`crate::infrastructure::redis::keys::ttl`].** §3.2 asks for
//!   *mandatory* TTLs on every key, and enforces it with a `RedisKey` whose constructors all
//!   require a `Duration`. That module is the canonical list — one named constant per key, each
//!   next to a sentence explaining why it is that value. A parallel `TTL_300_SECS` in a
//!   different file is a second answer to the same question, and the two will disagree.
//! - **The cache prefixes duplicated key constructors that already own their layout.**
//!   `RedisKey::profile` writes `u:{id}:profile`; there is nothing for `USER_CACHE_PREFIX` to
//!   namespace. A free-floating prefix is a second way to spell a key, and the way that loses
//!   the mandatory TTL.
//!
//! `RATE_LIMIT_PREFIX` went the other way: it *was* used, by the limiter on `master`, but that
//! limiter built its key by `format!` and so bypassed `RedisKey`. The rewritten limiter takes
//! its namespace from a [`crate::middleware::rate_limit::LimitPolicy`] and builds every key
//! through `RedisKey::rate_limit`, so the constant has no job left.
//!
//! # What stayed, and the rule for adding more
//!
//! A constant belongs here when it tunes **behaviour of a subsystem** — how many connections
//! one user may hold, how long a loser retries for a lock. It does **not** belong here when
//! it is a **cache lifetime**, because those live in `redis::keys::ttl` next to the key they
//! belong to, and the two lists are asserted to agree where they overlap.
//!
//! The module header on `master` claimed this file was "copied verbatim" and that a later step
//! would move the real one. That note described a migration that had already happened, and
//! pointed at a file that no longer exists.

// ========== REDIS KEY NAMESPACES ==========

/// Namespace for access-token revocations.
///
/// A prefix rather than a bare key so `RedisKey::block_list`'s first argument is required: a
/// revocation and, say, a session revocation written to the same namespace would collide, and
/// a `SCAN` for one kind of entry would match another's. See
/// [`crate::middleware::auth::blocklist_key`].
pub const BLOCKLIST_PREFIX: &str = "BL";

// ========== CACHE COALESCING ==========

/// How long a losing coalesced fetch waits between attempts.
///
/// Paired with [`LOCK_MAX_RETRIES`] to give the total wait budget; the relationship is asserted
/// in the tests below, because "40 retries" and "2 seconds" are only meaningful together.
pub const LOCK_RETRY_MS: u64 = 50;

/// How many times a losing coalesced fetch retries before fetching directly.
///
/// Bounded on purpose: the winner may have crashed mid-fetch, leaving a lock that outlives it.
/// After the budget, falling through to a direct fetch is better than returning an error — a
/// slow response beats a failed one, and Postgres is the source of truth either way.
pub const LOCK_MAX_RETRIES: u32 = 40;

/// [`LOCK_MAX_RETRIES`] × [`LOCK_RETRY_MS`], the total a loser waits before giving up.
#[must_use]
pub const fn lock_budget_ms() -> u64 {
    LOCK_RETRY_MS * LOCK_MAX_RETRIES as u64
}

// ========== WEBSOCKET CONSTANTS ==========

/// How many concurrent connections one user may hold on a single instance.
///
/// A user typically has two (web + mobile), so this is headroom rather than a hard product
/// limit — its job is to stop one misbehaving client from pinning thousands of `mpsc` channels
/// and a slice of the 25 MB Redis budget (§3.2).
pub const MAX_WS_CONNECTIONS_PER_USER: usize = 5;

/// Depth of the per-connection outbound channel.
///
/// Bounded so a client that stops reading its socket cannot make the server buffer without
/// limit; once full, sends are dropped rather than awaited — §9.4's "back pressure is a
/// correctness concern, not a performance one".
pub const MESSAGE_BUFFER_SIZE: usize = 128;

/// How many offline messages are retained per user in the Redis ring buffer.
///
/// Bounded because the buffer lives in the same ephemeral, memory-capped instance as
/// everything else. A user who is away longer than this reconnects to a gap, which the client
/// reconciles by refetching — an unbounded buffer would instead evict live keys belonging to
/// everyone else.
pub const OFFLINE_MESSAGE_HISTORY: usize = 50;

/// TTL on a user's offline message ring buffer, in seconds.
///
/// Must exceed the grace period a disconnected client is expected to be back within, or the
/// replay-on-reconnect path finds nothing waiting. Equal to [`crate::infrastructure::redis::keys::ttl::WS_BUFFER`] — asserted in
/// the tests, because two sources of truth for one duration is how they come to disagree.
pub const MESSAGE_BUFFER_TTL_SECS: i64 = 300;

#[cfg(test)]
mod tests {
    //! Tests for the invariants that make these numbers safe.
    //!
    //! A constant is a decision someone will ask about later — "why is this 40?" — so each
    //! assertion here records *why* the value cannot be changed freely, which is more useful
    //! than restating the number.

    use super::*;
    use crate::infrastructure::redis::keys::ttl;
    use crate::middleware::rate_limit::{FailureMode, LimitPolicy};

    // ── no unbounded growth: §3.2 ──────────────────────────────────────────

    #[test]
    fn every_ttl_this_module_owns_is_positive() {
        // §3.2: a zero (or negative) TTL means the key never expires, which is the unbounded
        // growth that fills a free instance and takes auth down with it.
        const { assert!(MESSAGE_BUFFER_TTL_SECS > 0) };
    }

    #[test]
    fn the_buffer_ttl_agrees_with_the_canonical_key_ttl() {
        // The drift guard. `redis::keys::ttl::WS_BUFFER` is where the plan puts canonical
        // lifetimes (§3.2), and this constant is what the socket code reads; if one moves
        // without the other, a reconnect finds an empty buffer that should have been full.
        assert_eq!(
            MESSAGE_BUFFER_TTL_SECS,
            i64::try_from(ttl::WS_BUFFER.as_secs()).expect("a positive duration")
        );
    }

    // ── bounded memory: §9.4 ───────────────────────────────────────────────

    #[test]
    fn every_buffer_size_is_non_zero() {
        // A zero channel capacity makes `mpsc::channel` panic; a zero history writes a
        // `LPUSH` that nothing can ever read back.
        const {
            assert!(MESSAGE_BUFFER_SIZE > 0);
            assert!(OFFLINE_MESSAGE_HISTORY > 0);
            assert!(MAX_WS_CONNECTIONS_PER_USER > 0);
        }
    }

    #[test]
    fn the_retained_history_fits_inside_the_outbound_buffer() {
        // `LPUSH` + `LTRIM` keeps `OFFLINE_MESSAGE_HISTORY` entries in Redis, while a single
        // connection can push at most `MESSAGE_BUFFER_SIZE` before sends are dropped. A history
        // larger than the buffer would mean the buffer, not the history, is what bounds delivery
        // — so the ordering below is a real constraint, not an accident.
        // Stated again in numbers, because the assertion message below cannot interpolate:
        // 50 retained entries have to fit inside a buffer of 128.
        const {
            assert!(
                OFFLINE_MESSAGE_HISTORY < MESSAGE_BUFFER_SIZE,
                "the retained history must fit inside one connection's outbound buffer"
            )
        };
    }

    #[test]
    fn the_per_user_connection_cap_is_above_the_typical_device_count() {
        // A user typically has web + mobile. A cap at or below that would reject a legitimate
        // second device rather than a misbehaving client.
        const {
            assert!(
                MAX_WS_CONNECTIONS_PER_USER >= 2,
                "a user with web and mobile must both be able to connect"
            )
        };
    }

    // ── the coalescing budget ──────────────────────────────────────────────

    #[test]
    fn the_retry_budget_is_bounded_and_short() {
        // "40 retries" only means something next to "50ms apart". The product is the number
        // an operator needs when a slow request is reported: it is how long a loser waits
        // before fetching directly.
        assert_eq!(lock_budget_ms(), 2_000);
        assert!(
            u128::from(lock_budget_ms()) < ttl::LOCK.as_millis().max(1),
            "a loser must not wait longer than the lock it is waiting on, or it waits on a \\
             lock whose holder has already given up"
        );
    }

    #[test]
    fn the_retry_loop_cannot_spin() {
        // A zero interval would turn a bounded wait into a busy loop against Redis.
        const {
            assert!(LOCK_RETRY_MS > 0);
            assert!(LOCK_MAX_RETRIES > 0);
        }
    }

    // ── key namespacing ────────────────────────────────────────────────────

    #[test]
    fn the_blocklist_prefix_is_short_uppercase_and_unambiguous() {
        // It appears in every revocation key, so it is a readability and collision-safety
        // property as much as a convention. Uppercase keeps it visually distinct from the
        // `u:`/`b:` entity namespaces a caller might otherwise confuse it with.
        assert_eq!(BLOCKLIST_PREFIX, "BL");
        assert!(
            !BLOCKLIST_PREFIX.contains(':'),
            "a prefix must not contain the separator"
        );
        assert!(
            BLOCKLIST_PREFIX.chars().all(|c| c.is_ascii_uppercase()),
            "the prefix must stay uppercase so it reads as a namespace, not an id"
        );
    }

    #[test]
    fn a_revocation_key_is_prefixed_and_bounded() {
        // The end-to-end version of the two tests above: what actually reaches Redis.
        let key = crate::middleware::auth::blocklist_key(uuid::Uuid::from_u128(7));

        assert!(key.as_str().contains(BLOCKLIST_PREFIX));
        assert!(!key.ttl().is_zero());
    }

    // ── the limiter's namespaces come from the policy, not from here ──────

    #[test]
    fn the_limiter_namespaces_are_lowercase_and_distinct() {
        // §4.4's tiered limits only work if the tiers are separate buckets. The namespaces
        // live on `LimitPolicy` rather than here, so this asserts the property from wherever
        // it is actually decided.
        let namespaces = [
            LimitPolicy::auth().namespace,
            LimitPolicy::broadcast_join().namespace,
            LimitPolicy::read().namespace,
            LimitPolicy::write().namespace,
        ];

        for (index, namespace) in namespaces.iter().enumerate() {
            assert!(!namespace.is_empty());
            assert_eq!(
                *namespace,
                namespace.to_lowercase(),
                "`{namespace}` must be lowercase to match the key layout"
            );
            assert!(
                !namespaces[index + 1..].contains(namespace),
                "`{namespace}` is used by two tiers, so they share a counter"
            );
        }
    }

    #[test]
    fn the_auth_tiers_are_the_ones_that_fail_closed() {
        // Recorded here because the reason is a capacity decision, not an error-handling one:
        // these are the endpoints whose absence of a limiter is worth an outage.
        assert_eq!(LimitPolicy::auth().on_backend_failure, FailureMode::Closed);
        assert_eq!(
            LimitPolicy::broadcast_join().on_backend_failure,
            FailureMode::Closed
        );
    }
}
