use crate::modules::subscribers::service::SubscribersService;

#[derive(Clone)]
pub struct SubscribersState {
    pub service: SubscribersService,
}
impl SubscribersState {
    pub fn new(db: sqlx::PgPool) -> Self {
        Self {
            service: SubscribersService::new(db),
        }
    }
}
