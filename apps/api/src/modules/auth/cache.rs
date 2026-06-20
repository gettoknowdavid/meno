use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::OtpType;
use crate::shared::services::redis::Redis;
use crate::shared::services::redis::keys::RedisKey;
use fred::interfaces::{HashesInterface, KeysInterface};
use std::collections::HashMap;
use uuid::Uuid;

const TTL_OTP_SECS: i64 = 900;
const TTL_COOLDOWN_SECS: i64 = 60;
const TTL_RATE_WINDOW_SECS: i64 = 900;
const MAX_LOGIN_ATTEMPTS: u64 = 10;
const TTL_ACCESS_TOKEN_BLOCK_MAX: i64 = 900;

#[derive(Clone)]
pub struct AuthRedisCache {
    redis: Redis,
}

impl AuthRedisCache {
    /// Creates a new instance of `AuthRedisCache`.
    ///
    /// # Parameters
    /// - `redis`: The Redis client instance used for caching.
    ///
    /// # Returns
    /// - A new `AuthRedisCache` instance.
    #[must_use]
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }
}

#[async_trait::async_trait]
pub trait AuthCache: Send + Sync + 'static {
    /// Stores a one-time password (OTP) in the cache with a predefined TTL.
    ///
    /// # Parameters
    /// - `email`: The email address associated with the OTP.
    /// - `otp`: The generated OTP string.
    /// - `otp_type`: The type of OTP (e.g., email verification, password reset).
    ///
    /// # Returns
    /// - `Ok(())` if the OTP was successfully stored.
    ///
    /// # Errors
    /// - `AuthError::Redis` if the cache operation fails.
    async fn store_otp(&self, email: &str, otp: &str, otp_type: &OtpType) -> Result<(), AuthError>;

    /// Verifies a provided OTP against the stored value in the cache.
    ///
    /// If the OTP matches, it is automatically revoked (deleted) from the cache.
    ///
    /// # Parameters
    /// - `email`: The email address associated with the OTP.
    /// - `code`: The OTP code provided by the user.
    /// - `otp_type`: The type of OTP being verified.
    ///
    /// # Returns
    /// - `Ok(true)` if the OTP is valid and was deleted.
    /// - `Ok(false)` if the OTP does not match.
    ///
    /// # Errors
    /// - `AuthError::InvalidOtp` if no OTP is found for the given email and type.
    /// - `AuthError::Redis` if the cache operation fails.
    async fn verify_otp(
        &self,
        email: &str,
        code: &str,
        otp_type: &OtpType,
    ) -> Result<bool, AuthError>;

    /// Revokes (deletes) an OTP from the cache.
    ///
    /// # Parameters
    /// - `email`: The email address associated with the OTP.
    /// - `otp_type`: The type of OTP to revoke.
    ///
    /// # Returns
    /// - `Ok(())` if the operation completes successfully.
    ///
    /// # Errors
    /// - `AuthError::Redis` if the cache operation fails.
    async fn revoke_otp(&self, email: &str, otp_type: OtpType) -> Result<(), AuthError>;

    /// Checks if a new OTP can be sent to the specified email (rate limiting).
    ///
    /// # Parameters
    /// - `email`: The email address to check.
    ///
    /// # Returns
    /// - `Ok(true)` if an OTP can be sent.
    /// - `Ok(false)` if the user is currently in a cooldown period.
    ///
    /// # Errors
    /// - `AuthError::Redis` if the cache operation fails.
    async fn can_resend_otp(&self, email: &str) -> Result<bool, AuthError>;

    /// Sets a cooldown period during which no new OTPs can be sent to the email.
    ///
    /// # Parameters
    /// - `email`: The email address to set the cooldown for.
    ///
    /// # Returns
    /// - `Ok(())` if the cooldown was successfully set.
    ///
    /// # Errors
    /// - `AuthError::Redis` if the cache operation fails.
    async fn set_resend_cooldown(&self, email: &str) -> Result<(), AuthError>;

    /// Stores the OAuth2 state and PKCE code verifier.
    ///
    /// # Parameters
    /// - `state`: The CSRF state token.
    /// - `verifier`: The PKCE code verifier.
    ///
    /// # Returns
    /// - `Ok(())` if the state was successfully stored.
    ///
    /// # Errors
    /// - `AuthError::Redis` if the cache operation fails.
    async fn store_oauth_state(&self, state: &str, verifier: &str) -> Result<(), AuthError>;

    /// Consumes (retrieves and deletes) the PKCE code verifier for a given OAuth2 state.
    ///
    /// # Parameters
    /// - `state`: The CSRF state token to consume.
    ///
    /// # Returns
    /// - `Ok(String)` containing the PKCE code verifier.
    ///
    /// # Errors
    /// - `AuthError::InvalidToken` if the state is missing or invalid.
    /// - `AuthError::Redis` if the cache operation fails.
    async fn consume_oauth_state(&self, state: &str) -> Result<String, AuthError>;

    /// Increments and checks the login rate limit for an email.
    ///
    /// # Parameters
    /// - `email`: The email address attempting to login.
    ///
    /// # Returns
    /// - `Ok(())` if the attempt is within limits.
    ///
    /// # Errors
    /// - `AuthError::TooManyRequests` if the rate limit is exceeded.
    /// - `AuthError::Redis` if the cache operation fails.
    async fn check_login_rate_limit(&self, email: &str) -> Result<(), AuthError>;

    /// Clears the login rate limit for an email (usually after a successful login).
    ///
    /// # Parameters
    /// - `email`: The email address to clear the limit for.
    ///
    /// # Returns
    /// - `Ok(())` if the limit was cleared.
    ///
    /// # Errors
    /// - `AuthError::Redis` if the cache operation fails.
    async fn clear_login_rate_limit(&self, email: &str) -> Result<(), AuthError>;

    /// Blocks an access token JTI for a specified duration.
    ///
    /// # Parameters
    /// - `jti`: The Unique ID of the token to block.
    /// - `remaining_secs`: The number of seconds until the token naturally expires.
    ///
    /// # Returns
    /// - `Ok(())` if the token was successfully blocked.
    ///
    /// # Errors
    /// - `AuthError::Redis` if the cache operation fails.
    async fn block_access_token(&self, jti: Uuid, remaining_secs: i64) -> Result<(), AuthError>;

    /// Checks if an access token JTI is currently blocked.
    ///
    /// # Parameters
    /// - `jti`: The Unique ID of the token to check.
    ///
    /// # Returns
    /// - `Ok(true)` if the token is blocked.
    /// - `Ok(false)` otherwise.
    ///
    /// # Errors
    /// - `AuthError::Redis` if the cache operation fails.
    async fn is_token_blocked(&self, jti: Uuid) -> Result<bool, AuthError>;

    /// Checks if all tokens for a user issued before a certain timestamp are blocked.
    ///
    /// # Parameters
    /// - `user_id`: The ID of the user.
    /// - `issued_at`: The timestamp the token was issued at.
    ///
    /// # Returns
    /// - `Ok(true)` if the tokens are blocked.
    /// - `Ok(false)` otherwise.
    ///
    /// # Errors
    /// - `AuthError::Redis` if the cache operation fails.
    async fn is_user_tokens_blocked(
        &self,
        user_id: Uuid,
        issued_at: i64,
    ) -> Result<bool, AuthError>;

    /// Blocks all current access tokens for a user.
    ///
    /// Typically used during password resets or security breaches.
    ///
    /// # Parameters
    /// - `user_id`: The ID of the user.
    /// - `ttl`: The duration (in seconds) to keep the block active.
    ///
    /// # Returns
    /// - `Ok(())` if the block was successfully applied.
    ///
    /// # Errors
    /// - `AuthError::Redis` if the cache operation fails.
    async fn block_all_user_tokens(&self, user_id: Uuid, ttl: i64) -> Result<(), AuthError>;
}

#[async_trait::async_trait]
impl AuthCache for AuthRedisCache {
    async fn store_otp(&self, email: &str, otp: &str, otp_type: &OtpType) -> Result<(), AuthError> {
        let key = RedisKey::otp(email, otp_type.as_ref());
        self.redis
            .set::<String>(&key, &otp.to_string(), Some(TTL_OTP_SECS))
            .await
            .map_err(AuthError::Redis)
    }

    async fn verify_otp(
        &self,
        email: &str,
        code: &str,
        otp_type: &OtpType,
    ) -> Result<bool, AuthError> {
        let key = RedisKey::otp(email, otp_type.as_ref());
        let stored: Option<String> = self.redis.get(&key).await.map_err(AuthError::Redis)?;
        match stored {
            Some(ref s) if s == code => {
                self.redis.del(&key).await.map_err(AuthError::Redis)?;
                Ok(true)
            }
            Some(_) => Ok(false),
            None => Err(AuthError::InvalidOtp),
        }
    }

    async fn revoke_otp(&self, email: &str, otp_type: OtpType) -> Result<(), AuthError> {
        let key = RedisKey::otp(email, otp_type.as_ref());
        self.redis.del(&key).await.map_err(AuthError::Redis)?;
        Ok(())
    }

    async fn can_resend_otp(&self, email: &str) -> Result<bool, AuthError> {
        let key = RedisKey::rate_limit("OTP_RESEND", email);
        let exists: Option<String> = self.redis.get(&key).await.map_err(AuthError::Redis)?;
        Ok(exists.is_none())
    }

    async fn set_resend_cooldown(&self, email: &str) -> Result<(), AuthError> {
        let key = RedisKey::rate_limit("OTP_RESEND", email);
        self.redis
            .set::<String>(&key, &"1".to_string(), Some(TTL_COOLDOWN_SECS))
            .await
            .map_err(AuthError::Redis)
    }

    async fn store_oauth_state(&self, state: &str, verifier: &str) -> Result<(), AuthError> {
        let key = RedisKey::oauth2_state(state);
        let mut fields = HashMap::new();
        fields.insert("csrf_token".to_string(), "true".to_string());
        fields.insert("pkce_code_verifier".to_string(), verifier.to_string());

        let pipeline = self.redis.pipeline();
        pipeline
            .hset::<(), _, _>(key.as_ref(), fields)
            .await
            .map_err(AuthError::Redis)?;
        pipeline
            .expire::<(), _>(key.as_ref(), 300, None)
            .await
            .map_err(AuthError::Redis)?;
        pipeline.all::<()>().await.map_err(AuthError::Redis)?;
        Ok(())
    }

    async fn consume_oauth_state(&self, state: &str) -> Result<String, AuthError> {
        let key = RedisKey::oauth2_state(state);
        let data: HashMap<String, String> =
            self.redis.hgetall(&key).await.map_err(AuthError::Redis)?;
        let _ = self.redis.del(&key).await;

        match (data.get("csrf_token"), data.get("pkce_code_verifier")) {
            (Some(csrf), Some(verifier)) if csrf == "true" => Ok(verifier.clone()),
            _ => Err(AuthError::InvalidToken),
        }
    }

    async fn check_login_rate_limit(&self, email: &str) -> Result<(), AuthError> {
        let key = RedisKey::rate_limit("LOGIN_ATTEMPTS", email);
        let count: u64 = self
            .redis
            .incr_and_expire_if_first(&key, TTL_RATE_WINDOW_SECS)
            .await
            .map_err(AuthError::Redis)?;

        if count > MAX_LOGIN_ATTEMPTS {
            return Err(AuthError::TooManyRequests);
        }
        Ok(())
    }

    async fn clear_login_rate_limit(&self, email: &str) -> Result<(), AuthError> {
        let key = RedisKey::rate_limit("LOGIN_ATTEMPTS", email);
        self.redis.del(&key).await.map_err(AuthError::Redis)?;
        Ok(())
    }

    async fn block_access_token(&self, jti: Uuid, remaining_secs: i64) -> Result<(), AuthError> {
        // Clamp to max so we don't store stale blocklist entries forever
        let ttl = remaining_secs.min(TTL_ACCESS_TOKEN_BLOCK_MAX);
        if ttl <= 0 {
            return Ok(());
        }
        let key = RedisKey::block_list("ACCESS_TOKEN", jti);
        self.redis
            .set::<String>(&key, &"BLOCKED".to_string(), Some(ttl))
            .await
            .map_err(AuthError::Redis)
    }

    async fn is_token_blocked(&self, jti: Uuid) -> Result<bool, AuthError> {
        let key = RedisKey::block_list("ACCESS_TOKEN", jti);
        self.redis.exists(&key).await.map_err(AuthError::Redis)
    }

    async fn is_user_tokens_blocked(
        &self,
        user_id: Uuid,
        issued_at: i64,
    ) -> Result<bool, AuthError> {
        let key = RedisKey::block_list("ALL_USER_ACCESS_TOKENS", user_id);
        let blocked_at: Option<i64> = self.redis.get(&key).await.map_err(AuthError::Redis)?;
        Ok(blocked_at.is_some_and(|b| issued_at < b))
    }

    async fn block_all_user_tokens(&self, user_id: Uuid, ttl: i64) -> Result<(), AuthError> {
        let key = RedisKey::block_list("ALL_USER_ACCESS_TOKENS", user_id);
        let blocked_at = time::OffsetDateTime::now_utc().unix_timestamp();
        self.redis
            .set::<i64>(&key, &blocked_at, Some(ttl))
            .await
            .map_err(AuthError::Redis)
    }
}
