//! The `/users/me` read-through cache (§3.2).
//!
//! `MeResponse` embeds settings, providers and counts, so a cold `/users/me` is five
//! queries; the cache turns the common case into one `GET`. The key is the
//! application's own `u:{user_id}:profile` with the shared `ttl::PROFILE`
//! (five minutes) — a *cache* TTL, deliberately longer than `master`'s sixty seconds
//! because every write path invalidates it explicitly.
//!
//! # Fail-open, deliberately
//!
//! Every method returns `Result` so a Redis outage is *visible*, but the service
//! treats an error as a miss and logs it: availability over freshness, because the
//! payload is a profile and not a balance. The one exception is invalidation after a
//! write — there the service reads straight from the database rather than risk
//! serving the value it just replaced. That is the difference between "a cache miss"
//! and "the wrong answer", and both are decided in `service.rs`, not here.

use async_trait::async_trait;
use uuid::Uuid;

use super::dto::MeResponse;
use super::error;
use crate::infrastructure::redis::Redis;
use crate::infrastructure::redis::keys::{RedisKey, ttl};
use meno_core::Error as MenoError;

#[cfg(any(test, feature = "test-support"))]
pub use memory::InMemoryProfileCache;

/// What the service needs from a `/users/me` cache.
#[async_trait]
pub trait ProfileCache: Send + Sync + 'static {
    /// Store a freshly built `MeResponse` under the user's key.
    async fn cache_me(&self, value: MeResponse) -> Result<(), MenoError>;

    /// The cached payload, or `None` on a miss (or a deserialisation gap).
    async fn get_cached_me(&self, user_id: Uuid) -> Result<Option<MeResponse>, MenoError>;

    /// Drop the user's entry — called after every write that lands inside
    /// `MeResponse`, by both the profile and the settings services.
    async fn invalidate_cached_profile(&self, user_id: Uuid) -> Result<(), MenoError>;
}

/// The Redis-backed cache production runs.
#[derive(Clone)]
pub struct ProfileRedisCache {
    redis: Redis,
}

impl ProfileRedisCache {
    /// Wrap the application's Redis client.
    #[must_use]
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }
}

#[async_trait]
impl ProfileCache for ProfileRedisCache {
    async fn cache_me(&self, value: MeResponse) -> Result<(), MenoError> {
        let key = RedisKey::profile(value.id, ttl::PROFILE);
        self.redis
            .set(&key, &value)
            .await
            .map_err(|e| error::internal("profile_cache_set", e))
    }

    async fn get_cached_me(&self, user_id: Uuid) -> Result<Option<MeResponse>, MenoError> {
        let key = RedisKey::profile(user_id, ttl::PROFILE);
        self.redis
            .get::<MeResponse>(&key)
            .await
            .map_err(|e| error::internal("profile_cache_get", e))
    }

    async fn invalidate_cached_profile(&self, user_id: Uuid) -> Result<(), MenoError> {
        let key = RedisKey::profile(user_id, ttl::PROFILE);
        self.redis
            .del(&key)
            .await
            .map_err(|e| error::internal("profile_cache_del", e))?;
        Ok(())
    }
}

#[cfg(any(test, feature = "test-support"))]
pub mod memory {
    //! The in-memory double of [`super::ProfileCache`] (§10).
    //!
    //! Keeps whole `MeResponse` values, so a test proves caching by mutating the
    //! repository underneath and observing the *old* value come back — the read-through
    //! behaviour itself, not just that a key exists somewhere.

    use std::collections::HashMap;
    use std::sync::{Mutex, MutexGuard};

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::{MeResponse, ProfileCache};
    use meno_core::Error as MenoError;

    /// The double the profile suite runs against.
    #[derive(Debug, Default)]
    pub struct InMemoryProfileCache {
        entries: Mutex<HashMap<Uuid, MeResponse>>,
    }

    impl InMemoryProfileCache {
        /// An empty cache.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// The stored entry, for assertions that do not go through the trait.
        #[must_use]
        pub fn get(&self, user_id: Uuid) -> Option<MeResponse> {
            self.lock().get(&user_id).cloned()
        }

        fn lock(&self) -> MutexGuard<'_, HashMap<Uuid, MeResponse>> {
            self.entries.lock().unwrap_or_else(|e| e.into_inner())
        }
    }

    #[async_trait]
    impl ProfileCache for InMemoryProfileCache {
        async fn cache_me(&self, value: MeResponse) -> Result<(), MenoError> {
            self.lock().insert(value.id, value);
            Ok(())
        }

        async fn get_cached_me(&self, user_id: Uuid) -> Result<Option<MeResponse>, MenoError> {
            Ok(self.lock().get(&user_id).cloned())
        }

        async fn invalidate_cached_profile(&self, user_id: Uuid) -> Result<(), MenoError> {
            self.lock().remove(&user_id);
            // The double never fails — it exists to prove hit/miss/invalidate
            // behaviour. A *failing* cache is asserted with a bespoke impl in the
            // test that needs one, which is the only way to reach this error path.
            Ok(())
        }
    }
}
