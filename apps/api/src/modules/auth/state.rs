use crate::config::MenoConfig;
use crate::modules::auth::jwt::Jwt;
use crate::modules::auth::services::AuthService;
use crate::shared::integrations::google::GoogleAuthService;
use crate::shared::services::redis::RedisService;

#[derive(Clone)]
pub struct AuthState {
    pub service: AuthService,
    pub jwt: Jwt,
    pub google: GoogleAuthService,
}
impl AuthState {
    pub fn new(db: sqlx::PgPool, redis: RedisService, config: &MenoConfig) -> Self {
        Self {
            service: AuthService::new(db, redis),
            jwt: Jwt::new(
                &config.jwt_secret,
                &config.jwt_refresh_secret,
                config.access_token_expiration,
                config.refresh_token_expiration,
            ),
            google: GoogleAuthService::new(config),
        }
    }
}
