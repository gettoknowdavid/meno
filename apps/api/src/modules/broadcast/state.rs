use crate::jobs::Jobs;
use crate::modules::broadcast::cache::{BroadcastCache, BroadcastRedisCache};
use crate::modules::broadcast::repository::BroadcastRepository;
use crate::modules::broadcast::service::{BroadcastService, DynBroadcastService};
use crate::shared::services::livekit::LivekitService;
use crate::shared::services::redis::Redis;
use crate::shared::services::ws::WsService;
use crate::shared::services::ws::pubsub::WsPubSubBridge;
use std::sync::Arc;

#[derive(Clone)]
pub struct BroadcastState {
    pub service: Arc<DynBroadcastService>,
}

impl BroadcastState {
    pub fn new(
        db: sqlx::PgPool,
        redis: Redis,
        livekit: LivekitService,
        pubsub: Arc<WsPubSubBridge>,
        ws: WsService,
        jobs: Jobs,
    ) -> Self {
        let repo = Arc::new(BroadcastRepository::new(db.clone()));
        let cache: Arc<dyn BroadcastCache> = Arc::new(BroadcastRedisCache::new(redis.clone()));
        let service = Arc::new(
            BroadcastService::builder()
                .repo(Arc::clone(&repo))
                .cache(Arc::clone(&cache))
                .db(db)
                .redis(redis)
                .livekit(livekit)
                .pubsub(pubsub)
                .ws(ws)
                .jobs(jobs)
                .build(),
        );
        Self { service }
    }
}
