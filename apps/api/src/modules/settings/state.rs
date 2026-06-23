use crate::modules::profile::cache::{ProfileCache, ProfileRedisCache};
use crate::modules::settings::repository::{SettingsRepo, SettingsRepository};
use crate::modules::settings::service::SettingsService;
use crate::shared::services::redis::Redis;
use std::sync::Arc;

#[derive(Clone)]
pub struct SettingsState {
    pub service: SettingsService,
}

impl SettingsState {
    pub fn new(db: sqlx::PgPool, redis: Redis) -> Self {
        let repo: Arc<dyn SettingsRepo> = Arc::new(SettingsRepository::new(db));
        let profile_cache: Arc<dyn ProfileCache> = Arc::new(ProfileRedisCache::new(redis));
        Self {
            service: SettingsService::new(repo, profile_cache),
        }
    }
}
