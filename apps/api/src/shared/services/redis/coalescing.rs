use crate::shared::constants::{LOCK_MAX_RETRIES, LOCK_RETRY_MS, LOCK_TTL_SECS};
use crate::shared::services::redis::Redis;
use crate::shared::services::redis::keys::RedisKey;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Generic cache coalescing function that prevents thundering herd.
///
/// # How it works:
/// 1. First request for a cache key acquires a lock and fetches data
/// 2. Subsequent requests wait for the first to complete
/// 3. All requests receive the same cached result
///
/// # Type Parameters:
/// - `T`: The cached value type (must be Serialize + DeserializeOwned)
/// - `E`: The error type (must implement From<CacheError> + ...)
/// - `F`: A future that returns Result<T, E>
///
/// # Example:
/// ```ignore
/// let data = coalesce_cache(
///     &redis,
///     "broadcasts:list:page1",
///     30,
///     || async {
///         fetch_from_database().await
///     },
/// ).await?;
/// ```
pub async fn coalesce_cache<T, E, Fut>(
    redis: &Redis,
    cache_key: &str,
    ttl_secs: i64,
    fetcher: impl Fn() -> Fut,
) -> Result<T, E>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    E: From<CacheError> + From<crate::shared::services::redis::Error> + Send,
    Fut: Future<Output = Result<T, E>> + Send,
{
    let key = RedisKey::new_raw(cache_key);

    // Step 1: Try to get from cache
    if let Ok(Some(cached)) = redis.get::<T>(&key).await {
        debug!(cache_key = %cache_key, "cache hit");
        return Ok(cached);
    }

    debug!(cache_key = %cache_key, "cache miss, attempting to acquire lock");

    // Step 2: Try to acquire lock (set if not exists)
    let acquired_lock = try_acquire_lock(redis, cache_key, LOCK_TTL_SECS).await;

    if acquired_lock {
        // Step 3a: Winner - fetch from source
        debug!(cache_key = %cache_key, "lock acquired, fetching from source");

        let result = fetcher().await?;

        // Cache the result
        let redis_clone = redis.clone();
        let key_clone = key;
        let result_clone = result.clone();
        let cache_key_owned = cache_key.to_owned();

        tokio::spawn(async move {
            if let Err(e) = redis_clone
                .set(&key_clone, &result_clone, Some(ttl_secs))
                .await
            {
                warn!(cache_key = %cache_key_owned, error = %e, "failed to cache result");
            } else {
                debug!(cache_key = %cache_key_owned, "result cached successfully");
            }

            // Release lock
            release_lock(&redis_clone, &cache_key_owned).await;
        });

        Ok(result)
    } else {
        // Step 3b: Loser - wait for winner to populate cache
        debug!(cache_key = %cache_key, "lock not acquired, waiting for cache to be populated");

        for attempt in 1..=LOCK_MAX_RETRIES {
            sleep(Duration::from_millis(LOCK_RETRY_MS)).await;

            if let Ok(Some(cached)) = redis.get::<T>(&key).await {
                debug!(
                    cache_key = %cache_key,
                    attempt = attempt,
                    "cache populated, returning cached result"
                );
                return Ok(cached);
            }

            debug!(
                cache_key = %cache_key,
                attempt = attempt,
                max_retries = LOCK_MAX_RETRIES,
                "cache not yet populated, waiting..."
            );
        }

        // Fallback: lock holder may have crashed
        warn!(
            cache_key = %cache_key,
            "lock wait timeout, falling back to direct fetch"
        );
        fetcher().await
    }
}

/// Try to acquire a distributed lock using SET NX (set if not exists)
async fn try_acquire_lock(redis: &Redis, cache_key: &str, ttl_secs: u64) -> bool {
    let lock_key = RedisKey::lock(cache_key);

    // Use a Lua script for atomic NX+EXPIRE (avoids race on older Redis):
    let script = r#"
            if redis.call('SET', KEYS[1], '1', 'NX', 'EX', ARGV[1]) then
                return 1
            else
                return 0
            end
        "#;

    let keys = vec![lock_key.as_ref()];
    let args = vec![ttl_secs.to_string()];
    let result: i64 = redis.eval(script, keys, args).await.unwrap_or(0);
    result == 1
}

/// Release the distributed lock
pub async fn release_lock(redis: &Redis, cache_key: &str) {
    let lock_key = RedisKey::lock(cache_key);
    let _ = redis.del(&lock_key).await;
}

/// Error type for cache coalescing operations
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Failed to acquire lock after {max_retries} retries")]
    LockTimeout { max_retries: u32 },

    #[error("Redis error: {0}")]
    Redis(#[from] crate::shared::services::redis::Error),
}
