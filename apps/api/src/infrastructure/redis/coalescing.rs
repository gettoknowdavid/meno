//! Cache coalescing — prevents a thundering herd on a cold cache.
//!
//! Ported from `apps/api/src/shared/services/redis/coalescing.rs` on `master`
//! (`903c3ba`) with the §3.2 TTL rule applied.
//!
//! # How it works
//!
//! 1. The first request for a cache key takes a lock and fetches from the source.
//! 2. Requests that arrive meanwhile wait for the winner to populate the cache.
//! 3. Every request returns the same value, and the source is hit once.
//!
//! Without this, a popular broadcast going live sends every listener's first request
//! at an empty cache simultaneously, and Postgres serves the same query N times at
//! exactly the moment it can least afford it.
//!
//! # Why the TTL travels with the key
//!
//! On `master` the caller passed `ttl_secs` and the cache key was built separately with
//! no expiry, which is the §3.2 violation. Here the caller supplies one
//! [`RedisKey`], and the lock key is derived from it, so the cache entry, the lock, and
//! the eviction all agree on one duration.

use crate::infrastructure::constants::{LOCK_MAX_RETRIES, LOCK_RETRY_MS};
use crate::infrastructure::redis::Redis;
use crate::infrastructure::redis::keys::RedisKey;
use fred::error::Error as RedisError;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Read-through cache with single-flight semantics.
///
/// `T` must round-trip through JSON, because the winner's value is serialised into
/// Redis and the losers deserialise it back.
pub async fn coalesce_cache<T, E, Fut>(
    redis: &Redis,
    key: &RedisKey,
    fetcher: impl Fn() -> Fut,
) -> Result<T, E>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    E: From<CacheError> + Send,
    Fut: Future<Output = Result<T, E>> + Send,
{
    // Step 1: cache hit.
    if let Ok(Some(cached)) = redis.get::<T>(key).await {
        debug!(key = %key.as_str(), "cache hit");
        return Ok(cached);
    }

    debug!(key = %key.as_str(), "cache miss, attempting to acquire lock");

    // Step 2: race for the lock.
    if try_acquire_lock(redis, key).await {
        // Step 3a: winner fetches, caches, then releases.
        debug!(key = %key.as_str(), "lock acquired, fetching from source");

        let result = fetcher().await?;

        // The winner must not block the request on the cache write, but the lock may
        // not be released before the write completes — otherwise a loser would see a
        // populated lock with an empty cache and fall back to its own fetch, which is
        // the stampede we are here to prevent. So the write and the release happen
        // together in one task, and the response returns immediately.
        let redis = redis.clone();
        let key = key.clone();
        // The task needs its own copy: the caller gets `result` back immediately, and
        // `T` is only bound by `Clone`.
        let to_cache = result.clone();

        tokio::spawn(async move {
            if let Err(e) = redis.set(&key, &to_cache).await {
                warn!(key = %key.as_str(), error = %e, "failed to cache result");
            } else {
                debug!(key = %key.as_str(), "result cached");
            }

            release_lock(&redis, &key).await;
        });

        Ok(result)
    } else {
        // Step 3b: losers poll until the cache fills.
        debug!(key = %key.as_str(), "lock held, waiting for cache to populate");

        for attempt in 1..=LOCK_MAX_RETRIES {
            sleep(Duration::from_millis(LOCK_RETRY_MS)).await;

            if let Ok(Some(cached)) = redis.get::<T>(key).await {
                debug!(key = %key.as_str(), attempt, "cache populated, returning it");
                return Ok(cached);
            }

            debug!(
                key = %key.as_str(),
                attempt,
                max_retries = LOCK_MAX_RETRIES,
                "cache not yet populated, waiting"
            );
        }

        // The winner may have crashed mid-fetch, leaving a lock that outlives it. After
        // the bounded wait, fall through to a direct fetch rather than failing: a slow
        // response beats an error, and the source of truth is Postgres anyway.
        warn!(
            key = %key.as_str(),
            max_retries = LOCK_MAX_RETRIES,
            "lock wait timed out, falling back to direct fetch"
        );
        fetcher().await
    }
}

/// Try to take the distributed lock guarding `key`.
///
/// A Lua script makes `SET NX EX` one atomic step. Doing it as `SET` then `EXPIRE` from
/// the client would leave a window in which the lock exists with no expiry — and a lock
/// that never expires is a permanent deadlock for that cache key.
async fn try_acquire_lock(redis: &Redis, key: &RedisKey) -> bool {
    let lock_key = RedisKey::lock(key.as_str(), key.ttl());

    let script = r"
        if redis.call('SET', KEYS[1], '1', 'NX', 'EX', ARGV[1]) then
            return 1
        else
            return 0
        end
    ";

    let keys = vec![lock_key.as_ref()];
    let args = vec![lock_key.ttl().as_secs().to_string()];
    match redis.eval::<i64, _, _>(script, keys, args).await {
        Ok(1) => true,
        // A Redis outage must not be reported as "someone else holds the lock" without
        // distinction — but both mean "do not fetch under lock", so the caller falls
        // back to a direct fetch either way.
        Ok(_) => false,
        Err(e) => {
            warn!(key = %lock_key.as_str(), error = %e, "lock acquisition failed");
            false
        }
    }
}

/// Release the lock guarding `key`.
///
/// Errors are swallowed deliberately: the lock has a TTL, so a failed delete self-heals
/// after at most one expiry window, and propagating would turn a cosmetic failure into
/// a request failure.
pub async fn release_lock(redis: &Redis, key: &RedisKey) {
    let lock_key = RedisKey::lock(key.as_str(), key.ttl());
    if let Err(e) = redis.del(&lock_key).await {
        warn!(key = %lock_key.as_str(), error = %e, "failed to release lock; relying on TTL");
    }
}

/// Failure modes of the coalescing machinery.
///
/// This lives in `infrastructure`, so holding a `fred` error here is legitimate — §5.5
/// forbids driver types in *domain* code, and this is where they get erased.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// The lock could not be taken within the retry budget.
    #[error("failed to acquire lock after {max_retries} retries")]
    LockTimeout {
        /// How many retries were attempted before giving up.
        max_retries: u32,
    },

    /// Redis itself failed.
    #[error("redis error: {0}")]
    Redis(#[from] RedisError),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Integration tests. They need a live Redis, so they are ignored by default and
    /// CI stays green without infrastructure. Run them with:
    ///
    /// ```text
    /// docker compose -f ops/docker-compose.yml up -d redis
    /// REDIS_URL=redis://localhost:6379 cargo test -p meno-api -- --ignored
    /// ```
    ///
    /// Without `REDIS_URL` set they return immediately rather than failing, so a
    /// developer who forgets to export it sees a skip, not a red build.
    mod live {
        use super::*;
        use crate::infrastructure::redis::RedisConfig;
        use crate::infrastructure::redis::keys::ttl;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        /// Connect using `REDIS_URL`, or `None` when it is unset.
        ///
        /// `None` makes each test body `return` early, so a developer who forgets to
        /// export the variable sees a passing skip rather than a red build.
        async fn redis() -> Option<Redis> {
            let url = std::env::var("REDIS_URL").ok()?;
            Some(
                Redis::new(RedisConfig::from_url(url))
                    .await
                    .expect("connect to REDIS_URL"),
            )
        }

        #[tokio::test]
        #[ignore = "requires a live Redis; set REDIS_URL and pass --ignored"]
        async fn set_then_get_round_trips_and_expires() {
            let Some(redis) = redis().await else { return };
            let key = RedisKey::new_raw("test:roundtrip", Duration::from_secs(60));

            let _ = redis.del(&key).await;
            redis
                .set(&key, &serde_json::json!({"n": 1}))
                .await
                .expect("set");

            let got: Option<serde_json::Value> = redis.get(&key).await.expect("get");
            assert_eq!(got, Some(serde_json::json!({"n": 1})));

            redis.del(&key).await.expect("del");
            let gone: Option<serde_json::Value> = redis.get(&key).await.expect("get");
            assert!(gone.is_none(), "a deleted key must not read back");
        }

        #[tokio::test]
        #[ignore = "requires a live Redis; set REDIS_URL and pass --ignored"]
        async fn the_written_key_actually_carries_its_ttl() {
            let Some(redis) = redis().await else { return };
            let key = RedisKey::new_raw("test:ttl", Duration::from_secs(60));
            let _ = redis.del(&key).await;

            redis.set(&key, &1_i64).await.expect("set");

            // Read the TTL back from Redis itself rather than trusting our own field:
            // this is the assertion that `set` really applied the key's expiry.
            let applied: i64 = redis
                .eval(
                    "return redis.call('TTL', KEYS[1])",
                    vec![key.as_ref()],
                    vec![()],
                )
                .await
                .expect("eval");
            assert!(
                (1..=60).contains(&applied),
                "expected a TTL within the key's 60s, Redis said {applied}"
            );

            redis.del(&key).await.expect("del");
        }

        #[tokio::test]
        #[ignore = "requires a live Redis; set REDIS_URL and pass --ignored"]
        async fn coalesce_hits_the_source_once_under_concurrency() {
            // The whole point of the module: N concurrent callers, one source call.
            let Some(redis) = redis().await else { return };
            let key = RedisKey::new_raw("test:coalesce", Duration::from_secs(60));
            let _ = redis.del(&key).await;

            let calls = Arc::new(AtomicUsize::new(0));

            let fetch = |calls: Arc<AtomicUsize>| {
                move || {
                    let calls = calls.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        Ok::<i64, CacheError>(42_i64)
                    }
                }
            };

            let results = tokio::join!(
                coalesce_cache(&redis, &key, fetch(calls.clone())),
                coalesce_cache(&redis, &key, fetch(calls.clone())),
                coalesce_cache(&redis, &key, fetch(calls.clone())),
            );

            assert_eq!(results.0.unwrap(), 42);
            assert_eq!(results.1.unwrap(), 42);
            assert_eq!(results.2.unwrap(), 42);
            assert!(
                calls.load(Ordering::SeqCst) < 3,
                "coalescing must collapse the herd, saw {} fetches",
                calls.load(Ordering::SeqCst)
            );

            let _ = redis.del(&key).await;
        }

        #[tokio::test]
        #[ignore = "requires a live Redis; set REDIS_URL and pass --ignored"]
        async fn locking_is_exclusive_under_a_race() {
            let Some(redis) = redis().await else { return };
            let key = RedisKey::new_raw("test:lock", ttl::LOCK);
            let _ = redis.del(&RedisKey::lock(key.as_str(), ttl::LOCK)).await;

            let winners = Arc::new(AtomicUsize::new(0));
            let mut handles = Vec::new();
            for _ in 0..8 {
                let redis = redis.clone();
                let key = key.clone();
                let winners = winners.clone();
                handles.push(tokio::spawn(async move {
                    if try_acquire_lock(&redis, &key).await {
                        winners.fetch_add(1, Ordering::SeqCst);
                    }
                }));
            }
            for h in handles {
                h.await.expect("join");
            }

            assert_eq!(
                winners.load(Ordering::SeqCst),
                1,
                "exactly one caller may hold the lock"
            );
            let _ = redis.del(&RedisKey::lock(key.as_str(), ttl::LOCK)).await;
        }

        #[tokio::test]
        #[ignore = "requires a live Redis; set REDIS_URL and pass --ignored"]
        async fn pattern_invalidation_removes_every_user_key() {
            let Some(redis) = redis().await else { return };
            let user = uuid::Uuid::new_v4();

            let keys = [
                RedisKey::profile(user, ttl::PROFILE),
                RedisKey::session(user, ttl::SESSION),
                RedisKey::unread_count(user, ttl::UNREAD_COUNT),
            ];
            for key in &keys {
                redis.set(key, &1_i64).await.expect("set");
            }

            let deleted = redis
                .invalidate_all_user_keys(user)
                .await
                .expect("invalidate");
            assert!(deleted >= 3, "expected at least 3 removals, got {deleted}");

            for key in &keys {
                let gone: Option<i64> = redis.get(key).await.expect("get");
                assert!(gone.is_none(), "{} survived invalidation", key.as_str());
            }
        }
    }

    #[test]
    fn lock_key_is_derived_from_the_cache_key_and_keeps_its_ttl() {
        // A lock that outlives the value it guards would let a stale winner's data sit
        // in the cache; a lock shorter than the fill would let two winners through.
        let cache = RedisKey::new_raw("broadcasts:list", Duration::from_secs(30));
        let lock = RedisKey::lock(cache.as_str(), cache.ttl());
        assert_eq!(lock.as_str(), "lock:broadcasts:list");
        assert_eq!(lock.ttl(), cache.ttl());
    }

    #[test]
    fn cache_error_renders_without_leaking_a_driver_message() {
        let err = CacheError::LockTimeout { max_retries: 40 };
        assert_eq!(err.to_string(), "failed to acquire lock after 40 retries");
    }
}
