use crate::config::Config;
use crate::jobs::Jobs;
use crate::modules::auth::jwt::{Jwt, JwtConfig};
use crate::modules::auth::services::AuthService;
use crate::shared::services::redis::Redis;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AuthState {
    pub service: AuthService,
    pub jwt: Jwt,
}
impl AuthState {
    pub fn new(db: PgPool, redis: Redis, config: std::sync::Arc<Config>, jobs: Jobs) -> Self {
        let jwt = Jwt::new(&JwtConfig::from_config(&config));
        let service = AuthService::new(db, redis, jobs, config, jwt.clone());
        Self { service, jwt }
    }
}
