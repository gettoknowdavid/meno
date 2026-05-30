use crate::modules::auth::errors::AuthError;
use crate::modules::auth::jwt::hash_token;
use crate::modules::auth::model::{AuthProvider, OtpType, RefreshToken, User, UserIdentity};
use crate::shared::constants::{MAX_LOGIN_ATTEMPTS, TTL_60_SECS, TTL_300_SECS, TTL_900_SECS};
use crate::shared::services::redis::RedisService;
use crate::shared::services::redis::keys::RedisKey;
use fred::prelude::*;
use std::collections::HashMap;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthRepository {
    database: sqlx::PgPool,
    redis: RedisService,
}
impl AuthRepository {
    pub fn new(database: sqlx::PgPool, redis: RedisService) -> Self {
        Self { database, redis }
    }

    // DB
    pub async fn create(&self, full_name: &str, email: &str) -> Result<User, AuthError> {
        let mut tx = self.database.begin().await.map_err(AuthError::Database)?;

        let user = sqlx::query_as!(
            User,
            r#"INSERT INTO users (full_name, email)
               VALUES ($1, $2)
               RETURNING id, full_name, bio, email, avatar_id, avatar_url, verified, role,
                   created_at, updated_at, deleted_at"#,
            full_name,
            email,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(AuthError::Database)?;

        sqlx::query!(
            r#"INSERT INTO general_settings (user_id) VALUES ($1)"#,
            user.id
        )
        .execute(&mut *tx)
        .await
        .map_err(AuthError::Database)?;

        tx.commit().await.map_err(AuthError::Database)?;
        Ok(user)
    }
    pub async fn create_user_tx(&self, full_name: &str, email: &str) -> Result<User, AuthError> {
        let mut tx = self.database.begin().await.map_err(AuthError::Database)?;

        let user = sqlx::query_as!(
            User,
            r#"INSERT INTO users (full_name, email)
               VALUES ($1, $2)
               RETURNING id, full_name, bio, email, avatar_id, avatar_url, verified, role,
                   created_at, updated_at, deleted_at"#,
            full_name,
            email,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(AuthError::Database)?;

        sqlx::query!(
            r#"INSERT INTO general_settings (user_id) VALUES ($1)"#,
            user.id
        )
        .execute(&mut *tx)
        .await
        .map_err(AuthError::Database)?;

        tx.commit().await.map_err(AuthError::Database)?;
        Ok(user)
    }
    pub async fn create_identity(
        &self,
        user_id: Uuid,
        provider_type: &AuthProvider,
        provider_user_id: &str,
        password_hash: Option<&str>,
    ) -> Result<UserIdentity, AuthError> {
        sqlx::query_as!(
            UserIdentity,
            r#"INSERT INTO user_identities (user_id, provider_type, provider_user_id, password_hash)
               VALUES ($1, $2::text, $3, $4)
               RETURNING *"#,
            user_id,
            provider_type.to_string(),
            provider_user_id,
            password_hash,
        )
        .fetch_one(&self.database)
        .await
        .map_err(AuthError::Database)
    }
    pub async fn find_identity(
        &self,
        provider_type: &AuthProvider,
        provider_user_id: &str,
    ) -> Result<Option<UserIdentity>, AuthError> {
        sqlx::query_as!(
            UserIdentity,
            r#"SELECT * FROM user_identities
               WHERE provider_type = $1::text AND provider_user_id = $2"#,
            provider_type.to_string(),
            provider_user_id,
        )
        .fetch_optional(&self.database)
        .await
        .map_err(AuthError::Database)
    }
    pub async fn link_provider(
        &self,
        user_id: Uuid,
        provider_type: &AuthProvider,
        provider_user_id: &str,
    ) -> Result<(), AuthError> {
        sqlx::query!(
            r#"INSERT INTO user_identities (user_id, provider_type, provider_user_id)
               VALUES ($1, $2::text, $3)
               ON CONFLICT (user_id, provider_type) DO NOTHING"#,
            user_id,
            provider_type.to_string(),
            provider_user_id,
        )
        .execute(&self.database)
        .await
        .map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn update_password(&self, user_id: Uuid, hash_pwd: String) -> Result<(), AuthError> {
        sqlx::query!(
            r#"UPDATE user_identities SET password_hash = $1 WHERE user_id = $2"#,
            hash_pwd,
            user_id
        )
        .execute(&self.database)
        .await
        .map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, AuthError> {
        sqlx::query_as!(
            User,
            r#"SELECT id, full_name, bio, email, avatar_id, avatar_url, verified, role, created_at, updated_at, deleted_at
               FROM users WHERE email = $1"#,
            email,
        )
            .fetch_optional(&self.database)
            .await
            .map_err(AuthError::Database)
    }
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AuthError> {
        sqlx::query_as!(
            User,
            r#"SELECT id, full_name, bio, email, avatar_id, avatar_url, verified, role, created_at, updated_at, deleted_at
               FROM users WHERE id = $1"#,
            id
        )
            .fetch_optional(&self.database)
            .await
            .map_err(AuthError::Database)
    }
    pub async fn user_exists(&self, email: &str) -> Result<bool, AuthError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM users WHERE email = $1) AS "exists!""#,
            email
        )
        .fetch_one(&self.database)
        .await
        .map_err(AuthError::Database)
    }
    pub async fn set_verified(&self, email: &str) -> Result<(), AuthError> {
        sqlx::query!("UPDATE users SET verified = true WHERE email = $1", email)
            .execute(&self.database)
            .await
            .map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn store_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
        refresh_token: &str,
        expires_in_secs: i64,
    ) -> Result<(), AuthError> {
        let expires_at = OffsetDateTime::now_utc() + Duration::minutes(expires_in_secs);
        sqlx::query!(
            r#"INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at)
               VALUES ($1, $2, $3, $4)"#,
            jti,
            user_id,
            hash_token(refresh_token),
            expires_at
        )
        .execute(&self.database)
        .await
        .map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn find_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
    ) -> Result<Option<RefreshToken>, AuthError> {
        sqlx::query_as!(
            RefreshToken,
            r#"SELECT * FROM refresh_tokens WHERE id = $1 AND user_id = $2"#,
            jti,
            user_id
        )
        .fetch_optional(&self.database)
        .await
        .map_err(AuthError::Database)
    }
    pub async fn rotate_refresh_token(
        &self,
        user_id: Uuid,
        old_jti: Uuid,
        new_jti: Uuid,
        new_token: &str,
        expires_in_secs: i64,
    ) -> Result<(), AuthError> {
        let expires_at = OffsetDateTime::now_utc() + Duration::minutes(expires_in_secs);

        let mut tx = self.database.begin().await.map_err(AuthError::Database)?;

        sqlx::query!(
            "DELETE FROM refresh_tokens WHERE id = $1 AND user_id = $2",
            old_jti,
            user_id
        )
        .execute(&mut *tx)
        .await
        .map_err(AuthError::Database)?;

        sqlx::query!(
            r#"INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at)
               VALUES ($1, $2, $3, $4)"#,
            new_jti,
            user_id,
            hash_token(new_token),
            expires_at
        )
        .execute(&mut *tx)
        .await
        .map_err(AuthError::Database)?;

        tx.commit().await.map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn revoke_refresh_token(&self, jti: Uuid) -> Result<(), AuthError> {
        sqlx::query!("DELETE FROM refresh_tokens WHERE id = $1", jti)
            .execute(&self.database)
            .await
            .map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn revoke_all_refresh_tokens(&self, user_id: Uuid) -> Result<(), AuthError> {
        sqlx::query!("DELETE FROM refresh_tokens WHERE user_id = $1", user_id)
            .execute(&self.database)
            .await
            .map_err(AuthError::Database)?;
        Ok(())
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
    pub async fn cleanup_expired_refresh_tokens(&self) -> Result<u64, AuthError> {
        let mut total_deleted: u64 = 0;
        loop {
            let chunk_deleted: Option<i64> = sqlx::query_scalar!(
                r#"
                WITH deleted_rows AS (
                    DELETE FROM refresh_tokens
                    WHERE id IN (
                        SELECT id FROM refresh_tokens
                        WHERE expires_at < NOW()
                        LIMIT 5000
                    )
                    RETURNING 1
                )
                SELECT COUNT(*) FROM deleted_rows;
                "#
            )
            .fetch_one(&self.database)
            .await
            .map_err(AuthError::Database)?;

            let count = chunk_deleted.unwrap_or(0) as u64;
            total_deleted += count;

            if count < 5000 {
                // Breaks out of the loop cleanly
                break;
            }

            tokio::task::yield_now().await;
        }
        Ok(total_deleted)
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
}
