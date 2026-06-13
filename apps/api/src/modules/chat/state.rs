use crate::modules::chat::service::ChatService;
use crate::shared::services::redis::RedisService;
use crate::shared::services::ws::WsService;

#[derive(Clone)]
pub struct ChatState {
    pub service: ChatService,
}
impl ChatState {
    pub fn new(db: sqlx::PgPool, redis: RedisService, ws: WsService) -> Self {
        Self {
            service: ChatService::new(db, redis, ws),
        }
    }
}
