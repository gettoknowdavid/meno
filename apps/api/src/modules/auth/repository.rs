use crate::modules::auth::dto::RegisterRequest;
use crate::modules::auth::errors::AuthError;
use crate::modules::auth::jwt_service::hash_token;
use crate::modules::auth::model::{AccountProvider, User};
use crate::modules::auth::utils::generate_otp;
use fred::prelude::*;
use uuid::Uuid;

const RD_VERIFICATION_OTP_PREFIX: &str = "meno_auth_verify";
const RD_VERIFICATION_OTP_TTL_SECS: i64 = 900;

#[derive(Clone)]
pub struct AuthRepository {
    db: sqlx::PgPool,
    rd: Pool,
}
impl AuthRepository {
    pub fn new(db: sqlx::PgPool, rd: Pool) -> Self {
        Self { db, rd }
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
        sqlx::query!(
            r#"INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at)
               VALUES ($1, $2, $3, now() + interval '30 days')"#,
            jti,
            user_id,
            hash_token(refresh_token),
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

    // Redis
    pub async fn set_verification_otp(&self, email: &str) -> Result<String, AuthError> {
        let otp = generate_otp();
        let key = format!("{}:{}", RD_VERIFICATION_OTP_PREFIX, email);
        let ttl = Expiration::EX(RD_VERIFICATION_OTP_TTL_SECS);
        self.rd
            .set::<(), _, _>(key, otp.clone(), Some(ttl), None, false)
            .await
            .map_err(AuthError::Redis)?;
        Ok(otp)
    }
    pub async fn verify_otp(&self, email: &str, code: &str) -> Result<bool, AuthError> {
        let key = format!("{}:{}", RD_VERIFICATION_OTP_PREFIX, email);
        let stored: Option<String> = self.rd.get(key.clone()).await.map_err(AuthError::Redis)?;
        match stored {
            Some(ref s) if s == code => {
                self.rd.del::<(), _>(key).await.map_err(AuthError::Redis)?;
                Ok(true)
            }
            Some(_) => Ok(false),
            None => Err(AuthError::InvalidOtp),
        }
    }
}
