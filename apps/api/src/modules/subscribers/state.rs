use crate::modules::subscribers::service::SubscribersService;
use crate::shared::services::ws::pubsub::WsPubSubBridge;
use std::sync::Arc;
use sqlx::PgPool;
use crate::shared::identity::IdentityReader;

#[derive(Clone)]
pub struct SubscribersState {
    pub service: SubscribersService,

}
impl SubscribersState {
    pub fn new(db: PgPool, identity: Arc<dyn IdentityReader>, pubsub: Arc<WsPubSubBridge>) -> Self {
        Self { service: SubscribersService::new(db, identity, pubsub) }
    }
}
