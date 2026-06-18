use crate::modules::notifications::service::NotificationService;
use crate::shared::services::push::PushNotificationService;
use crate::shared::services::redis::Redis;

#[derive(Clone)]
pub struct NotificationState {
    pub service: NotificationService,
}
impl NotificationState {
    pub fn new(db: sqlx::PgPool, redis: Redis, push: PushNotificationService) -> Self {
        Self {
            service: NotificationService::new(db, redis, push),
        }
    }
}
