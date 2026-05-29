use uuid::Uuid;

/// Centralized Redis key definitions
///
/// All keys are defined here as static methods to ensure consistency.
/// Pattern: {namespace}:{entity}:{id}:{suffix}
#[derive(Debug, Clone)]
pub struct RedisKey(String);
impl RedisKey {
    fn new(key: String) -> Self {
        Self(key)
    }

    // ========== BROADCAST KEYS ==========

    /// Current live participant count
    /// TTL: None (deleted when broadcast ends)
    /// Type: i64
    pub fn live_count(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{}:live", broadcast_id))
    }

    /// Host grace period TTL (seconds remaining)
    /// TTL: Grace period duration (120s, 90s, etc.)
    /// Type: string (seconds as string)
    pub fn host_grace(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{}:grace", broadcast_id))
    }

    /// When grace period started (Unix timestamp)
    /// TTL: Grace period + 10s
    /// Type: i64
    pub fn grace_started(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{}:grace_start", broadcast_id))
    }

    /// Number of disconnects in current session
    /// TTL: 1 hour
    /// Type: i64
    pub fn disconnect_count(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{}:disc_count", broadcast_id))
    }

    /// Broadcast start timestamp for quota
    /// TTL: 24 hours
    /// Type: i64
    pub fn started_at(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{}:start", broadcast_id))
    }

    /// Recording ready URL
    /// TTL: 7 days
    /// Type: string
    pub fn recording_ready(broadcast_id: Uuid) -> Self {
        Self::new(format!("recording:{}", broadcast_id))
    }

    /// Stores a recently viewed/retrieved broadcast
    pub fn broadcast(broadcast_id: Uuid) -> Self {
        Self::new(format!("b:{}", broadcast_id))
    }

    // ========== USER KEYS ==========

    /// User presence (online status)
    /// TTL: 120 seconds
    /// Type: "1" if online
    pub fn presence(user_id: Uuid) -> Self {
        Self::new(format!("u:{}:online", user_id))
    }

    /// WebSocket message buffer (ring buffer)
    /// TTL: 5 minutes
    /// Type: List of JSON strings
    pub fn ws_buffer(user_id: Uuid) -> Self {
        Self::new(format!("u:{}:ws_buf", user_id))
    }

    /// Daily quota usage
    /// TTL: 24 hours (expires at UTC midnight)
    /// Type: i64 (seconds used)
    pub fn quota(user_id: Uuid, date: &str) -> Self {
        Self::new(format!("u:{}:quota:{}", user_id, date))
    }

    /// Displays the recently viewed profile
    pub fn profile(user_id: Uuid) -> Self {
        Self::new(format!("u:{}:profile", user_id))
    }

    /// Returns the list of providers of the user
    pub fn user_providers(user_id: Uuid) -> Self {
        Self::new(format!("u:{}:providers", user_id))
    }

    /// Caches the current user session
    pub fn session(user_id: Uuid) -> Self {
        Self::new(format!("u:{}:session", user_id))
    }

    // ========== RATE LIMITING ==========

    /// Reconnect rate limiting
    /// TTL: 60 seconds
    /// Type: i64 (count in window)
    pub fn reconnect_rate(user_id: Uuid) -> Self {
        Self::new(format!("rate:reconnect:{}", user_id))
    }

    /// Generic rate limit key
    pub fn rate_limit(prefix: &str, identifier: &str) -> Self {
        Self::new(format!("rate:{}:{}", prefix, identifier))
    }

    // ========== GLOBAL KEYS ==========

    /// OAuth2 state parameter
    /// TTL: 10 minutes
    pub fn oauth2_state(state: &str) -> Self {
        Self::new(format!("oauth2:{}", state))
    }

    /// Cached OTPs
    pub fn otp(email: &str, otp_type: &str) -> Self {
        Self::new(format!("otp:{}:{}", otp_type, email))
    }

    /// Cached search results
    pub fn search_results(query: &str, page: i64, limit: i64) -> Self {
        Self::new(format!("{}:{}:{}", query, page, limit))
    }

    /// Block access keys
    pub fn block_list(prefix: &str, id: Uuid) -> Self {
        Self::new(format!("block-list:{}:{}", prefix, id))
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
