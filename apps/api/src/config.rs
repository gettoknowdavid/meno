use crate::shared::middleware::rate_limit::RateLimitConfig;
use anyhow::{Context, Result, anyhow};
use std::env::var;
use std::fs::read_to_string;

#[derive(Clone)]
pub struct Config {
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub livekit_host: String,

    // pub aws_region: String,
    // pub aws_access_key_id: String,
    // pub aws_secret_access_key: String,

    // pub email_url: String,
    // pub cloudinary_url: String,
    pub firebase_project_id: String,
    pub firebase_service_account_json: String,

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
    pub access_token_expiration: i64,
    pub refresh_token_expiration: i64,

    pub redis_url: String,

    pub default_rate_limit: RateLimitConfig,

    pub origins: Vec<String>,

    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_password: String,
    pub smtp_from: String,

    pub storage_endpoint: String,
    pub storage_access_key: String,
    pub storage_secret_key: String,
    pub storage_bucket: String,
    pub storage_region: String,
    pub storage_public_url: String,
}

impl Config {
    /// Attempts to load the configuration for the application environment variables and secrets.
    ///
    /// This function initializes environment variables using `dotenvy` and retrieves the required
    /// configuration values from the environment or errors if they are not properly set or invalid.
    ///
    /// # Returns
    ///
    /// `Result<Config>` -
    /// - On success, returns a `Config` struct containing all the necessary configuration values.
    /// - On failure, returns an `anyhow::Error` describing the missing or invalid configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the required environment variables are missing or contain invalid values:
    /// - `FIREBASE_SERVICE_ACCOUNT_PATH` must point to a valid JSON file.
    /// - Critical environment variables such as API keys, secrets, URLs, and token expiration settings must be present.
    /// - Any value that requires a valid numeric format (e.g., `PORT`, `ACCESS_TOKEN_EXPIRATION`, etc.) must be parseable into the expected type.
    ///
    /// # Environment Variables
    ///
    /// The following environment variables are used in this function:
    /// - `FIREBASE_SERVICE_ACCOUNT_PATH`: Path to the Firebase service account JSON file.
    /// - `LIVEKIT_API_KEY`
    /// - `LIVEKIT_API_SECRET`
    /// - `LIVEKIT_HOST`
    /// - `FIREBASE_PROJECT_ID`
    /// - `GOOGLE_CLIENT_ID`
    /// - `GOOGLE_CLIENT_SECRET`
    /// - `GOOGLE_REDIRECT_URI`
    /// - `GOOGLE_AUTH_URI`
    /// - `GOOGLE_TOKEN_URI`
    /// - `DATABASE_URL`
    /// - `ENV`: Defaults to `"dev"` if not provided.
    /// - `PORT`: Defaults to `8080` if not provided.
    /// - `JWT_SECRET`
    /// - `JWT_REFRESH_SECRET`
    /// - `ACCESS_TOKEN_EXPIRATION`: Defaults to `900` seconds (15 minutes) if not provided.
    /// - `REFRESH_TOKEN_EXPIRATION`: Defaults to `604800` seconds (7 days) if not provided.
    /// - `REDIS_URL`
    /// - `SMTP_HOST`
    /// - `SMTP_PORT`: Defaults to `465` if not provided.
    /// - `SMTP_USER`
    /// - `SMTP_PASSWORD`
    /// - `SMTP_FROM`
    /// - `STORAGE_ENDPOINT`
    /// - `STORAGE_ACCESS_KEY`
    /// - `STORAGE_SECRET_KEY`
    /// - `STORAGE_BUCKET`
    /// - `STORAGE_REGION`
    /// - `STORAGE_PUBLIC_URL`
    ///
    /// # Default Values
    ///
    /// Some environment variables provide default values if they are not explicitly set:
    /// - `ENV`: `"dev"`
    /// - `PORT`: `8080`
    /// - `ACCESS_TOKEN_EXPIRATION`: `900` seconds
    /// - `REFRESH_TOKEN_EXPIRATION`: `604800` seconds
    /// - `SMTP_PORT`: `465`
    ///
    /// # Example
    ///
    /// ```rust
    /// match Config::from_env() {
    ///     Ok(config) => {
    ///         println!("Configuration successfully loaded!");
    ///         // Use the configuration as needed
    ///     }
    ///     Err(e) => {
    ///         eprintln!("Failed to load configuration: {}", e);
    ///     }
    /// }
    /// ```
    ///
    /// # Related
    ///
    /// See the `Config` struct for details on each configuration field.
    pub fn from_env() -> Result<Config> {
        dotenvy::dotenv().ok();

        let service_account_json = if let Ok(p) = var("FIREBASE_SERVICE_ACCOUNT_PATH") {
            read_to_string(&p).context(format!("Failed to read service account from {p}"))?
        } else {
            return Err(anyhow!("FIREBASE_SERVICE_ACCOUNT_PATH is missing"));
        };

        Ok(Config {
            livekit_api_key: var("LIVEKIT_API_KEY").context("LIVEKIT_API_KEY is missing")?,
            livekit_api_secret: var("LIVEKIT_API_SECRET")
                .context("LIVEKIT_API_SECRET is missing")?,
            livekit_host: var("LIVEKIT_HOST").context("LIVEKIT_HOST is missing")?,
            firebase_project_id: var("FIREBASE_PROJECT_ID")
                .context("FIREBASE_PROJECT_ID is missing")?,
            firebase_service_account_json: service_account_json,
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
                .parse::<i64>()
                .context("ACCESS_TOKEN_EXPIRATION must be a valid number")?,
            refresh_token_expiration: var("REFRESH_TOKEN_EXPIRATION")
                .unwrap_or_else(|_| "604800".to_string())
                .parse::<i64>()
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
            storage_endpoint: var("STORAGE_ENDPOINT").context("STORAGE_ENDPOINT is missing")?,
            storage_access_key: var("STORAGE_ACCESS_KEY")
                .context("STORAGE_ACCESS_KEY is missing")?,
            storage_secret_key: var("STORAGE_SECRET_KEY")
                .context("STORAGE_SECRET_KEY is missing")?,
            storage_bucket: var("STORAGE_BUCKET").context("STORAGE_BUCKET is missing")?,
            storage_region: var("STORAGE_REGION").context("STORAGE_REGION is missing")?,
            storage_public_url: var("STORAGE_PUBLIC_URL")
                .context("STORAGE_PUBLIC_URL is missing")?,
        })
    }
}
