use crate::modules::chat::errors::ChatError;
use crate::shared::services::redis::Redis;
use crate::shared::services::redis::keys::RedisKey;
use uuid::Uuid;

#[derive(Clone)]
pub struct ChatCache {
    redis: Redis,
}
impl ChatCache {
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }

    pub fn chat_list_cache_key(broadcast_id: Uuid, cursor: Option<&str>, limit: i64) -> String {
        match cursor {
            Some(c) if !c.is_empty() => {
                format!("chat:{}:msgs:cur={}:lim={}", broadcast_id, c, limit)
            }
            _ => format!("chat:{}:msgs:cur=_start:lim={}", broadcast_id, limit),
        }
    }
    pub async fn invalidate_chat(&self, broadcast_id: Uuid) {
        let redis = self.redis.clone();
        let broadcast_id_clone = broadcast_id.clone();

        tokio::spawn(async move {
            let pattern = format!("chat:{}:msgs:*", broadcast_id_clone);
            if let Err(e) = redis.delete_by_pattern(&pattern).await {
                tracing::warn!(
                    error = %e,
                    broadcast_id = %broadcast_id_clone,
                    "Failed to invalidate `Chat` list cache"
                );
            } else {
                tracing::debug!(
                    broadcast_id = %broadcast_id_clone,
                    "`Chat` list cache invalidated"
                );
            }
        });
    }

    pub async fn is_live(&self, broadcast_id: Uuid) -> Result<bool, ChatError> {
        let key = RedisKey::live_count(broadcast_id);
        self.redis.exists(&key).await.map_err(ChatError::Redis)
    }
}
