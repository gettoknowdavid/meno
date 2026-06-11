use crate::modules::notifications::service::NotificationService;
use crate::shared::services::push::PushNotificationService;
use crate::shared::services::redis::RedisService;
use crate::shared::services::ws::WsService;

#[derive(Clone)]
pub struct NotificationState {
    pub service: NotificationService,
}
impl NotificationState {
    pub fn new(
        db: sqlx::PgPool,
        redis: RedisService,
        ws: WsService,
        push: PushNotificationService,
    ) -> Self {
        Self {
            service: NotificationService::new(db, redis, ws, push),
        }
    }
}
