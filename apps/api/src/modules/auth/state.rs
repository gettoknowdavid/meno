use crate::config::Config;
use crate::modules::auth::cache::{AuthCache, AuthRedisCache};
use crate::modules::auth::repository::{AuthRepo, AuthRepository};
use crate::modules::auth::services::AuthService;
use crate::modules::auth::token::TokenService;
use crate::shared::integrations::google::GoogleAuth;
use crate::shared::services::redis::Redis;
use std::sync::Arc;

/// Everything the auth module needs at runtime, already wired and ready.
#[derive(Clone)]
pub struct AuthState {
    pub service: Arc<AuthService>,
    pub tokens: Arc<TokenService>,
}

impl AuthState {
    #[must_use]
    pub fn new(db: sqlx::PgPool, redis: Redis, config: &Config, jobs: crate::jobs::Jobs) -> Self {
        let repo: Arc<dyn AuthRepo> = Arc::new(AuthRepository::new(db));
        let cache: Arc<dyn AuthCache> = Arc::new(AuthRedisCache::new(redis));
        let google = Arc::new(GoogleAuth::new(config));

        let token_config = crate::modules::auth::token::TokenConfig {
            access_secret: config.jwt_secret.clone(),
            refresh_secret: config.jwt_refresh_secret.clone(),
            access_ttl_secs: config.access_token_expiration,
            refresh_ttl_secs: config.refresh_token_expiration,
        };

        let tokens = Arc::new(TokenService::new(
            token_config,
            Arc::clone(&repo),
            Arc::clone(&cache),
        ));

        let service = Arc::new(AuthService::new(
            Arc::clone(&repo),
            Arc::clone(&cache),
            Arc::clone(&tokens),
            google,
            jobs,
        ));

        Self { service, tokens }
    }
}
