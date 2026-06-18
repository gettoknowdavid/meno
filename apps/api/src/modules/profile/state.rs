use crate::modules::profile::service::ProfileService;
use crate::shared::services::redis::Redis;
use crate::shared::services::storage::StorageService;

#[derive(Clone)]
pub struct ProfileState {
    pub service: ProfileService,
}

impl ProfileState {
    pub fn new(db: sqlx::PgPool, redis: Redis, storage: StorageService) -> Self {
        Self {
            service: ProfileService::new(db, redis, storage),
        }
    }
}
