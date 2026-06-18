use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::OtpType;
use crate::shared::constants::{MAX_LOGIN_ATTEMPTS, TTL_60_SECS, TTL_300_SECS, TTL_900_SECS};
use crate::shared::services::redis::Redis;
use crate::shared::services::redis::keys::RedisKey;
use fred::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthCache {
    redis: Redis,
}
impl AuthCache {
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }

    pub async fn block_access_token(
        &self,
        jti: Uuid,
        remaining_secs: i64,
    ) -> Result<(), AuthError> {
        let key = RedisKey::block_list("ACCESS_TOKEN", jti);
        self.redis
            .set::<String>(&key, &"BLOCKED".to_string(), Some(remaining_secs))
            .await
            .map_err(AuthError::Redis)?;
        Ok(())
    }
    pub async fn block_all_user_access_tokens(
        &self,
        user_id: Uuid,
        access_token_ttl_secs: i64,
    ) -> Result<(), AuthError> {
        let key = RedisKey::block_list("ALL_USER_ACCESS_TOKENS", user_id);
        self.redis
            .set::<String>(&key, &"BLOCKED".to_string(), Some(access_token_ttl_secs))
            .await
            .map_err(AuthError::Redis)
    }
    pub async fn store_otp(
        &self,
        email: &str,
        otp: &str,
        otp_type: &OtpType,
    ) -> Result<(), AuthError> {
        let key = RedisKey::otp(&email, &otp_type.to_string());
        self.redis
            .set::<String>(&key, &otp.to_string(), Some(TTL_900_SECS))
            .await
            .map_err(AuthError::Redis)
    }
    pub async fn verify_otp(
        &self,
        email: &str,
        code: &str,
        otp_type: &OtpType,
    ) -> Result<bool, AuthError> {
        let key = RedisKey::otp(&email, &otp_type.to_string());
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
    pub async fn revoke_otp(&self, email: &str, otp_type: OtpType) -> Result<(), AuthError> {
        let key = RedisKey::otp(&email, &otp_type.to_string());
        self.redis.del(&key).await.map_err(AuthError::Redis)?;
        Ok(())
    }
    pub async fn can_resend_otp(&self, email: &str) -> Result<bool, AuthError> {
        let key = RedisKey::rate_limit("OTP_RESEND", email);
        let exists: Option<String> = self.redis.get(&key).await.map_err(AuthError::Redis)?;
        Ok(exists.is_none())
    }
    pub async fn set_resend_cooldown(&self, email: &str) -> Result<(), AuthError> {
        let key = RedisKey::rate_limit("OTP_RESEND", email);
        let value = "1".to_string();
        self.redis
            .set::<String>(&key, &value, Some(TTL_60_SECS))
            .await
            .map_err(AuthError::Redis)
    }
    pub async fn store_oauth_state(
        &self,
        state: &String,
        verifier: &String,
    ) -> Result<(), AuthError> {
        let key = RedisKey::oauth2_state(&state);

        let mut fields = HashMap::new();
        fields.insert("csrf_token", "true");
        fields.insert("pkce_code_verifier", verifier);

        let pipeline = self.redis.pipeline();

        pipeline
            .hset::<(), _, _>(key.as_ref(), fields)
            .await
            .map_err(AuthError::Redis)?;

        pipeline
            .expire::<(), _>(key.as_ref(), TTL_300_SECS, None)
            .await
            .map_err(AuthError::Redis)?;

        pipeline.all::<()>().await.map_err(AuthError::Redis)?;

        Ok(())
    }

    pub async fn consumes_oauth_state(&self, state: &String) -> Result<String, AuthError> {
        let key = RedisKey::oauth2_state(&state);

        let data: HashMap<String, String> =
            self.redis.hgetall(&key).await.map_err(AuthError::Redis)?;

        let _ = self.redis.del(&key).await;

        let has_csrf = data.get("csrf_token");
        let pkce_code_verifier = data.get("pkce_code_verifier");

        match (has_csrf, pkce_code_verifier) {
            (Some(csrf), Some(verifier)) if csrf == "true" => Ok(verifier.clone()),
            _ => Err(AuthError::InvalidToken),
        }
    }
    pub async fn check_login_rate_limit(&self, email: &str) -> Result<(), AuthError> {
        let key = RedisKey::rate_limit("LOGIN_ATTEMPTS", &email);

        let count: u64 = self
            .redis
            .incr_and_expire_if_first(&key, TTL_900_SECS)
            .await
            .map_err(AuthError::Redis)?;

        if count > MAX_LOGIN_ATTEMPTS {
            return Err(AuthError::TooManyRequests);
        }

        Ok(())
    }
    pub async fn clear_login_rate_limit(&self, email: &str) -> Result<(), AuthError> {
        let key = RedisKey::rate_limit("LOGIN_ATTEMPTS", &email);
        self.redis.del(&key).await.map_err(AuthError::Redis)?;
        Ok(())
    }
    pub async fn invalidate_all_user_keys(&self, user_id: Uuid) -> Result<u64, AuthError> {
        self.redis
            .invalidate_all_user_keys(user_id)
            .await
            .map_err(AuthError::Redis)
    }
}
