use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::{AuthProvider, RefreshToken, User, UserIdentity};
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

/// The trait exists solely to enable test doubles.
/// In production, only `AuthRepository` implements it.
#[async_trait::async_trait]
pub trait AuthRepo: Send + Sync + 'static {
    async fn create_user(&self, full_name: &str, email: &str) -> Result<User, AuthError>;

    async fn create_identity(
        &self,
        user_id: Uuid,
        provider: &AuthProvider,
        provider_user_id: &str,
        password_hash: Option<&str>,
    ) -> Result<UserIdentity, AuthError>;

    async fn find_identity(
        &self,
        provider: &AuthProvider,
        provider_user_id: &str,
    ) -> Result<Option<UserIdentity>, AuthError>;

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, AuthError>;

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AuthError>;

    async fn find_user_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, AuthError>;

    async fn set_verified(&self, email: &str) -> Result<(), AuthError>;

    async fn update_password(&self, user_id: Uuid, hash: String) -> Result<(), AuthError>;

    async fn link_provider(
        &self,
        user_id: Uuid,
        provider: &AuthProvider,
        provider_user_id: &str,
    ) -> Result<(), AuthError>;

    async fn store_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
        token_hash: &str,
        expires_at: OffsetDateTime,
    ) -> Result<(), AuthError>;

    async fn find_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
    ) -> Result<Option<RefreshToken>, AuthError>;

    async fn rotate_refresh_token(
        &self,
        user_id: Uuid,
        old_jti: Uuid,
        new_jti: Uuid,
        new_hash: &str,
        expires_at: OffsetDateTime,
    ) -> Result<(), AuthError>;

    async fn revoke_refresh_token(&self, jti: Uuid) -> Result<(), AuthError>;

    async fn revoke_all_refresh_tokens(&self, user_id: Uuid) -> Result<(), AuthError>;

    async fn cleanup_expired_refresh_tokens(&self) -> Result<u64, AuthError>;
}

#[derive(Debug, Clone)]
pub struct AuthRepository {
    db: sqlx::PgPool,
}

impl AuthRepository {
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl AuthRepo for AuthRepository {
    async fn create_user(&self, full_name: &str, email: &str) -> Result<User, AuthError> {
        let mut tx = self.db.begin().await.map_err(AuthError::Database)?;

        let user = sqlx::query_as!(
            User,
            r#"INSERT INTO users (full_name, email)
               VALUES ($1, $2)
               RETURNING
                   id,
                   full_name,
                   bio,
                   email,
                   avatar_id,
                   avatar_url,
                   verified,
                   role,
                   created_at,
                   updated_at,
                   deleted_at"#,
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

    async fn create_identity(
        &self,
        user_id: Uuid,
        provider: &AuthProvider,
        provider_user_id: &str,
        password_hash: Option<&str>,
    ) -> Result<UserIdentity, AuthError> {
        sqlx::query_as!(
            UserIdentity,
            r#"INSERT INTO user_identities (user_id, provider_type, provider_user_id, password_hash)
               VALUES ($1, $2::text, $3, $4)
               RETURNING *"#,
            user_id,
            provider.to_string(),
            provider_user_id,
            password_hash,
        )
        .fetch_one(&self.db)
        .await
        .map_err(AuthError::Database)
    }

    async fn find_identity(
        &self,
        provider: &AuthProvider,
        provider_user_id: &str,
    ) -> Result<Option<UserIdentity>, AuthError> {
        sqlx::query_as!(
            UserIdentity,
            r#"SELECT * FROM user_identities
               WHERE provider_type = $1::text AND provider_user_id = $2"#,
            provider.to_string(),
            provider_user_id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(AuthError::Database)
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, AuthError> {
        sqlx::query_as!(
            User,
            r#"SELECT
                    id,
                    full_name,
                    bio,
                    email,
                    avatar_id,
                    avatar_url,
                    verified,
                    role,
                    created_at,
                    updated_at,
                    deleted_at
               FROM users WHERE email = $1"#,
            email,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(AuthError::Database)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AuthError> {
        sqlx::query_as!(
            User,
            r#"SELECT
                    id,
                    full_name,
                    bio,
                    email,
                    avatar_id,
                    avatar_url,
                    verified,
                    role,
                    created_at,
                    updated_at,
                    deleted_at
               FROM users WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(AuthError::Database)
    }

    async fn find_user_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, AuthError> {
        let rows = sqlx::query!(
            "SELECT provider_type::text as provider_type FROM user_identities WHERE user_id = $1",
            user_id,
        )
        .fetch_all(&self.db)
        .await
        .map_err(AuthError::Database)?;

        Ok(rows
            .iter()
            .filter_map(|r| AuthProvider::from_str(&r.provider_type).ok())
            .map(|s| AuthProvider::from(s))
            .collect())
    }

    async fn set_verified(&self, email: &str) -> Result<(), AuthError> {
        sqlx::query!("UPDATE users SET verified = true WHERE email = $1", email)
            .execute(&self.db)
            .await
            .map_err(AuthError::Database)?;
        Ok(())
    }

    async fn update_password(&self, user_id: Uuid, hash: String) -> Result<(), AuthError> {
        sqlx::query!(
            r#"UPDATE user_identities SET password_hash = $1 WHERE user_id = $2"#,
            hash,
            user_id
        )
        .execute(&self.db)
        .await
        .map_err(AuthError::Database)?;
        Ok(())
    }

    async fn link_provider(
        &self,
        user_id: Uuid,
        provider: &AuthProvider,
        provider_user_id: &str,
    ) -> Result<(), AuthError> {
        sqlx::query!(
            r#"INSERT INTO user_identities (user_id, provider_type, provider_user_id)
               VALUES ($1, $2::text, $3)
               ON CONFLICT (user_id, provider_type) DO NOTHING"#,
            user_id,
            provider.to_string(),
            provider_user_id,
        )
        .execute(&self.db)
        .await
        .map_err(AuthError::Database)?;
        Ok(())
    }

    async fn store_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
        token_hash: &str,
        expires_at: OffsetDateTime,
    ) -> Result<(), AuthError> {
        sqlx::query!(
            r#"INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at)
               VALUES ($1, $2, $3, $4)"#,
            jti,
            user_id,
            token_hash,
            expires_at,
        )
        .execute(&self.db)
        .await
        .map_err(AuthError::Database)?;
        Ok(())
    }

    async fn find_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
    ) -> Result<Option<RefreshToken>, AuthError> {
        sqlx::query_as!(
            RefreshToken,
            r#"SELECT * FROM refresh_tokens WHERE id = $1 AND user_id = $2"#,
            jti,
            user_id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(AuthError::Database)
    }

    async fn rotate_refresh_token(
        &self,
        user_id: Uuid,
        old_jti: Uuid,
        new_jti: Uuid,
        new_hash: &str,
        expires_at: OffsetDateTime,
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
            new_hash,
            expires_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(AuthError::Database)?;

        tx.commit().await.map_err(AuthError::Database)?;
        Ok(())
    }

    async fn revoke_refresh_token(&self, jti: Uuid) -> Result<(), AuthError> {
        sqlx::query!("DELETE FROM refresh_tokens WHERE id = $1", jti)
            .execute(&self.db)
            .await
            .map_err(AuthError::Database)?;
        Ok(())
    }

    async fn revoke_all_refresh_tokens(&self, user_id: Uuid) -> Result<(), AuthError> {
        sqlx::query!("DELETE FROM refresh_tokens WHERE user_id = $1", user_id)
            .execute(&self.db)
            .await
            .map_err(AuthError::Database)?;
        Ok(())
    }

    async fn cleanup_expired_refresh_tokens(&self) -> Result<u64, AuthError> {
        let mut total_deleted: u64 = 0;
        loop {
            let chunk: Option<i64> = sqlx::query_scalar!(
                r#"WITH deleted AS (
                    DELETE FROM refresh_tokens
                    WHERE id IN (
                        SELECT id FROM refresh_tokens
                        WHERE expires_at < NOW()
                        LIMIT 5000
                    )
                    RETURNING 1
                )
                SELECT COUNT(*) FROM deleted"#
            )
            .fetch_one(&self.db)
            .await
            .map_err(AuthError::Database)?;

            let n = chunk.unwrap_or(0) as u64;
            total_deleted += n;
            if n < 5000 {
                break;
            }
            tokio::task::yield_now().await;
        }
        Ok(total_deleted)
    }
}
