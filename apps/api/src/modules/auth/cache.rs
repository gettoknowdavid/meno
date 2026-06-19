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

#[async_trait::async_trait]
pub trait AuthCache: Send + Sync + 'static {
    async fn store_otp(&self, email: &str, otp: &str, otp_type: &OtpType) -> Result<(), AuthError>;
    async fn verify_otp(
        &self,
        email: &str,
        code: &str,
        otp_type: &OtpType,
    ) -> Result<bool, AuthError>;
    async fn revoke_otp(&self, email: &str, otp_type: OtpType) -> Result<(), AuthError>;
    async fn can_resend_otp(&self, email: &str) -> Result<bool, AuthError>;
    async fn set_resend_cooldown(&self, email: &str) -> Result<(), AuthError>;
    async fn store_oauth_state(&self, state: &str, verifier: &str) -> Result<(), AuthError>;
    async fn consume_oauth_state(&self, state: &str) -> Result<String, AuthError>;
    async fn check_login_rate_limit(&self, email: &str) -> Result<(), AuthError>;
    async fn clear_login_rate_limit(&self, email: &str) -> Result<(), AuthError>;
    async fn block_access_token(&self, jti: Uuid, remaining_secs: i64) -> Result<(), AuthError>;
    async fn is_token_blocked(&self, jti: Uuid) -> Result<bool, AuthError>;
    async fn is_user_tokens_blocked(
        &self,
        user_id: Uuid,
        issued_at: i64,
    ) -> Result<bool, AuthError>;
    async fn block_all_user_tokens(&self, user_id: Uuid, ttl: i64) -> Result<(), AuthError>;
}

#[derive(Clone)]
pub struct RedisAuthCache {
    redis: Redis,
}

impl RedisAuthCache {
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }
}

#[async_trait::async_trait]
impl AuthCache for RedisAuthCache {
    async fn store_otp(&self, email: &str, otp: &str, otp_type: &OtpType) -> Result<(), AuthError> {
        let key = RedisKey::otp(email, &otp_type.to_string());
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
        let key = RedisKey::otp(email, &otp_type.to_string());
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
        let key = RedisKey::otp(email, &otp_type.to_string());
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
