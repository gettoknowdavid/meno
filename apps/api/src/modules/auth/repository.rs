use crate::modules::auth::dto::RegisterRequest;
use crate::modules::auth::errors::AuthError;
use crate::modules::auth::jwt_service::hash_token;
use crate::modules::auth::model::{AccountProvider, User};
use crate::modules::auth::utils::generate_otp;
use fred::prelude::*;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const RD_VERIFICATION_OTP_PREFIX: &str = "meno_auth_verify";
const RD_VERIFICATION_OTP_TTL_SECS: i64 = 900;

const RD_RESEND_RATE_LIMIT_PREFIX: &str = "meno_auth_resend";
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
    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, AuthError> {
        sqlx::query_as!(User, "SELECT * FROM users WHERE email = $1", email)
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
        expires_at: OffsetDateTime,
    ) -> Result<(), AuthError> {
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
    pub async fn set_verification_otp(&self, email: &str) -> Result<String, AuthError> {
        let otp = generate_otp();
        if self.use_redis_otp {
            let key = format!("{}:{}", RD_VERIFICATION_OTP_PREFIX, email);
            let ttl = Expiration::EX(RD_VERIFICATION_OTP_TTL_SECS);
            self.rd
                .set::<(), _, _>(key, otp.clone(), Some(ttl), None, false)
                .await
                .map_err(AuthError::Redis)?;
        } else {
            let expires_at = OffsetDateTime::now_utc() + Duration::minutes(15);
            sqlx::query!(
                r#"INSERT INTO otps (email, code, type, expires_at)
                    VALUES ($1, $2, 'verify_email', $3)
                    ON CONFLICT (email) DO UPDATE
                    SET code = EXCLUDED.code, expires_at = EXCLUDED.expires_at, used = false"#,
                email,
                otp,
                expires_at,
            )
            .execute(&self.db)
            .await
            .map_err(AuthError::Database)?;
        }
        Ok(otp)
    }
    pub async fn verify_otp(&self, email: &str, code: &str) -> Result<bool, AuthError> {
        if self.use_redis_otp {
            let key = format!("{}:{}", RD_VERIFICATION_OTP_PREFIX, email);
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
            let row = sqlx::query!(
                r#"SELECT code, expires_at, used FROM otps WHERE email = $1 AND type = 'verify_email'"#,
                email
            )
            .fetch_optional(&self.db)
            .await
            .map_err(AuthError::Database)?;

            match row {
                None => Err(AuthError::InvalidOtp),
                Some(r) if r.used => Err(AuthError::OtpAlreadyUsed),
                Some(r) if r.expires_at < OffsetDateTime::now_utc() => Err(AuthError::InvalidOtp),
                Some(r) if r.code != code => Ok(false),
                Some(_) => {
                    sqlx::query!("UPDATE otps SET used = true WHERE email = $1", email)
                        .execute(&self.db)
                        .await
                        .map_err(AuthError::Database)?;
                    Ok(true)
                }
            }
        }
    }
    pub async fn revoke_otp(&self, email: &str) -> Result<(), AuthError> {
        if self.use_redis_otp {
            let key = format!("{}:{}", RD_VERIFICATION_OTP_PREFIX, email);
            self.rd.del::<(), _>(key).await.map_err(AuthError::Redis)?;
        } else {
            sqlx::query!("UPDATE otps SET used = true WHERE email = $1 AND expires_at <> now() AND used = false", email)
                .execute(&self.db)
                .await
                .map_err(AuthError::Database)?;
        }
        Ok(())
    }

    // Redis
    pub async fn can_resend_verification_otp(&self, email: &str) -> Result<bool, AuthError> {
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
}
