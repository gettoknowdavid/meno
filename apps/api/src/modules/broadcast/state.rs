use crate::modules::broadcast::repository::{BroadcastRepo, BroadcastRepository};
use crate::modules::broadcast::service::BroadcastService;
use crate::shared::services::livekit::LivekitService;
use crate::shared::services::redis::Redis;
use std::sync::Arc;

#[derive(Clone)]
pub struct BroadcastState {
    pub service: Arc<BroadcastService>,
}

impl BroadcastState {
    pub fn new(db: sqlx::PgPool, redis: Redis, livekit: LivekitService) -> Self {
        let repo: Arc<dyn BroadcastRepo> = Arc::new(BroadcastRepository::new(db));
        let service = Arc::new(BroadcastService::new(Arc::clone(&repo), livekit, redis));
        Self { service }
    }
}
