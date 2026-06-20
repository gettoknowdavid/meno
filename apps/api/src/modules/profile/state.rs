#[derive(Clone)]
pub struct ProfileState {
    pub service: crate::modules::profile::service::ProfileService,
}

impl ProfileState {
    pub fn new(
        db: sqlx::PgPool,
        redis: crate::shared::services::redis::Redis,
        storage: crate::shared::services::storage::StorageService,
    ) -> Self {
        Self {
            service: crate::modules::profile::service::ProfileService::new(db, redis, storage),
        }
    }
}
