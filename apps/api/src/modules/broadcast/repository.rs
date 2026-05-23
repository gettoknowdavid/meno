use crate::shared::services::redis::RedisService;
use crate::shared::services::storage::StorageService;
use crate::shared::services::ws::hub::WsHub;

#[derive(Clone)]
pub struct BroadcastRepository {
    database: sqlx::PgPool,
    redis: RedisService,
    ws: WsHub,
    storage: StorageService,
}
impl BroadcastRepository {
    pub fn new(db: sqlx::PgPool, rd: RedisService, ws: WsHub, storage: StorageService) -> Self {
        Self {
            database: db,
            redis: rd,
            ws,
            storage,
        }
    }
}
