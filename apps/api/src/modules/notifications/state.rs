#[derive(Clone)]
pub struct NotificationState {
    pub service: crate::modules::notifications::service::NotificationService,
}
impl NotificationState {
    pub fn new(
        db: sqlx::PgPool,
        redis: crate::shared::services::redis::Redis,
        push: crate::shared::services::push::PushNotificationService,
        pubsub: std::sync::Arc<crate::shared::services::ws::pubsub::WsPubSubBridge>,
    ) -> Self {
        Self {
            service: crate::modules::notifications::service::NotificationService::new(
                db, redis, push, pubsub,
            ),
        }
    }
}
