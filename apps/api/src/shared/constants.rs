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

// ========== TTL CONSTANTS ==========

/// Expiry time of 30 seconds
pub const TTL_30_SECS: i64 = 30;

/// Expiry time of 60 seconds or 1 minute
pub const TTL_60_SECS: i64 = 60;

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
