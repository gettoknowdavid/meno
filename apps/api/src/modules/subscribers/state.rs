use crate::modules::subscribers::service::SubscribersService;
use crate::shared::services::ws::WsService;

#[derive(Clone)]
pub struct SubscribersState {
    pub service: SubscribersService,
}
impl SubscribersState {
    pub fn new(db: sqlx::PgPool, ws: WsService) -> Self {
        Self {
            service: SubscribersService::new(db, ws),
        }
    }
}
