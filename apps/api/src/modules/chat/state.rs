use crate::modules::chat::service::ChatService;
use crate::shared::services::redis::Redis;
use crate::shared::services::ws::pubsub::WsPubSubBridge;
use std::sync::Arc;

#[derive(Clone)]
pub struct ChatState {
    pub service: ChatService,
}
impl ChatState {
    pub fn new(db: sqlx::PgPool, redis: Redis, pubsub: Arc<WsPubSubBridge>) -> Self {
        Self {
            service: ChatService::new(db, redis, pubsub),
        }
    }
}
