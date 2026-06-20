#[derive(Clone)]
pub struct ChatState {
    pub service: crate::modules::chat::service::ChatService,
}
impl ChatState {
    pub fn new(
        db: sqlx::PgPool,
        redis: crate::shared::services::redis::Redis,
        pubsub: std::sync::Arc<crate::shared::services::ws::pubsub::WsPubSubBridge>,
    ) -> Self {
        Self {
            service: crate::modules::chat::service::ChatService::new(db, redis, pubsub),
        }
    }
}
