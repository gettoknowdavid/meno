use crate::modules::broadcast::repository::BroadcastRepository;
use crate::shared::services::redis::RedisService;
use crate::shared::services::storage::StorageService;
use crate::shared::services::ws::hub::WsHub;

#[derive(Clone)]
pub struct BroadcastService {
    repo: BroadcastRepository,
}
impl BroadcastService {
    pub fn new(db: sqlx::PgPool, rd: RedisService, ws: WsHub, storage: StorageService) -> Self {
        let repo = BroadcastRepository::new(db, rd, ws, storage);
        Self { repo }
    }
}
