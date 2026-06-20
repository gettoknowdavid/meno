use crate::modules::broadcast::errors::BroadcastError;
use crate::shared::services::redis::Redis;
use crate::shared::services::redis::keys::RedisKey;
use uuid::Uuid;

#[derive(Clone)]
pub struct BroadcastRedisCache {
    redis: Redis,
}

impl BroadcastRedisCache {
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }
}

#[async_trait::async_trait]
pub trait BroadcastCache: Send + Sync + 'static {
    async fn set_started_at(
        &self,
        broadcast_id: Uuid,
        time: &time::OffsetDateTime,
    ) -> Result<(), BroadcastError>;

    async fn set_live_count(&self, broadcast_id: Uuid) -> Result<(), BroadcastError>;

    async fn get_live_count(&self, broadcast_id: Uuid) -> Result<Option<i64>, BroadcastError>;

    async fn increment_live_count(&self, broadcast_id: Uuid) -> Result<i64, BroadcastError>;

    async fn decrement_live_count(&self, broadcast_id: Uuid) -> Result<i64, BroadcastError>;

    async fn delete_live_count(&self, broadcast_id: Uuid) -> Result<(), BroadcastError>;

    async fn delete_host_grace(&self, broadcast_id: Uuid) -> Result<(), BroadcastError>;

    async fn clear_broadcast_cache(&self, broadcast_id: Uuid);

    /// Invalidate ALL broadcast-list cache entries.
    ///
    /// Call this whenever the global list could have changed:
    ///   - `go_live` (a new active broadcast appears)
    ///   - `end_broadcast` (an active broadcast disappears)
    ///   - `create` (a new draft / scheduled broadcast appears)
    ///   - `delete` (a broadcast disappears)
    fn invalidate_list_caches(&self);

    fn invalidate_broadcast_cache(&self, broadcast_id: Uuid);
}

#[async_trait::async_trait]
impl BroadcastCache for BroadcastRedisCache {
    async fn set_started_at(
        &self,
        broadcast_id: Uuid,
        time: &time::OffsetDateTime,
    ) -> Result<(), BroadcastError> {
        let key = RedisKey::started_at(broadcast_id);
        let _ = self.redis.set(&key, &time, None).await?;
        Ok(())
    }

    async fn set_live_count(&self, broadcast_id: Uuid) -> Result<(), BroadcastError> {
        let key = RedisKey::live_count(broadcast_id);
        let _ = self.redis.set(&key, &1_i64, None).await?;
        Ok(())
    }

    async fn get_live_count(&self, broadcast_id: Uuid) -> Result<Option<i64>, BroadcastError> {
        let key = RedisKey::live_count(broadcast_id);
        let count = self.redis.get::<i64>(&key).await?;
        Ok(count)
    }

    async fn increment_live_count(&self, broadcast_id: Uuid) -> Result<i64, BroadcastError> {
        let key = RedisKey::live_count(broadcast_id);
        let count = self.redis.incr(&key).await.unwrap_or(1);
        Ok(count)
    }

    async fn decrement_live_count(&self, broadcast_id: Uuid) -> Result<i64, BroadcastError> {
        let key = RedisKey::live_count(broadcast_id);
        let remaining = self.redis.decr(&key).await.unwrap_or(0).max(0);
        if remaining == 0 {
            let _ = self.redis.del(&key).await;
        }
        Ok(remaining)
    }

    async fn delete_live_count(&self, broadcast_id: Uuid) -> Result<(), BroadcastError> {
        let key = RedisKey::live_count(broadcast_id);
        let _ = self.redis.del(&key).await;
        Ok(())
    }

    async fn delete_host_grace(&self, broadcast_id: Uuid) -> Result<(), BroadcastError> {
        let key = RedisKey::host_grace(broadcast_id);
        let _ = self.redis.del(&key).await;
        Ok(())
    }

    async fn clear_broadcast_cache(&self, broadcast_id: Uuid) {
        let keys = vec![
            RedisKey::live_count(broadcast_id),
            RedisKey::host_grace(broadcast_id),
            RedisKey::started_at(broadcast_id),
        ];
        for key in keys {
            let _ = self.redis.del(&key).await;
        }
    }

    fn invalidate_list_caches(&self) {
        let redis = self.redis.clone();
        tokio::spawn(async move {
            if let Err(e) = redis.delete_by_pattern("home:now_live:*").await {
                tracing::warn!(
                    error = %e,
                    "Failed to invalidate `Now Live` list cache"
                );
            } else {
                tracing::debug!("`Now Live` list cache invalidated");
            }

            if let Err(e) = redis.delete_by_pattern("home:recently_live:*").await {
                tracing::warn!(
                    error = %e,
                    "Failed to invalidate `Recently Live` list cache"
                );
            } else {
                tracing::debug!("`Recently Live` list cache invalidated");
            }

            if let Err(e) = redis.delete_by_pattern("bl:*").await {
                tracing::warn!(
                    error = %e,
                    "Failed to invalidate broadcast list cache"
                );
            } else {
                tracing::debug!("Broadcast list cache invalidated");
            }

            if let Err(e) = redis.delete_by_pattern("pl:*").await {
                tracing::warn!(
                    error = %e,
                    "Failed to invalidate participant list cache"
                );
            } else {
                tracing::debug!("Participant list cache invalidated");
            }
        });
    }

    fn invalidate_broadcast_cache(&self, broadcast_id: Uuid) {
        let redis = self.redis.clone();
        tokio::spawn(async move {
            let key = RedisKey::broadcast(broadcast_id);
            let _ = redis.del(&key).await;
        });
    }
}
