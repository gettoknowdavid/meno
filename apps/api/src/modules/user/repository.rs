use crate::modules::auth::model::{AuthProvider, User};
use crate::modules::user::dto::{MeResponse, PublicProfileResponse};
use crate::modules::user::errors::UserError;
use crate::modules::user::model::{GeneralSettings, UserStats};
use crate::shared::services::redis::RedisService;
use crate::shared::services::storage::StorageService;
use std::str::FromStr;
use uuid::Uuid;

const ME_PROFILE_CACHE_PREFIX: &str = "ME.PROFILE";
const ME_PROFILE_TTL_SECS: i64 = 60;

const USER_PROFILE_CACHE_PREFIX: &str = "USER.PROFILE";
const USER_PROFILE_TTL_SECS: i64 = 60;

#[derive(Clone)]
pub struct UserRepository {
    pub database: sqlx::PgPool,
    pub redis: RedisService,
    pub storage: StorageService,
}
impl UserRepository {
    pub fn new(database: sqlx::PgPool, redis: RedisService, storage: StorageService) -> Self {
        Self {
            database,
            redis,
            storage,
        }
    }
    pub async fn find_by_id(&self, user_id: Uuid) -> Result<Option<User>, UserError> {
        sqlx::query_as!(
            User,
            r#"SELECT * FROM users WHERE id = $1 AND deleted_at IS NULL"#,
            user_id
        )
        .fetch_optional(&self.database)
        .await
        .map_err(UserError::Database)
    }
    pub async fn find_user_settings(
        &self,
        user_id: Uuid,
    ) -> Result<Option<GeneralSettings>, UserError> {
        sqlx::query_as!(
            GeneralSettings,
            r#"SELECT * FROM general_settings WHERE user_id = $1"#,
            user_id
        )
        .fetch_optional(&self.database)
        .await
        .map_err(UserError::Database)
    }
    pub async fn find_user_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, UserError> {
        let rows = sqlx::query!(
            "SELECT provider_type::text as provider_type FROM user_identities WHERE user_id = $1",
            user_id,
        )
        .fetch_all(&self.database)
        .await
        .map_err(UserError::Database)?;

        let providers = rows
            .iter()
            .filter_map(|r| AuthProvider::from_str(&r.provider_type).ok())
            .map(|s| AuthProvider::from(s))
            .collect();

        Ok(providers)
    }
    pub async fn find_avatar_key(&self, user_id: Uuid) -> Result<Option<String>, UserError> {
        sqlx::query_scalar!("SELECT avatar_id FROM users WHERE id = $1", user_id)
            .fetch_optional(&self.database)
            .await
            .map_err(UserError::Database)
            .map(|r| r.flatten())
    }
    pub async fn update_user(
        &self,
        user_id: Uuid,
        full_name: Option<&str>,
        bio: Option<&str>,
        avatar_key: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<User, UserError> {
        sqlx::query_as!(
            User,
            r#"UPDATE users SET
                 full_name =  COALESCE($1, full_name),
                 bio = COALESCE($2, bio),
                 avatar_id = COALESCE($3, avatar_id),
                 avatar_url = COALESCE($4, avatar_url),
                 updated_at = NOW()
                WHERE id = $5 AND deleted_at IS NULL
                RETURNING *"#,
            full_name,
            bio,
            avatar_key,
            avatar_url,
            user_id,
        )
        .fetch_one(&self.database)
        .await
        .map_err(UserError::Database)
    }
    pub async fn get_user_stats(&self, user_id: Uuid) -> Result<UserStats, UserError> {
        let row = sqlx::query!(
            r#"SELECT COUNT(CASE WHEN subscription_id = $1 THEN 1 END) as followers,
                      COUNT(CASE WHEN subscriber_id = $1 THEN 1 END) as following,
                      (SELECT COUNT(*) FROM broadcasts WHERE creator_id = $1 AND deleted_at IS NULL) as broadcasts
               FROM user_subscribers"#,
            user_id
        )
        .fetch_one(&self.database)
        .await
        .map_err(UserError::Database)?;

        Ok(UserStats {
            broadcasts: row.broadcasts.unwrap_or(0),
            followers: row.followers.unwrap_or(0),
            following: row.following.unwrap_or(0),
        })
    }
    pub async fn is_following(
        &self,
        subscription_id: Uuid,
        subscriber_id: Uuid,
    ) -> Result<bool, UserError> {
        let exists = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM user_subscribers
                WHERE subscription_id = $1 AND subscriber_id = $2
            )"#,
            subscription_id,
            subscriber_id
        )
        .fetch_one(&self.database)
        .await
        .map_err(UserError::Database)?;
        Ok(exists.unwrap_or(false))
    }

    // Redis
    fn me_cache_key(&self, user_id: Uuid) -> String {
        format!("{}:{}", ME_PROFILE_CACHE_PREFIX, user_id)
    }
    fn profile_cache_key(&self, user_id: Uuid) -> String {
        format!("{}:{}", USER_PROFILE_CACHE_PREFIX, user_id)
    }
    pub async fn cache_me(&self, value: MeResponse) -> Result<(), UserError> {
        let key = self.me_cache_key(value.id);
        self.redis
            .set(&key, &value, Some(ME_PROFILE_TTL_SECS))
            .await
            .map_err(UserError::Redis)?;
        Ok(())
    }
    pub async fn cache_profile(&self, value: PublicProfileResponse) -> Result<(), UserError> {
        let key = self.profile_cache_key(value.id);
        self.redis
            .set(&key, &value, Some(USER_PROFILE_TTL_SECS))
            .await
            .map_err(UserError::Redis)?;
        Ok(())
    }
    pub async fn get_cached_me(&self, user_id: Uuid) -> Result<Option<MeResponse>, UserError> {
        let key = self.me_cache_key(user_id);
        self.redis
            .get::<MeResponse>(&key)
            .await
            .map_err(UserError::Redis)
    }
    pub async fn get_cached_profile(
        &self,
        user_id: Uuid,
    ) -> Result<Option<PublicProfileResponse>, UserError> {
        let key = self.profile_cache_key(user_id);
        self.redis
            .get::<PublicProfileResponse>(&key)
            .await
            .map_err(UserError::Redis)
    }
    pub async fn invalidate_cached_profile(&self, user_id: Uuid) -> Result<(), UserError> {
        let key = self.profile_cache_key(user_id);
        self.redis.del(&key).await.map_err(UserError::Redis)?;
        Ok(())
    }

    // Storage
    pub async fn update_avatar_url(
        &self,
        user_id: Uuid,
        new_avatar_key: &str,
    ) -> Result<(Option<String>, Option<String>), UserError> {
        let exists = self
            .storage
            .object_exists(&new_avatar_key)
            .await
            .map_err(|e| UserError::StorageError(e.to_string()))?;

        if !exists {
            return Err(UserError::AvatarNotUploaded);
        }

        // Delete old avatar in background (fire and forget)
        if let Ok(Some(old_key)) = self.find_avatar_key(user_id).await {
            let storage = self.storage.clone();
            let old_key = old_key.clone();
            tokio::spawn(async move {
                if let Err(e) = storage.delete(&old_key).await {
                    tracing::warn!(error = %e, key = %old_key, "Failed to delete old avatar");
                }
            });
        }

        let public_url = self.storage.public_url_for(&new_avatar_key);
        Ok((Some(new_avatar_key.to_string()), Some(public_url)))
    }
}
