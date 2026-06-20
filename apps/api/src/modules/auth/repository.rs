use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::{AuthProvider, RefreshToken, User, UserIdentity};
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuthRepository {
    db: sqlx::PgPool,
}

impl AuthRepository {
    #[must_use]
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

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

    /// Deletes expired refresh tokens from the database in batched operations.
    ///
    /// This method performs a batched deletion of refresh tokens that have passed their expiration time.
    /// To prevent long-running queries and potential table locks, it processes records in chunks of 5,000
    /// tokens per transaction. The deletion continues until all expired tokens are removed, yielding
    /// control back to the async runtime between batches to ensure fair scheduling with other concurrent
    /// operations.
    ///
    /// The method automatically handles the transaction lifecycle for each batch, committing the
    /// deletion for each chunk independently. This approach provides several advantages:
    /// - Prevents transaction bloat from accumulating too many changes
    /// - Minimizes lock contention on the refresh_tokens table
    /// - Allows the database to reclaim storage incrementally
    /// - Provides predictable performance regardless of the total number of expired tokens
    ///
    /// # Parameters
    ///
    /// - `&self`: Immutable reference to the repository instance.
    ///
    /// # Returns
    ///
    /// Returns `Result<u64, AuthError>` where the `u64` represents the total number of refresh tokens
    /// successfully deleted from the database. If no expired tokens exist, returns `Ok(0)`.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Database` if:
    /// - A database connection error occurs
    /// - The deletion query fails (e.g., syntax error, constraint violation)
    /// - Transaction management fails (e.g., transaction begin or commit fails)
    ///
    /// # Performance Notes
    ///
    /// This implementation uses a cursor-based deletion strategy with a fixed batch size of 5,000 records.
    /// The batch size strikes a balance between efficiency and resource usage:
    /// - Large enough to minimize round-trips to the database
    /// - Small enough to avoid excessive memory usage and lock contention
    ///
    /// Each batch executes within its own implicit transaction through the `DELETE` statement, ensuring
    /// that partial deletions are persisted and don't cause long-held locks. The `yield_now()` call
    /// between batches prevents blocking the async runtime during large deletion operations.
    ///
    /// For optimal performance, ensure the `expires_at` column is indexed. Consider running this
    /// operation during off-peak hours to minimize impact on read operations. The method uses the
    /// database's `NOW()` function for time comparison, ensuring consistent behavior across time zones
    /// and server clocks.
    ///
    /// # Example
    ///
    /// ```rust
    /// use auth::repository::AuthRepository;
    /// use time::OffsetDateTime;
    ///
    /// # async fn example(repo: &AuthRepository) -> Result<(), AuthError> {
    /// // Periodically clean up expired tokens, e.g., in a scheduled job
    /// let deleted_count = repo.cleanup_expired_refresh_tokens().await?;
    /// tracing::info!("Cleaned up {} expired refresh tokens", deleted_count);
    ///
    /// // If you need to monitor the operation's progress
    /// if deleted_count > 0 {
    ///     // Log the cleanup metrics
    ///     metrics::counter!("auth.refresh_tokens.cleaned", deleted_count as u64);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # See Also
    ///
    /// - [`revoke_refresh_token()`](method@Self::revoke_refresh_token) for revoking a specific token
    /// - [`revoke_all_refresh_tokens()`](method@Self::revoke_all_refresh_tokens) for revoking all tokens for a user
    /// - [`rotate_refresh_token()`](method@Self::rotate_refresh_token) for token rotation operations
    /// - The `refresh_tokens` table structure in the database schema documentation
    ///
    /// # Notes
    ///
    /// This method should be called periodically (e.g., via a cron job or scheduled task)
    /// to prevent the `refresh_tokens` table from growing indefinitely. Failure to clean
    /// up expired tokens can lead to:
    /// - Increased storage costs
    /// - Slower query performance on token lookups
    /// - Potential security risks from retaining expired tokens
    ///
    /// The method is idempotent and safe to call multiple times concurrently.
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

            let n = chunk.unwrap_or(0).cast_unsigned();
            total_deleted += n;
            if n < 5000 {
                break;
            }
            tokio::task::yield_now().await;
        }
        Ok(total_deleted)
    }
}
