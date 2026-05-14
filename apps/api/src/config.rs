use crate::shared::middleware::rate_limit::RateLimitConfig;
use anyhow::{Context, Result};
use std::env::var;

#[derive(Clone)]
pub struct MenoConfig {
    pub livekit_api_key: String,
    pub livekit_api_secret: String,

    pub aws_region: String,
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,

    pub database_url: String,

    pub env: String,
    pub port: u16,

    pub email_url: String,
    pub cloudinary_url: String,

    pub firebase_service_account_url: String,

    pub google_client_id: String,
    pub google_secret: String,
    pub google_accounts_password: String,

    pub jwt_secret: String,
    pub jwt_refresh_secret: String,
    pub token_expiration_time: String,

    pub redis_url: String,

    pub default_rate_limit: RateLimitConfig,

    pub origins: Vec<String>,
}

impl MenoConfig {
    pub fn from_env() -> Result<MenoConfig> {
        Ok(MenoConfig {
            livekit_api_key: var("LIVEKIT_API_KEY").context("LIVEKIT_API_KEY is missing")?,
            livekit_api_secret: var("LIVEKIT_API_SECRET")
                .context("LIVEKIT_API_SECRET is missing")?,
            aws_region: var("AWS_REGION").context("AWS_REGION is missing")?,
            aws_access_key_id: var("AWS_ACCESS_KEY_ID").context("AWS_ACCESS_KEY_ID is missing")?,
            aws_secret_access_key: var("AWS_SECRET_ACCESS_KEY")
                .context("AWS_SECRET_ACCESS_KEY is missing")?,
            database_url: var("DATABASE_URL").context("DATABASE_URL is missing")?,
            env: var("ENV").unwrap_or_else(|_| "dev".to_string()),
            port: var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse::<u16>()
                .context("PORT must be a valid number")?,
            email_url: var("EMAIL_URL").context("EMAIL_URL is missing")?,
            cloudinary_url: var("CLOUDINARY_URL").context("CLOUDINARY_URL is missing")?,
            firebase_service_account_url: var("FIREBASE_SERVICE_ACCOUNT_URL")
                .context("FIREBASE_SERVICE_ACCOUNT_URL is missing")?,
            google_client_id: var("GOOGLE_CLIENT_ID").context("GOOGLE_CLIENT_ID is missing")?,
            google_secret: var("GOOGLE_SECRET").context("GOOGLE_SECRET is missing")?,
            google_accounts_password: var("GOOGLE_ACCOUNTS_PASSWORD")
                .context("GOOGLE_ACCOUNTS_PASSWORD is missing")?,
            jwt_secret: var("JWT_SECRET").context("JWT_SECRET is missing")?,
            jwt_refresh_secret: var("JWT_REFRESH_SECRET")
                .context("JWT_REFRESH_SECRET is missing")?,
            token_expiration_time: var("TOKEN_EXPIRATION_TIME")
                .context("TOKEN_EXPIRATION_TIME is missing")?,
            redis_url: var("REDIS_URL").context("REDIS_URL is missing")?,
            default_rate_limit: RateLimitConfig::new(60, 60),
            origins: vec![
                "https://app.yourdomain.com".to_string(),
                "https://staging.yourdomain.com".to_string(),
            ],
        })
    }
}
