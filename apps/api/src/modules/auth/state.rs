use crate::modules::auth::cache::{AuthCache, AuthRedisCache};
use crate::modules::auth::repository::{AuthRepo, AuthRepository};
use std::sync::Arc;

/// Everything the auth module needs at runtime, already wired and ready.
#[derive(Clone)]
pub struct AuthState {
    pub service: Arc<crate::modules::auth::services::AuthService>,
    pub tokens: Arc<crate::modules::auth::token::TokenService>,
}

impl AuthState {
    #[must_use]
    pub fn new(
        db: sqlx::PgPool,
        redis: crate::shared::services::redis::Redis,
        config: &crate::config::Config,
        jobs: crate::jobs::Jobs,
    ) -> Self {
        let repo: Arc<dyn AuthRepo> = Arc::new(AuthRepository::new(db));
        let cache: Arc<dyn AuthCache> = Arc::new(AuthRedisCache::new(redis));
        let google = Arc::new(crate::shared::integrations::google::GoogleAuth::new(config));

        let token_config = crate::modules::auth::token::TokenConfig {
            access_secret: config.jwt_secret.clone(),
            refresh_secret: config.jwt_refresh_secret.clone(),
            access_ttl_secs: config.access_token_expiration,
            refresh_ttl_secs: config.refresh_token_expiration,
        };

        let tokens = Arc::new(crate::modules::auth::token::TokenService::new(
            token_config,
            Arc::clone(&repo),
            Arc::clone(&cache),
        ));

        let service = Arc::new(crate::modules::auth::services::AuthService::new(
            Arc::clone(&repo),
            Arc::clone(&cache),
            Arc::clone(&tokens),
            google,
            jobs,
        ));

        Self { service, tokens }
    }
}
