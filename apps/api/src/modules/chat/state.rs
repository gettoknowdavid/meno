use crate::modules::chat::service::ChatService;
use crate::shared::services::redis::RedisService;

#[derive(Clone)]
pub struct ChatState {
    pub service: ChatService,
}
impl ChatState {
    pub fn new(db: sqlx::PgPool, redis: RedisService) -> Self {
        Self {
            service: ChatService::new(db, redis),
        }
    }
}
