//! The unread counter: Redis in the deployment, a map in the double.
//!
//! # Why a counter at all
//!
//! An inbox badge is read on every app open and every socket event, and `COUNT(*)`
//! over a growing `notifications` table is the wrong query to run that often. The
//! count is therefore maintained on the write paths and served from
//! [`RedisKey::unread_count`], with the database as the cold-path rebuild — see
//! [`crate::modules::notifications::service::NotificationsService::unread_count`].
//!
//! # It is advisory, and that is deliberate
//!
//! Every method is infallible. A failed `INCR` means the badge is stale until the
//! key expires or a rebuild reads the table; failing the request that created the
//! notification because Redis hiccuped would be strictly worse. The adapter logs and
//! moves on, which is what `master` did with its `let _ = …` calls, now with a reason
//! written down.
//!
//! # The decrement clamps
//!
//! `DECR` past zero would render a badge of `-1` after a delete that raced a rebuild.
//! A Lua script keeps decrement-and-clamp atomic, so two concurrent deletes cannot
//! both observe the same value.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use uuid::Uuid;

use crate::infrastructure::redis::Redis;
use crate::infrastructure::redis::keys::{RedisKey, ttl};

/// The per-user unread count.
#[async_trait]
pub trait UnreadCounter: Send + Sync + std::fmt::Debug {
    /// The cached count, or `None` when there is no cached answer.
    ///
    /// `None` is the caller's cue to read the database; it is also what a Redis
    /// failure answers, because a cache that cannot be read has no answer either.
    async fn read(&self, owner_id: Uuid) -> Option<i64>;

    /// Count one more unread notification.
    async fn increment(&self, owner_id: Uuid);

    /// Count one fewer, never below zero.
    async fn decrement(&self, owner_id: Uuid);

    /// Set the count to zero — `PATCH /notifications/read-all`.
    async fn reset(&self, owner_id: Uuid);
}

/// The real adapter: the shared Redis.
#[derive(Clone)]
pub struct RedisUnreadCounter {
    redis: Redis,
}

impl std::fmt::Debug for RedisUnreadCounter {
    /// Names the field and stops: the handle carries the Redis URL.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisUnreadCounter")
            .field("redis", &"[redis]")
            .finish()
    }
}

impl RedisUnreadCounter {
    /// Read and write through `redis`.
    #[must_use]
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }
}

#[async_trait]
impl UnreadCounter for RedisUnreadCounter {
    async fn read(&self, owner_id: Uuid) -> Option<i64> {
        let key = RedisKey::unread_count(owner_id, ttl::UNREAD_COUNT);
        match self.redis.get_i64(&key).await {
            Ok(count) => {
                // `get_i64` answers 0 for a missing key, which is indistinguishable
                // from a cached zero. Ask whether the key is really there, so a
                // cache miss falls through to the database and a real zero does not
                // cost a query.
                if self.redis.exists(&key).await.unwrap_or(false) {
                    Some(count)
                } else {
                    None
                }
            }
            Err(error) => {
                tracing::warn!(
                    owner_id = %owner_id,
                    error = %error,
                    "unread counter read failed; the database will answer instead"
                );
                None
            }
        }
    }

    async fn increment(&self, owner_id: Uuid) {
        let key = RedisKey::unread_count(owner_id, ttl::UNREAD_COUNT);
        if let Err(error) = self.redis.incr_and_expire_if_first(&key).await {
            tracing::warn!(
                owner_id = %owner_id,
                error = %error,
                "unread counter increment failed; the badge will be rebuilt from the database"
            );
        }
    }

    async fn decrement(&self, owner_id: Uuid) {
        let key = RedisKey::unread_count(owner_id, ttl::UNREAD_COUNT);
        let script = "
            local v = redis.call('DECR', KEYS[1])
            if v < 0 then
                redis.call('SET', KEYS[1], '0', 'EX', ARGV[1])
                return 0
            end
            return v
        ";

        if let Err(error) = self
            .redis
            .eval::<i64, Vec<&str>, Vec<i64>>(script, vec![key.as_ref()], vec![key.ttl().as_secs() as i64])
            .await
        {
            tracing::warn!(
                owner_id = %owner_id,
                error = %error,
                "unread counter decrement failed; the badge will be rebuilt from the database"
            );
        }
    }

    async fn reset(&self, owner_id: Uuid) {
        let key = RedisKey::unread_count(owner_id, ttl::UNREAD_COUNT);
        if let Err(error) = self.redis.set(&key, &0_i64).await {
            tracing::warn!(
                owner_id = %owner_id,
                error = %error,
                "unread counter reset failed; the next read rebuilds it from the database"
            );
        }
    }
}

/// A [`UnreadCounter`] held in memory.
///
/// A missing entry reads as `None`, so the service's database fallback is exercised
/// by the same tests that exercise the cache.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct InMemoryUnreadCounter {
    counts: Mutex<HashMap<Uuid, i64>>,
}

#[cfg(any(test, feature = "test-support"))]
impl InMemoryUnreadCounter {
    /// An empty counter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The stored value, for assertions that do not go through the trait.
    #[must_use]
    pub fn value(&self, owner_id: Uuid) -> Option<i64> {
        self.lock().get(&owner_id).copied()
    }

    /// Forget every entry, as a Redis restart does.
    pub fn clear(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Uuid, i64>> {
        self.counts.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(any(test, feature = "test-support"))]
#[async_trait]
impl UnreadCounter for InMemoryUnreadCounter {
    async fn read(&self, owner_id: Uuid) -> Option<i64> {
        self.value(owner_id)
    }

    async fn increment(&self, owner_id: Uuid) {
        *self.lock().entry(owner_id).or_insert(0) += 1;
    }

    async fn decrement(&self, owner_id: Uuid) {
        let mut counts = self.lock();
        let count = counts.entry(owner_id).or_insert(0);
        *count = (*count - 1).max(0);
    }

    async fn reset(&self, owner_id: Uuid) {
        self.lock().insert(owner_id, 0);
    }
}

#[cfg(test)]
mod tests {
    //! The double's contract, which the service tests rely on.

    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[tokio::test]
    async fn a_cold_counter_has_no_answer_rather_than_zero() {
        let counter = InMemoryUnreadCounter::new();
        assert_eq!(counter.read(id(1)).await, None);
    }

    #[tokio::test]
    async fn increments_and_decrements_stay_within_the_users_own_count() {
        let counter = InMemoryUnreadCounter::new();

        counter.increment(id(1)).await;
        counter.increment(id(1)).await;
        counter.increment(id(2)).await;

        assert_eq!(counter.read(id(1)).await, Some(2));
        assert_eq!(counter.read(id(2)).await, Some(1));
    }

    #[tokio::test]
    async fn a_decrement_never_goes_negative() {
        // The clamp the Lua script performs. A badge of `-1` is a visible bug.
        let counter = InMemoryUnreadCounter::new();

        counter.decrement(id(1)).await;
        assert_eq!(counter.read(id(1)).await, Some(0));

        counter.increment(id(1)).await;
        counter.decrement(id(1)).await;
        counter.decrement(id(1)).await;
        assert_eq!(counter.read(id(1)).await, Some(0));
    }

    #[tokio::test]
    async fn a_reset_answers_a_cached_zero_not_a_miss() {
        // The distinction matters: a miss makes the next read hit the database, and
        // `read-all` is exactly when the database would still say "unread".
        let counter = InMemoryUnreadCounter::new();
        counter.increment(id(1)).await;

        counter.reset(id(1)).await;

        assert_eq!(counter.read(id(1)).await, Some(0));
    }
}
