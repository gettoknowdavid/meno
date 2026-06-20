use crate::modules::subscribers::service::SubscribersService;
use crate::shared::services::ws::pubsub::WsPubSubBridge;
use std::sync::Arc;

#[derive(Clone)]
pub struct SubscribersState {
    pub service: SubscribersService,
}
impl SubscribersState {
    pub fn new(db: sqlx::PgPool, pubsub: Arc<WsPubSubBridge>) -> Self {
        Self {
            service: SubscribersService::new(db, pubsub),
        }
    }
}
