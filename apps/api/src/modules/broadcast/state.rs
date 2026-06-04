use crate::modules::broadcast::repository::BroadcastRepository;
use crate::modules::broadcast::service::BroadcastService;
use crate::shared::services::livekit::LivekitService;
use crate::shared::services::redis::RedisService;
use crate::shared::services::ws::WsService;

#[derive(Clone)]
pub struct BroadcastState {
    pub service: BroadcastService,
}

impl BroadcastState {
    pub fn new(
        db: sqlx::PgPool,
        redis: RedisService,
        livekit: LivekitService,
        ws: WsService,
    ) -> Self {
        Self {
            service: BroadcastService::new(
                BroadcastRepository::new(db),
                livekit,
                redis,
                ws,
            ),
        }
    }
}