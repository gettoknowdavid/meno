use crate::modules::chat::errors::ChatError;
use crate::shared::services::redis::Redis;
use crate::shared::services::redis::keys::RedisKey;
use uuid::Uuid;

#[derive(Clone)]
pub struct ChatRedisCache {
    redis: Redis,
}
impl ChatRedisCache {
    #[must_use]
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }
}

#[async_trait::async_trait]
pub trait ChatCache: Send + Sync + 'static {
    async fn invalidate_chat(&self, broadcast_id: Uuid);
    async fn is_live(&self, broadcast_id: Uuid) -> Result<bool, ChatError>;
}

#[async_trait::async_trait]
impl ChatCache for ChatRedisCache {
    async fn invalidate_chat(&self, broadcast_id: Uuid) {
        let redis = self.redis.clone();

        tokio::spawn(async move {
            let pattern = format!("chat:{}:msgs:*", broadcast_id);
            if let Err(e) = redis.delete_by_pattern(&pattern).await {
                tracing::warn!(
                    error = %e,
                    broadcast_id = %broadcast_id,
                    "Failed to invalidate `Chat` list cache"
                );
            } else {
                tracing::debug!(
                    broadcast_id = %broadcast_id,
                    "`Chat` list cache invalidated"
                );
            }
        });
    }

    async fn is_live(&self, broadcast_id: Uuid) -> Result<bool, ChatError> {
        let key = RedisKey::live_count(broadcast_id);
        self.redis.exists(&key).await.map_err(ChatError::Redis)
    }
}
