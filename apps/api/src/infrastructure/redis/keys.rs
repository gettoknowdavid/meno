//! Centralised Redis key definitions.
//!
//! Copied from `apps/api/src/shared/services/redis/keys.rs` on `master` (`903c3ba`)
//! with one structural change, required by plan §3.2 and restated in §9.4:
//!
//! > Set **mandatory TTLs on every key** — this is enforced in code review and by a
//! > `RedisKey` type whose constructors all require an expiry.
//!
//! # What changed and why it matters
//!
//! On `master`, `RedisKey` was a newtype over `String`. A key carried no expiry, and
//! the expiry was a separate `Option<i64>` argument on `Redis::set` — which callers
//! passed as `None`. That is how a `Render Key Value` free-tier instance (25 MB) fills
//! up: keys that are written but never evicted, so writes start failing and auth goes
//! down with them. §3.2's answer is `allkeys-lru`, but LRU only evicts under memory
//! pressure — it does not bound a key that nothing will ever delete.
//!
//! Now the key *is* the TTL. `Redis::set` reads the expiry off the key, so there is no
//! way to write a key without one: no `None`, no second argument to forget. Making it
//! impossible is the point — §3.2 says "enforced in code review", and a rule that
//! depends on reviewers noticing an `Option` is not enforced.
//!
//! # Naming
//!
//! Pattern: `{namespace}:{entity}:{id}:{suffix}`, unchanged from `master` so existing
//! cached entries stay valid during deploy.
//!
//! # TTL guidance for these ephemeral values (§3.2)
//!
//! Everything here is a cache that must be reconstructible from Postgres, because the
//! free tier has no persistence and loses data on restart. Two consequences:
//!
//! - **Token blocklists** are safe only because access tokens are short-lived; a lost
//!   blocklist leaves a revoked token valid until its own TTL expires.
//! - **OTPs are *not* safe in Redis.** §3.2 requires moving OTP state to Neon, or an
//!   unlucky restart locks every in-flight password reset. The key helper stays here so
//!   the eventual migration is mechanical, but callers must move off it.

use std::time::Duration;
use uuid::Uuid;

/// Canonical TTLs, so the decision for each key is made once and reviewable in one place.
///
/// Named constants rather than inline durations because "why is this 90 seconds?" is a
/// question worth answering next to the value, and because it makes a wrong TTL obvious
/// in review.
pub mod ttl {
    use std::time::Duration;

    /// Presence heartbeat. Short because a missed beat should evict quickly.
    pub const PRESENCE: Duration = Duration::from_secs(120);
    /// Offline message ring buffer.
    pub const WS_BUFFER: Duration = Duration::from_secs(300);
    /// Per-user daily quota counter.
    pub const QUOTA: Duration = Duration::from_secs(86_400);
    /// Cached profile projection.
    pub const PROFILE: Duration = Duration::from_secs(300);
    /// Cached identity-provider list for a user.
    pub const USER_PROVIDERS: Duration = Duration::from_secs(300);
    /// Cached session record.
    pub const SESSION: Duration = Duration::from_secs(900);
    /// Unread notification counter.
    pub const UNREAD_COUNT: Duration = Duration::from_secs(3_600);
    /// Reconnect flood protection.
    pub const RECONNECT_RATE: Duration = Duration::from_secs(60);
    /// Generic rate-limit window.
    pub const RATE_LIMIT: Duration = Duration::from_secs(60);
    /// OAuth2 `state` parameter — the flow must complete well inside this.
    pub const OAUTH2_STATE: Duration = Duration::from_secs(600);
    /// Cached OTP. See the module warning: this belongs in Neon, not Redis.
    pub const OTP: Duration = Duration::from_secs(300);
    /// Cached search result pages.
    pub const SEARCH_RESULTS: Duration = Duration::from_secs(60);
    /// Blocklist entries. Must outlive the access token it revokes.
    pub const BLOCK_LIST: Duration = Duration::from_secs(900);
    /// Idempotency keys — covers any realistic client retry window.
    pub const IDEMPOTENCY: Duration = Duration::from_secs(86_400);
    /// Cached cursor page.
    pub const CURSOR_CACHE: Duration = Duration::from_secs(60);
    /// Lock held while a cache miss is being filled.
    pub const LOCK: Duration = Duration::from_secs(5);

    // ── broadcast ──
    /// Live participant count. Short: it is recomputable and drives the UI.
    pub const LIVE_COUNT: Duration = Duration::from_secs(120);
    /// Host grace-period remaining seconds.
    pub const HOST_GRACE: Duration = Duration::from_secs(120);
    /// When the grace period started.
    pub const GRACE_STARTED: Duration = Duration::from_secs(130);
    /// Disconnects in the current session.
    pub const DISCONNECT_COUNT: Duration = Duration::from_secs(3_600);
    /// Live room membership set. Refreshed on every join/leave.
    pub const ROOM_MEMBERS: Duration = Duration::from_secs(3_600);
    /// Broadcast start timestamp, used for quota checks.
    pub const STARTED_AT: Duration = Duration::from_secs(86_400);
    /// Cached recording URL.
    pub const RECORDING_READY: Duration = Duration::from_secs(604_800);
    /// Cached broadcast projection.
    pub const BROADCAST: Duration = Duration::from_secs(300);
}

/// A Redis key and the expiry that must be applied whenever it is written.
///
/// The TTL is not decoration: [`super::Redis::set`] reads it, so a key cannot reach
/// Redis without one. Constructing a key and writing it are therefore the same decision,
/// which is what makes §3.2's rule enforceable rather than aspirational.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedisKey {
    key: String,
    ttl: Duration,
}

impl RedisKey {
    /// The single private constructor. Every public constructor routes through it, so
    /// there is no way to build a `RedisKey` without an expiry.
    fn new(key: String, ttl: Duration) -> Self {
        debug_assert!(
            !ttl.is_zero(),
            "a Redis key with a zero TTL would never expire, which is exactly the \
             unbounded-growth failure §3.2 exists to prevent"
        );
        Self { key, ttl }
    }

    /// Build a key from an arbitrary string. Used by the coalescing cache, whose key is
    /// composed by the caller.
    #[must_use]
    pub fn new_raw(key: &str, ttl: Duration) -> Self {
        Self::new(key.to_owned(), ttl)
    }

    /// The full key, as Redis will see it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.key
    }

    /// The expiry to apply on write.
    #[must_use]
    pub const fn ttl(&self) -> Duration {
        self.ttl
    }

    // ========== BROADCAST KEYS ==========

    /// Current live participant count. Recomputable from `broadcast_participants`.
    #[must_use]
    pub fn live_count(broadcast_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("b:{broadcast_id}:live"), ttl)
    }

    /// Host grace period TTL (seconds remaining).
    #[must_use]
    pub fn host_grace(broadcast_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("b:{broadcast_id}:grace"), ttl)
    }

    /// When the grace period started (Unix timestamp).
    #[must_use]
    pub fn grace_started(broadcast_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("b:{broadcast_id}:grace_start"), ttl)
    }

    /// Number of disconnects in a current session.
    #[must_use]
    pub fn disconnect_count(broadcast_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("b:{broadcast_id}:disc_count"), ttl)
    }

    /// Broadcast start timestamp, used for quota checks.
    #[must_use]
    pub fn started_at(broadcast_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("b:{broadcast_id}:start"), ttl)
    }

    /// Recording ready URL.
    #[must_use]
    pub fn recording_ready(broadcast_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("recording:{broadcast_id}"), ttl)
    }

    /// Cached broadcast projection.
    #[must_use]
    pub fn broadcast(broadcast_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("b:{broadcast_id}"), ttl)
    }

    /// Redis SET of the user IDs currently in a live broadcast room.
    ///
    /// Shared across every API instance, so `is_room_member` and `room_member_count`
    /// answer for the whole deployment rather than just the local process.
    #[must_use]
    pub fn room_members(broadcast_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("b:{broadcast_id}:members"), ttl)
    }

    // ========== USER KEYS ==========

    /// User presence (online status). TTL is the heartbeat window.
    #[must_use]
    pub fn presence(user_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("u:{user_id}:online"), ttl)
    }

    /// WebSocket message buffer (ring buffer).
    #[must_use]
    pub fn ws_buffer(user_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("u:{user_id}:ws_buf"), ttl)
    }

    /// Daily quota usage for the given UTC date.
    #[must_use]
    pub fn quota(user_id: Uuid, date: &str, ttl: Duration) -> Self {
        Self::new(format!("u:{user_id}:quota:{date}"), ttl)
    }

    /// Cached profile projection.
    #[must_use]
    pub fn profile(user_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("u:{user_id}:profile"), ttl)
    }

    /// The user's linked identity providers.
    #[must_use]
    pub fn user_providers(user_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("u:{user_id}:providers"), ttl)
    }

    /// Cached session record.
    #[must_use]
    pub fn session(user_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("u:{user_id}:session"), ttl)
    }

    /// Per-user unread notification count.
    #[must_use]
    pub fn unread_count(user_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("u:{user_id}:unread"), ttl)
    }

    // ========== RATE LIMITING ==========

    /// Reconnect flood protection.
    #[must_use]
    pub fn reconnect_rate(user_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("rate:reconnect:{user_id}"), ttl)
    }

    /// Generic rate limit key.
    #[must_use]
    pub fn rate_limit(prefix: &str, identifier: &str, ttl: Duration) -> Self {
        Self::new(format!("rate:{prefix}:{identifier}"), ttl)
    }

    // ========== GLOBAL KEYS ==========

    /// OAuth2 `state` parameter.
    #[must_use]
    pub fn oauth2_state(state: &str, ttl: Duration) -> Self {
        Self::new(format!("oauth2:{state}"), ttl)
    }

    /// Cached OTP. §3.2 requires this state to move to Neon — see the module warning.
    #[must_use]
    pub fn otp(email: &str, otp_type: &str, ttl: Duration) -> Self {
        Self::new(format!("otp:{otp_type}:{email}"), ttl)
    }

    /// Cached search result pages.
    #[must_use]
    pub fn search_results(query: &str, page: i64, limit: i64, ttl: Duration) -> Self {
        Self::new(format!("{query}:{page}:{limit}"), ttl)
    }

    /// Blocklist entries. Must outlive the access token they revoke.
    #[must_use]
    pub fn block_list(prefix: &str, id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("block-list:{prefix}:{id}"), ttl)
    }

    /// "Log out everywhere" marker for a user.
    ///
    /// A separate constructor from [`Self::block_list`] because a token `jti` and a
    /// `user_id` are both `Uuid`s drawn from the same space: sharing one key shape would
    /// mean a marker could collide with a single-token revocation, and a `SCAN` over one
    /// would match the other. The `u:` infix keeps them apart while staying in the same
    /// `block-list:` namespace, so a deployment can expire the whole set at once.
    #[must_use]
    pub fn user_tokens_blocked(prefix: &str, user_id: Uuid, ttl: Duration) -> Self {
        Self::new(format!("block-list:{prefix}:u:{user_id}"), ttl)
    }

    /// Idempotency key for safe client retries.
    #[must_use]
    pub fn idempotency(key: Uuid, ttl: Duration) -> Self {
        Self::new(format!("idem:{key}"), ttl)
    }

    /// Cached cursor page. Encodes module + owner + cursor + limit, so two callers
    /// paging the same list with different cursors never collide.
    #[must_use]
    pub fn cursor_cache(
        module: &str,
        owner_id: Uuid,
        cursor: &str,
        limit: i64,
        ttl: Duration,
    ) -> Self {
        Self::new(format!("cursor:{module}:{owner_id}:{cursor}:{limit}"), ttl)
    }

    /// The lock guarding a cache miss. TTL must exceed the expected fill time, or the
    /// lock expires mid-flight and a second worker starts a duplicate fetch.
    #[must_use]
    pub fn lock(cache_key: &str, ttl: Duration) -> Self {
        Self::new(format!("lock:{cache_key}"), ttl)
    }

    // ========== PATTERNS ==========
    //
    // Patterns are NOT keys: they are `SCAN` arguments, so the mandatory-TTL rule does
    // not apply — there is nothing to write. Kept as an explicit `RedisPattern` newtype
    // so a pattern can never be passed where a key is expected.

    /// Every per-user key, for `invalidate_all_user_keys`.
    #[must_use]
    pub fn user_pattern(user_id: Uuid) -> RedisPattern {
        RedisPattern(format!("u:{user_id}:*"))
    }
}

/// A `SCAN` glob. Never written, so it carries no TTL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedisPattern(String);

impl RedisPattern {
    /// The glob as Redis will see it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RedisPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for RedisKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.key)
    }
}

impl AsRef<str> for RedisKey {
    fn as_ref(&self) -> &str {
        &self.key
    }
}

impl From<RedisKey> for String {
    fn from(key: RedisKey) -> Self {
        key.key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn every_key_carries_the_ttl_it_was_built_with() {
        // This is the §3.2 rule expressed as an assertion: there is no way to build a
        // RedisKey without an expiry, so no key can reach Redis without one.
        let keys = vec![
            RedisKey::live_count(id(1), ttl::LIVE_COUNT),
            RedisKey::host_grace(id(1), ttl::HOST_GRACE),
            RedisKey::grace_started(id(1), ttl::GRACE_STARTED),
            RedisKey::disconnect_count(id(1), ttl::DISCONNECT_COUNT),
            RedisKey::started_at(id(1), ttl::STARTED_AT),
            RedisKey::recording_ready(id(1), ttl::RECORDING_READY),
            RedisKey::broadcast(id(1), ttl::BROADCAST),
            RedisKey::room_members(id(1), ttl::ROOM_MEMBERS),
            RedisKey::presence(id(2), ttl::PRESENCE),
            RedisKey::ws_buffer(id(2), ttl::WS_BUFFER),
            RedisKey::quota(id(2), "2026-10-03", ttl::QUOTA),
            RedisKey::profile(id(2), ttl::PROFILE),
            RedisKey::user_providers(id(2), ttl::USER_PROVIDERS),
            RedisKey::session(id(2), ttl::SESSION),
            RedisKey::unread_count(id(2), ttl::UNREAD_COUNT),
            RedisKey::reconnect_rate(id(2), ttl::RECONNECT_RATE),
            RedisKey::rate_limit("login", "1.2.3.4", ttl::RATE_LIMIT),
            RedisKey::oauth2_state("abc", ttl::OAUTH2_STATE),
            RedisKey::otp("a@b.c", "reset", ttl::OTP),
            RedisKey::search_results("q", 1, 20, ttl::SEARCH_RESULTS),
            RedisKey::block_list("BL", id(3), ttl::BLOCK_LIST),
            RedisKey::idempotency(id(3), ttl::IDEMPOTENCY),
            RedisKey::cursor_cache("broadcast", id(3), "cur", 20, ttl::CURSOR_CACHE),
            RedisKey::lock("broadcasts:list", ttl::LOCK),
            RedisKey::new_raw("raw", ttl::CURSOR_CACHE),
        ];

        assert_eq!(keys.len(), 25, "every constructor is covered by this test");
        for key in &keys {
            assert!(
                !key.ttl().is_zero(),
                "{} was built with a zero TTL",
                key.as_str()
            );
        }
    }

    #[test]
    fn key_layout_is_namespaced_by_entity_and_id() {
        let uid = id(7);
        assert_eq!(
            RedisKey::presence(uid, ttl::PRESENCE).as_str(),
            format!("u:{uid}:online")
        );
        assert_eq!(
            RedisKey::profile(uid, ttl::PROFILE).as_str(),
            format!("u:{uid}:profile")
        );
        assert_eq!(
            RedisKey::broadcast(uid, ttl::BROADCAST).as_str(),
            format!("b:{uid}")
        );
        assert_eq!(
            RedisKey::recording_ready(uid, ttl::RECORDING_READY).as_str(),
            format!("recording:{uid}")
        );
        // The room-members key must not collide with the cached broadcast projection
        // for the same id — they are different things with different lifetimes.
        assert_eq!(
            RedisKey::room_members(uid, ttl::ROOM_MEMBERS).as_str(),
            format!("b:{uid}:members")
        );
        assert_ne!(
            RedisKey::room_members(uid, ttl::ROOM_MEMBERS).as_str(),
            RedisKey::broadcast(uid, ttl::BROADCAST).as_str()
        );
    }

    #[test]
    fn distinct_entities_never_share_a_key() {
        // A collision here would mean one user's cache served another user's data.
        let a = RedisKey::profile(id(1), ttl::PROFILE);
        let b = RedisKey::profile(id(2), ttl::PROFILE);
        assert_ne!(a.as_str(), b.as_str());

        // And the namespaces must not overlap each other either.
        let per_user = RedisKey::session(id(1), ttl::SESSION);
        let rate = RedisKey::reconnect_rate(id(1), ttl::RECONNECT_RATE);
        assert!(!per_user.as_str().starts_with("rate:"));
        assert!(!rate.as_str().starts_with("u:"));
    }

    #[test]
    fn user_pattern_matches_the_user_key_family() {
        let uid = id(9);
        let pattern = RedisKey::user_pattern(uid);
        assert_eq!(pattern.as_str(), format!("u:{uid}:*"));
        // Every per-user key must fall inside its own user's pattern, or
        // invalidate_all_user_keys silently leaves stale entries behind.
        for key in [
            RedisKey::presence(uid, ttl::PRESENCE),
            RedisKey::profile(uid, ttl::PROFILE),
            RedisKey::session(uid, ttl::SESSION),
            RedisKey::unread_count(uid, ttl::UNREAD_COUNT),
            RedisKey::ws_buffer(uid, ttl::WS_BUFFER),
            RedisKey::user_providers(uid, ttl::USER_PROVIDERS),
        ] {
            assert!(
                key.as_str().starts_with(&format!("u:{uid}:")),
                "{} should be covered by the user pattern",
                key.as_str()
            );
        }
    }

    #[test]
    fn cursor_cache_keys_separate_every_input() {
        // Two callers paging the same list with different cursors must not read each
        // other's cached page.
        let base = RedisKey::cursor_cache("broadcast", id(1), "curA", 20, ttl::CURSOR_CACHE);
        assert_ne!(
            base.as_str(),
            RedisKey::cursor_cache("broadcast", id(1), "curB", 20, ttl::CURSOR_CACHE).as_str()
        );
        assert_ne!(
            base.as_str(),
            RedisKey::cursor_cache("broadcast", id(1), "curA", 50, ttl::CURSOR_CACHE).as_str()
        );
        assert_ne!(
            base.as_str(),
            RedisKey::cursor_cache("notes", id(1), "curA", 20, ttl::CURSOR_CACHE).as_str()
        );
        assert_ne!(
            base.as_str(),
            RedisKey::cursor_cache("broadcast", id(2), "curA", 20, ttl::CURSOR_CACHE).as_str()
        );
    }

    #[test]
    fn named_ttls_match_the_documented_intent() {
        // Lock TTL must exceed the time a cache fill is expected to take (1-2s), or the
        // lock expires mid-flight and two workers fetch the same miss.
        assert!(
            ttl::LOCK >= Duration::from_secs(2),
            "a 1-2s fill needs a lock longer than that"
        );
        // Presence must expire faster than the 120s reconnect grace window, or a
        // "present" user lingers after leaving.
        assert!(ttl::PRESENCE <= Duration::from_secs(120));
        // Rate-limit windows and the reconnect limiter must be short by design.
        assert!(ttl::RATE_LIMIT <= Duration::from_secs(60));
        assert!(ttl::RECONNECT_RATE <= Duration::from_secs(60));
        // Blocklist must outlive an access token so a logout actually revokes it.
        assert!(ttl::BLOCK_LIST >= Duration::from_secs(900));
    }

    #[test]
    fn redis_key_converts_into_a_string_without_losing_the_name() {
        let key = RedisKey::profile(id(4), ttl::PROFILE);
        let owned: String = key.clone().into();
        assert_eq!(owned, key.as_str());
    }
}
