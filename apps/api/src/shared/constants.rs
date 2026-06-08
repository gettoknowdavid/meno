// ========== REDIS CACHE CONSTANTS ==========
/// Used to store rate limiter data
pub const RATE_LIMIT_PREFIX: &str = "RT";

/// Used to store access tokens from logged-out users to prevent
/// usage after a user logs out
pub const BLOCKLIST_PREFIX: &str = "BL";

/// Scoped Cache Prefix for all cached user data
/// Use mainly for authenticated data
pub const USER_CACHE_PREFIX: &str = "USER";

/// Scoped Cache Prefix for all cached Global data
/// that does not require authentication
pub const GLOBAL_CACHE_PREFIX: &str = "GLOBAL";
pub const BROADCAST_CACHE_PREFIX: &str = "BROADCAST";

pub const MAX_LOGIN_ATTEMPTS: u64 = 10;

// ========== TTL CONSTANTS ==========

/// Expiry time of 10 seconds
pub const TTL_10_SECS: i64 = 10;

/// Expiry time of 15 seconds
pub const TTL_15_SECS: i64 = 15;

/// Expiry time of 30 seconds
pub const TTL_30_SECS: i64 = 30;

/// Expiry time of 60 seconds or 1 minute
pub const TTL_60_SECS: i64 = 60;

/// Expiry time of 120 seconds or 2 minutes
pub const TTL_120_SECS: i64 = 120;

/// Expiry time of 5 minutes
pub const TTL_300_SECS: i64 = 300;

/// Expiry time of 10 minutes
pub const TTL_600_SECS: i64 = 600;

/// Expiry time of 15 minutes
pub const TTL_900_SECS: i64 = 900;

/// Expiry time of 20 minutes
pub const TTL_1800_SECS: i64 = 1800;

/// Expiry time of 45 minutes
pub const TTL_2700_SECS: i64 = 2700;

/// Expiry time of 1 hour
pub const TTL_3600_SECS: i64 = 3600;

/// If the holder crashes, lock expires in 5s
pub const LOCK_TTL_SECS: u64 = 5;

/// Losers retry every 50ms
pub const LOCK_RETRY_MS: u64 = 50;

/// ~2s total wait before fallback
pub const LOCK_MAX_RETRIES: u32 = 40;

// ========== LIVEKIT CONSTANTS ==========
pub const LIVEKIT_ACCESS_TOKEN_TTL: i64 = 21600;

// ========== WEB SOCKET CONSTANTS ==========
/// Maximum number of messages to buffer per offline user
pub const MESSAGE_BUFFER_SIZE: i64 = 50;

/// TTL for message buffer in seconds (5 minutes)
pub const MESSAGE_BUFFER_TTL_SECS: i64 = 300;

pub const MAX_WS_CONNECTIONS_PER_USER: usize = 5;
