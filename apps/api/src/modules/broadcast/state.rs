use crate::modules::broadcast::cache::{BroadcastCache, BroadcastRedisCache};
use crate::modules::broadcast::repository::BroadcastRepository;
use crate::modules::broadcast::service::{BroadcastService, DynBroadcastService};

use std::sync::Arc;

#[derive(Clone)]
pub struct BroadcastState {
    pub service: DynBroadcastService,
}

impl BroadcastState {
    pub fn new(
        db: sqlx::PgPool,
        redis: crate::shared::services::redis::Redis,
        livekit: crate::shared::services::livekit::LivekitService,
        pubsub: Arc<crate::shared::services::ws::pubsub::WsPubSubBridge>,
        ws: crate::shared::services::ws::WsService,
        jobs: crate::jobs::Jobs,
    ) -> Self {
        let repo = Arc::new(BroadcastRepository::new(db.clone()));
        let cache: Arc<dyn BroadcastCache> = Arc::new(BroadcastRedisCache::new(redis.clone()));
        let service = BroadcastService::builder()
            .repo(Arc::clone(&repo))
            .cache(Arc::clone(&cache))
            .db(db)
            .redis(redis)
            .livekit(livekit)
            .pubsub(pubsub)
            .ws(ws)
            .jobs(jobs)
            .build();
        Self { service }
    }
}
