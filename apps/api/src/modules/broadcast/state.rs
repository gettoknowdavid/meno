use crate::modules::broadcast::repository::BroadcastRepository;
use crate::modules::broadcast::service::BroadcastService;
use crate::shared::services::livekit::LivekitService;
use crate::shared::services::redis::Redis;

#[derive(Clone)]
pub struct BroadcastState {
    pub service: BroadcastService,
}

impl BroadcastState {
    pub fn new(db: sqlx::PgPool, redis: Redis, livekit: LivekitService) -> Self {
        Self {
            service: BroadcastService::new(BroadcastRepository::new(db), livekit, redis),
        }
    }
}
