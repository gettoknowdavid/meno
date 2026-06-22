use crate::modules::notifications::state::NotificationState;
use crate::modules::subscribers::service::SubscribersService;
use crate::shared::identity::IdentityReader;
use std::sync::Arc;

#[derive(Clone)]
pub struct SubscribersState {
    pub service: SubscribersService,
}
impl SubscribersState {
    pub fn new(
        db: sqlx::PgPool,
        identity: Arc<dyn IdentityReader>,
        notifications: NotificationState,
    ) -> Self {
        Self {
            service: SubscribersService::new(db, identity, notifications.service),
        }
    }
}
