//! Cross-cutting constants for infrastructure adapters.
//!
//! Copied verbatim from `apps/api/src/shared/constants.rs` on `master` (`903c3ba`).
//! Step 2.3 moves that file to `infrastructure/constants.rs`; this is the subset the
//! Redis adapter needs so the crate compiles before that copy runs.

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

/// Scoped Cache Prefix for all cached broadcast data
pub const BROADCAST_CACHE_PREFIX: &str = "BROADCAST";

/// Maximum login attempts before an account is rate limited
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

/// If the holder crashes, the lock expires in 5s.
pub const LOCK_TTL_SECS: u64 = 5;

/// Losers retry every 50ms.
pub const LOCK_RETRY_MS: u64 = 50;

/// ~2s total wait before falling back to a direct fetch.
pub const LOCK_MAX_RETRIES: u32 = 40;

// ========== WEBSOCKET CONSTANTS ==========

/// How many concurrent connections one user may hold on a single instance.
///
/// A user typically has two (web + mobile), so this is headroom rather than a hard
/// product limit — its job is to stop one misbehaving client from pinning thousands
/// of `mpsc` channels and a slice of the 25 MB Redis budget (§3.2).
pub const MAX_WS_CONNECTIONS_PER_USER: usize = 5;

/// Depth of the per-connection outbound channel.
///
/// Bounded so a client that stops reading its socket cannot make the server buffer
/// without limit; once full, sends are dropped rather than awaited (§9.4's "back
/// pressure is a correctness concern, not a performance one").
pub const MESSAGE_BUFFER_SIZE: usize = 128;

/// How many offline messages are retained per user in the Redis ring buffer.
///
/// Bounded because the buffer lives in the same ephemeral, memory-capped instance as
/// everything else. A user who is away longer than this reconnects to a gap, which
/// the client reconciles by refetching — an unbounded buffer would instead evict
/// live keys belonging to everyone else.
pub const OFFLINE_MESSAGE_HISTORY: usize = 50;

/// TTL on a user's offline message ring buffer, in seconds.
///
/// Must exceed the grace period a disconnected client is expected to be back
/// within, or the replay-on-reconnect path finds nothing waiting.
pub const MESSAGE_BUFFER_TTL_SECS: i64 = 300;
