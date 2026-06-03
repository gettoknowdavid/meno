use crate::modules::profile::service::ProfileService;
use crate::shared::services::redis::RedisService;
use crate::shared::services::storage::StorageService;

#[derive(Clone)]
pub struct ProfileState {
    pub service: ProfileService,
}

impl ProfileState {
    pub fn new(db: sqlx::PgPool, redis: RedisService, storage: StorageService) -> Self {
        Self {
            service: ProfileService::new(db, redis, storage),
        }
    }
}
