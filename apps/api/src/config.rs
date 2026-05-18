use crate::shared::middleware::rate_limit::RateLimitConfig;
use anyhow::{Context, Result};
use std::env::var;

#[derive(Clone)]
pub struct MenoConfig {
    // pub livekit_api_key: String,
    // pub livekit_api_secret: String,
    //
    // pub aws_region: String,
    // pub aws_access_key_id: String,
    // pub aws_secret_access_key: String,

    // pub email_url: String,
    // pub cloudinary_url: String,

    // pub firebase_service_account_url: String,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub google_redirect_uri: String,
    pub google_auth_uri: String,
    pub google_token_uri: String,

    pub database_url: String,

    pub env: String,
    pub port: u16,

    pub jwt_secret: String,
    pub jwt_refresh_secret: String,
    pub access_token_expiration: u64,
    pub refresh_token_expiration: u64,

    pub redis_url: String,

    pub default_rate_limit: RateLimitConfig,

    pub origins: Vec<String>,

    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_password: String,
    pub smtp_from: String,
}

impl MenoConfig {
    pub fn from_env() -> Result<MenoConfig> {
        dotenvy::dotenv().ok();
        Ok(MenoConfig {
            // livekit_api_key: var("LIVEKIT_API_KEY").context("LIVEKIT_API_KEY is missing")?,
            // livekit_api_secret: var("LIVEKIT_API_SECRET").context("LIVEKIT_API_SECRET is missing")?,
            // aws_region: var("AWS_REGION").context("AWS_REGION is missing")?,
            // aws_access_key_id: var("AWS_ACCESS_KEY_ID").context("AWS_ACCESS_KEY_ID is missing")?,
            // aws_secret_access_key: var("AWS_SECRET_ACCESS_KEY").context("AWS_SECRET_ACCESS_KEY is missing")?,
            // email_url: var("EMAIL_URL").context("EMAIL_URL is missing")?,
            // cloudinary_url: var("CLOUDINARY_URL").context("CLOUDINARY_URL is missing")?,
            // firebase_service_account_url: var("FIREBASE_SERVICE_ACCOUNT_URL").context("FIREBASE_SERVICE_ACCOUNT_URL is missing")?,
            google_client_id: var("GOOGLE_CLIENT_ID").context("GOOGLE_CLIENT_ID is missing")?,
            google_client_secret: var("GOOGLE_CLIENT_SECRET")
                .context("GOOGLE_CLIENT_SECRET is missing")?,
            google_redirect_uri: var("GOOGLE_REDIRECT_URI")
                .context("GOOGLE_REDIRECT_URI is missing")?,
            google_auth_uri: var("GOOGLE_AUTH_URI").context("GOOGLE_AUTH_URI is missing")?,
            google_token_uri: var("GOOGLE_TOKEN_URI").context("GOOGLE_TOKEN_URI is missing")?,
            database_url: var("DATABASE_URL").context("DATABASE_URL is missing")?,
            env: var("ENV").unwrap_or_else(|_| "dev".to_string()),
            port: var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse::<u16>()
                .context("PORT must be a valid number")?,
            jwt_secret: var("JWT_SECRET").context("JWT_SECRET is missing")?,
            jwt_refresh_secret: var("JWT_REFRESH_SECRET")
                .context("JWT_REFRESH_SECRET is missing")?,
            access_token_expiration: var("ACCESS_TOKEN_EXPIRATION")
                .unwrap_or_else(|_| "900".to_string())
                .parse::<u64>()
                .context("ACCESS_TOKEN_EXPIRATION must be a valid number")?,
            refresh_token_expiration: var("REFRESH_TOKEN_EXPIRATION")
                .unwrap_or_else(|_| "604800".to_string())
                .parse::<u64>()
                .context("REFRESH_TOKEN_EXPIRATION must be a valid number")?,
            redis_url: var("REDIS_URL").context("REDIS_URL is missing")?,
            default_rate_limit: RateLimitConfig::new(60, 60),
            origins: vec![
                "https://app.yourdomain.com".to_string(),
                "https://staging.yourdomain.com".to_string(),
            ],
            smtp_host: var("SMTP_HOST").context("SMTP_HOST is missing")?,
            smtp_port: var("SMTP_PORT")
                .unwrap_or_else(|_| "465".to_string())
                .parse::<u16>()
                .context("SMTP_PORT must be a valid number")?,
            smtp_user: var("SMTP_USER").context("SMTP_USER is missing")?,
            smtp_password: var("SMTP_PASSWORD").context("SMTP_PASSWORD is missing")?,
            smtp_from: var("SMTP_FROM").context("SMTP_FROM is missing")?,
        })
    }
}
