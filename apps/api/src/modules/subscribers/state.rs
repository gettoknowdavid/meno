#[derive(Clone)]
pub struct SubscribersState {
    pub service: crate::modules::subscribers::service::SubscribersService,
}
impl SubscribersState {
    pub fn new(
        db: sqlx::PgPool,
        identity: std::sync::Arc<dyn crate::shared::identity::IdentityReader>,
        pubsub: std::sync::Arc<crate::shared::services::ws::pubsub::WsPubSubBridge>,
    ) -> Self {
        Self {
            service: crate::modules::subscribers::service::SubscribersService::new(
                db, identity, pubsub,
            ),
        }
    }
}
