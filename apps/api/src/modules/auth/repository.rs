use crate::modules::auth::dto::RegisterRequest;
use crate::modules::auth::errors::AuthError;
use crate::modules::auth::jwt_service::hash_token;
use crate::modules::auth::model::{AccountProvider, OtpType, RefreshToken, User};
use crate::modules::auth::utils::generate_otp;
use fred::prelude::*;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const RD_VERIFY_EMAIL_OTP_PREFIX: &str = "OTP.VERIFY_EMAIL";
const RD_VERIFY_EMAIL_OTP_TTL_SECS: i64 = 900;

const RD_RESET_PASSWORD_OTP_PREFIX: &str = "OTP.RESET_PASSWORD";
const RD_RESET_PASSWORD_OTP_TTL_SECS: i64 = 900;

const RD_RESEND_RATE_LIMIT_PREFIX: &str = "OTP.RESEND_RATE_LIMIT";
const RD_RESEND_RATE_LIMIT_TTL_SECS: i64 = 60;

#[derive(Clone)]
pub struct AuthRepository {
    db: sqlx::PgPool,
    rd: Pool,
    use_redis_otp: bool,
}
impl AuthRepository {
    pub fn new(db: sqlx::PgPool, rd: Pool, env: &str) -> Self {
        Self {
            db,
            rd,
            use_redis_otp: env != "dev" && env != "development",
        }
    }

    // DB
    pub async fn create(&self, req: &RegisterRequest, hash_pwd: String) -> Result<User, AuthError> {
        let mut tx = self.db.begin().await.map_err(AuthError::Database)?;
        let user = sqlx::query_as!(
            User,
            r#"INSERT INTO users (full_name, email, password, account_provider)
               VALUES ($1, $2, $3, $4)
               RETURNING *"#,
            req.full_name,
            req.email,
            hash_pwd,
            AccountProvider::Email as _,
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
    pub async fn update_password(&self, email: &str, hash_pwd: String) -> Result<(), AuthError> {
        sqlx::query!(
            r#"UPDATE users SET password = $1 WHERE email = $2"#,
            hash_pwd,
            email
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
            let stored: Option<String> =
                self.rd.get(key.clone()).await.map_err(AuthError::Redis)?;
            match stored {
                Some(ref s) if s == code => {
                    self.rd.del::<(), _>(key).await.map_err(AuthError::Redis)?;
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
            self.rd.del::<(), _>(key).await.map_err(AuthError::Redis)?;
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
        let exists: Option<String> = self.rd.get(&key).await.map_err(AuthError::Redis)?;
        Ok(exists.is_none())
    }
    pub async fn set_resend_cooldown(&self, email: &str) -> Result<(), AuthError> {
        let key = format!("{}:{}", RD_RESEND_RATE_LIMIT_PREFIX, email);
        let ttl = Expiration::EX(RD_RESEND_RATE_LIMIT_TTL_SECS);
        self.rd
            .set::<(), _, _>(key, "1", Some(ttl), None, false)
            .await
            .map_err(AuthError::Redis)
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
                let ttl = Expiration::EX(RD_VERIFY_EMAIL_OTP_TTL_SECS);
                (key, ttl)
            }
            OtpType::ResetPassword => {
                let key = format!("{}:{}", RD_RESET_PASSWORD_OTP_PREFIX, email);
                let ttl = Expiration::EX(RD_RESET_PASSWORD_OTP_TTL_SECS);
                (key, ttl)
            }
        };
        self.rd
            .set::<(), _, _>(key, otp, Some(ttl), None, false)
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
