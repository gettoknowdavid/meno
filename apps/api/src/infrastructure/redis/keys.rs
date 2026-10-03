use uuid::Uuid;

/// Centralized Redis key definitions
///
/// All keys are defined here as static methods to ensure consistency.
/// Pattern: {namespace}:{entity}:{id}:{suffix}
#[derive(Debug, Clone)]
pub struct RedisKey(String);
impl RedisKey {
    pub(crate) fn new(key: String) -> Self {
        Self(key)
    }

    #[must_use]
    pub fn new_raw(s: &str) -> Self {
        Self(s.to_owned())
    }

    // ========== BROADCAST KEYS ==========

    /// Current live participant count
    /// TTL: None (deleted when broadcast ends)
    /// Type: i64
    #[must_use]
    pub fn live_count(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{broadcast_id}:live"))
    }

    /// Host grace period TTL (seconds remaining)
    /// TTL: Grace period duration (120s, 90s, etc.)
    /// Type: string (seconds as string)
    #[must_use]
    pub fn host_grace(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{broadcast_id}:grace"))
    }

    /// When `grace period` started (Unix timestamp)
    /// TTL: Grace period + 10s
    /// Type: i64
    #[must_use]
    pub fn grace_started(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{broadcast_id}:grace_start"))
    }

    /// Number of disconnects in a current session
    /// TTL: 1 hour
    /// Type: i64
    #[must_use]
    pub fn disconnect_count(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{broadcast_id}:disc_count"))
    }

    /// Broadcast start timestamp for quota
    /// TTL: 24 hours
    /// Type: i64
    #[must_use]
    pub fn started_at(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{broadcast_id}:start"))
    }

    /// Recording ready URL
    /// TTL: 7 days
    /// Type: string
    #[must_use]
    pub fn recording_ready(broadcast_id: Uuid) -> Self {
        Self::new(format!("recording:{broadcast_id}"))
    }

    /// Stores a recently viewed/retrieved broadcast
    #[must_use]
    pub fn broadcast(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{broadcast_id}"))
    }

    // ========== USER KEYS ==========

    /// User presence (online status)
    /// TTL: 120 seconds
    /// Type: "1" if online
    #[must_use]
    pub fn presence(user_id: Uuid) -> Self {
        Self::new(format!("u:{user_id}:online"))
    }

    /// WebSocket message buffer (ring buffer)
    /// TTL: 5 minutes
    /// Type: List of JSON strings
    #[must_use]
    pub fn ws_buffer(user_id: Uuid) -> Self {
        Self::new(format!("u:{user_id}:ws_buf"))
    }

    /// Daily quota usage
    /// TTL: 24 hours (expires at UTC midnight)
    /// Type: i64 (seconds used)
    #[must_use]
    pub fn quota(user_id: Uuid, date: &str) -> Self {
        Self::new(format!("u:{user_id}:quota:{date}"))
    }

    /// Displays the recently viewed profile
    #[must_use]
    pub fn profile(user_id: Uuid) -> Self {
        Self::new(format!("u:{user_id}:profile",))
    }

    /// Returns the list of providers of the user
    #[must_use]
    pub fn user_providers(user_id: Uuid) -> Self {
        Self::new(format!("u:{user_id}:providers",))
    }

    /// Caches the current user session
    #[must_use]
    pub fn session(user_id: Uuid) -> Self {
        Self::new(format!("u:{user_id}:session",))
    }

    // ========== RATE LIMITING ==========

    /// Reconnect rate limiting
    /// TTL: 60 seconds
    /// Type: i64 (count in the current window)
    #[must_use]
    pub fn reconnect_rate(user_id: Uuid) -> Self {
        Self::new(format!("rate:reconnect:{user_id}",))
    }

    /// Generic rate limit key
    #[must_use]
    pub fn rate_limit(prefix: &str, identifier: &str) -> Self {
        Self::new(format!("rate:{prefix}:{identifier}"))
    }

    // ========== GLOBAL KEYS ==========

    /// OAuth2 state parameter
    /// TTL: 10 minutes
    #[must_use]
    pub fn oauth2_state(state: &str) -> Self {
        Self::new(format!("oauth2:{state}"))
    }

    /// Cached OTPs
    #[must_use]
    pub fn otp(email: &str, otp_type: &str) -> Self {
        Self::new(format!("otp:{otp_type}:{email}"))
    }

    /// Cached search results
    #[must_use]
    pub fn search_results(query: &str, page: i64, limit: i64) -> Self {
        Self::new(format!("{query}:{page}:{limit}"))
    }

    /// Block access keys
    #[must_use]
    pub fn block_list(prefix: &str, id: Uuid) -> Self {
        Self::new(format!("block-list:{prefix}:{id}"))
    }

    /// The key under which the mutex lock is stored.
    /// TTL matches the time we expect the DB query to take (1–2 s).
    pub fn lock(cache_key: &str) -> Self {
        Self::new_raw(&format!("lock:{cache_key}",))
    }

    /// Idempotency key for safe retries from the client.
    /// TTL: 24 hours — covers any realistic retry window.
    #[must_use]
    pub fn idempotency(key: Uuid) -> Self {
        Self::new(format!("idem:{key}",))
    }

    /// Cursor result cache
    /// Key encodes: module + owner + cursor_value + limit, so different callers never collide.
    #[must_use]
    pub fn cursor_cache(module: &str, owner_id: Uuid, cursor: &str, limit: i64) -> String {
        format!("cursor:{module}:{owner_id}:{cursor}:{limit}",)
    }

    /// Per-user unread notification count.
    /// Incremented on `notify()`, decremented on `mark_read()`,
    /// reset to 0 on `mark_all_read()`.
    ///
    /// TTL: 1 hour (refreshed on every write).
    /// Type: i64
    #[must_use]
    pub fn unread_count(user_id: Uuid) -> Self {
        Self::new(format!("u:{user_id}:unread",))
    }
}
impl std::fmt::Display for RedisKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl AsRef<str> for RedisKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl From<RedisKey> for String {
    fn from(key: RedisKey) -> String {
        key.0
    }
}
