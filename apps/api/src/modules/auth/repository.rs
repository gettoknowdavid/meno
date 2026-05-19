use crate::modules::auth::errors::AuthError;
use crate::modules::auth::jwt_service::hash_token;
use crate::modules::auth::model::{AuthProvider, OtpType, RefreshToken, User, UserIdentity};
use crate::modules::auth::utils::generate_otp;
use crate::shared::services::redis::RedisService;
use fred::prelude::*;
use std::collections::HashMap;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const RD_VERIFY_EMAIL_OTP_PREFIX: &str = "AUTH.OTP.VERIFY_EMAIL";
const RD_VERIFY_EMAIL_OTP_TTL_SECS: i64 = 900;

const RD_RESET_PASSWORD_OTP_PREFIX: &str = "AUTH.OTP.RESET_PASSWORD";
const RD_RESET_PASSWORD_OTP_TTL_SECS: i64 = 900;

const RD_RESEND_RATE_LIMIT_PREFIX: &str = "AUTH.OTP.RESEND_RATE_LIMIT";
const RD_RESEND_RATE_LIMIT_TTL_SECS: i64 = 60;

const RD_OAUTH_STATE_PREFIX: &str = "AUTH.OAUTH2";
const RD_OAUTH_STATE_TTL_SECS: i64 = 300;

#[derive(Clone)]
pub struct AuthRepository {
    db: sqlx::PgPool,
    redis: RedisService,
    use_redis_otp: bool,
}
impl AuthRepository {
    pub fn new(db: sqlx::PgPool, redis: RedisService, env: &str) -> Self {
        Self {
            db,
            redis,
            use_redis_otp: env != "dev" && env != "development",
        }
    }

    // DB
    pub async fn create(&self, full_name: &str, email: &str) -> Result<User, AuthError> {
        let mut tx = self.db.begin().await.map_err(AuthError::Database)?;

        let user = sqlx::query_as!(
            User,
            r#"INSERT INTO users (full_name, email) VALUES ($1, $2) RETURNING *"#,
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
    pub async fn create_user_tx(
        &self,
        full_name: &str,
        email: &str,
        pwd_hash: Option<&str>,
        provider_type: &AuthProvider,
    ) -> Result<User, AuthError> {
        let mut tx = self.db.begin().await.map_err(AuthError::Database)?;

        let user = sqlx::query_as!(
            User,
            r#"INSERT INTO users (full_name, email) VALUES ($1, $2) RETURNING *"#,
            full_name,
            email,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(AuthError::Database)?;

        sqlx::query!(
            r#"INSERT INTO user_identities (user_id, provider_type, provider_user_id, password_hash)
               VALUES ($1, $2::text, $3, $4)"#,
            user.id.clone(),
            provider_type.to_string(),
            email,
            pwd_hash,
        )
        .execute(&self.db)
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
        .fetch_one(&self.db)
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
        .fetch_optional(&self.db)
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
        .execute(&self.db)
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
        .execute(&self.db)
        .await
        .map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, AuthError> {
        sqlx::query_as!(User, "SELECT * FROM users WHERE email = $1", email)
            .fetch_optional(&self.db)
            .await
            .map_err(AuthError::Database)
    }
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AuthError> {
        sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id)
            .fetch_optional(&self.db)
            .await
            .map_err(AuthError::Database)
    }
    pub async fn user_exists(&self, email: &str) -> Result<bool, AuthError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM users WHERE email = $1) AS "exists!""#,
            email
        )
        .fetch_one(&self.db)
        .await
        .map_err(AuthError::Database)
    }
    pub async fn set_verified(&self, email: &str) -> Result<(), AuthError> {
        sqlx::query!("UPDATE users SET verified = true WHERE email = $1", email)
            .execute(&self.db)
            .await
            .map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn store_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
        refresh_token: &str,
    ) -> Result<(), AuthError> {
        let expires_at = OffsetDateTime::now_utc() + Duration::days(30);
        sqlx::query!(
            r#"INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at)
               VALUES ($1, $2, $3, $4)"#,
            jti,
            user_id,
            hash_token(refresh_token),
            expires_at
        )
        .execute(&self.db)
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
        .fetch_optional(&self.db)
        .await
        .map_err(AuthError::Database)
    }
    pub async fn rotate_refresh_token(
        &self,
        user_id: Uuid,
        old_jti: Uuid,
        new_jti: Uuid,
        new_token: &str,
    ) -> Result<(), AuthError> {
        let mut tx = self.db.begin().await.map_err(AuthError::Database)?;

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
            OffsetDateTime::now_utc() + Duration::days(30)
        )
        .execute(&mut *tx)
        .await
        .map_err(AuthError::Database)?;

        tx.commit().await.map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn revoke_refresh_token(&self, jti: Uuid) -> Result<(), AuthError> {
        sqlx::query!("DELETE FROM refresh_tokens WHERE id = $1", jti)
            .execute(&self.db)
            .await
            .map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn revoke_all_refresh_tokens(&self, user_id: Uuid) -> Result<(), AuthError> {
        sqlx::query!("DELETE FROM refresh_tokens WHERE user_id = $1", user_id)
            .execute(&self.db)
            .await
            .map_err(AuthError::Database)?;
        Ok(())
    }
    pub async fn store_otp(&self, email: &str, otp_type: OtpType) -> Result<String, AuthError> {
        let otp = generate_otp();
        if self.use_redis_otp {
            self.store_otp_redis(&email, &otp, &otp_type).await?;
        } else {
            self.store_otp_db(&email, &otp, &otp_type).await?;
        }
        Ok(otp)
    }
    pub async fn verify_otp(
        &self,
        email: &str,
        code: &str,
        otp_type: &OtpType,
    ) -> Result<bool, AuthError> {
        if self.use_redis_otp {
            let prefix = match otp_type {
                OtpType::VerifyEmail => RD_VERIFY_EMAIL_OTP_PREFIX,
                OtpType::ResetPassword => RD_RESET_PASSWORD_OTP_PREFIX,
            };
            let key = format!("{}:{}", prefix, email);
            let stored: Option<String> = self.redis.get(&key).await.map_err(AuthError::Redis)?;
            match stored {
                Some(ref s) if s == code => {
                    self.redis.del(&key).await.map_err(AuthError::Redis)?;
                    Ok(true)
                }
                Some(_) => Ok(false),
                None => Err(AuthError::InvalidOtp),
            }
        } else {
            let otp_type_str = otp_type.to_string();
            let row = sqlx::query!(
                r#"SELECT code, expires_at, used FROM otps WHERE email = $1 AND type = $2"#,
                email,
                otp_type_str,
            )
            .fetch_optional(&self.db)
            .await
            .map_err(AuthError::Database)?;
            match row {
                None => Err(AuthError::InvalidOtp),
                Some(r) => {
                    if r.used || r.expires_at < OffsetDateTime::now_utc() || r.code != code {
                        return Ok(false);
                    }
                    sqlx::query!(
                        "UPDATE otps SET used = true WHERE email = $1 AND type = $2",
                        email,
                        otp_type_str,
                    )
                    .execute(&self.db)
                    .await
                    .map_err(AuthError::Database)?;
                    Ok(true)
                }
            }
        }
    }
    pub async fn revoke_otp(&self, email: &str, otp_type: OtpType) -> Result<(), AuthError> {
        if self.use_redis_otp {
            let prefix = match otp_type {
                OtpType::VerifyEmail => RD_VERIFY_EMAIL_OTP_PREFIX,
                OtpType::ResetPassword => RD_RESET_PASSWORD_OTP_PREFIX,
            };
            let key = format!("{}:{}", prefix, email);
            self.redis.del(&key).await.map_err(AuthError::Redis)?;
        } else {
            sqlx::query!(
                r#"UPDATE otps SET used = true
                   WHERE email = $1 AND type = $2 AND expires_at <> now() AND used = false"#,
                email,
                otp_type.to_string()
            )
            .execute(&self.db)
            .await
            .map_err(AuthError::Database)?;
        }
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
            .fetch_one(&self.db)
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

    // Redis
    pub async fn can_resend_otp(&self, email: &str) -> Result<bool, AuthError> {
        let key = format!("{}:{}", RD_RESEND_RATE_LIMIT_PREFIX, email);
        let exists: Option<String> = self.redis.get(&key).await.map_err(AuthError::Redis)?;
        Ok(exists.is_none())
    }
    pub async fn set_resend_cooldown(&self, email: &str) -> Result<(), AuthError> {
        let key = format!("{}:{}", RD_RESEND_RATE_LIMIT_PREFIX, email);
        let value = "1".to_string();
        self.redis
            .set::<String>(&key, &value, Some(RD_RESEND_RATE_LIMIT_TTL_SECS))
            .await
            .map_err(AuthError::Redis)
    }
    pub async fn store_oauth_state(
        &self,
        state: &String,
        verifier: &String,
    ) -> Result<(), AuthError> {
        let key = format!("{}:{}", RD_OAUTH_STATE_PREFIX, state);

        let mut fields = HashMap::new();
        fields.insert("csrf_token", "true");
        fields.insert("pkce_code_verifier", verifier);

        let pipeline = self.redis.pipeline();

        pipeline
            .hset::<(), _, _>(&key, fields)
            .await
            .map_err(AuthError::Redis)?;

        pipeline
            .expire::<(), _>(&key, RD_OAUTH_STATE_TTL_SECS, None)
            .await
            .map_err(AuthError::Redis)?;

        pipeline.all::<()>().await.map_err(AuthError::Redis)?;

        Ok(())
    }

    pub async fn consumes_oauth_state(&self, state: &String) -> Result<String, AuthError> {
        let key = format!("{}:{}", RD_OAUTH_STATE_PREFIX, state);

        let data: HashMap<String, String> =
            self.redis.hgetall(&key).await.map_err(AuthError::Redis)?;

        let _ = self.redis.del(&key).await;

        let has_csrf = data.get("csrf_valid");
        let pkce_code_verifier = data.get("pkce_code_verifier");

        match (has_csrf, pkce_code_verifier) {
            (Some(csrf), Some(verifier)) if csrf == "true" => Ok(verifier.clone()),
            _ => Err(AuthError::InvalidToken),
        }
    }

    // Helpers
    async fn store_otp_redis(
        &self,
        email: &str,
        otp: &str,
        otp_type: &OtpType,
    ) -> Result<(), AuthError> {
        let (key, ttl) = match otp_type {
            OtpType::VerifyEmail => {
                let key = format!("{}:{}", RD_VERIFY_EMAIL_OTP_PREFIX, email);
                let ttl = Some(RD_VERIFY_EMAIL_OTP_TTL_SECS);
                (key, ttl)
            }
            OtpType::ResetPassword => {
                let key = format!("{}:{}", RD_RESET_PASSWORD_OTP_PREFIX, email);
                let ttl = Some(RD_RESET_PASSWORD_OTP_TTL_SECS);
                (key, ttl)
            }
        };
        self.redis
            .set::<String>(&key, &otp.to_string(), ttl)
            .await
            .map_err(AuthError::Redis)
    }
    async fn store_otp_db(
        &self,
        email: &str,
        otp: &str,
        otp_type: &OtpType,
    ) -> Result<(), AuthError> {
        let expires_at = OffsetDateTime::now_utc() + Duration::minutes(15);
        sqlx::query!(
            r#"INSERT INTO otps (email, code, type, expires_at)
               VALUES ($1, $2, $3::text, $4)
               ON CONFLICT (email, type) DO UPDATE
               SET code = EXCLUDED.code, expires_at = EXCLUDED.expires_at, used = false"#,
            email,
            otp,
            otp_type.to_string(),
            expires_at,
        )
        .execute(&self.db)
        .await
        .map_err(AuthError::Database)?;
        Ok(())
    }
}
