use crate::modules::auth::model::AuthProvider;
use crate::modules::profile::dto::{MeResponse, ProfileSearchResult, PublicProfileResponse};
use crate::modules::profile::errors::ProfileError;
use crate::modules::profile::model::{GeneralSettings, Profile};
use crate::shared::services::redis::RedisService;
use crate::shared::services::storage::StorageService;

use crate::shared::constants::TTL_60_SECS;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProfileRepository {
    pub database: sqlx::PgPool,
    pub redis: RedisService,
    pub storage: StorageService,
}
impl ProfileRepository {
    pub fn new(database: sqlx::PgPool, redis: RedisService, storage: StorageService) -> Self {
        Self {
            database,
            redis,
            storage,
        }
    }
    pub async fn find_by_id(&self, user_id: Uuid) -> Result<Option<Profile>, ProfileError> {
        sqlx::query_as!(
            Profile,
            r#"SELECT id, full_name, bio, email, avatar_id, avatar_url,
                      verified, followers, following, broadcasts, created_at
               FROM users WHERE id = $1 AND deleted_at IS NULL"#,
            user_id
        )
        .fetch_optional(&self.database)
        .await
        .map_err(ProfileError::Database)
    }
    pub async fn find_user_settings(
        &self,
        user_id: Uuid,
    ) -> Result<Option<GeneralSettings>, ProfileError> {
        sqlx::query_as!(
            GeneralSettings,
            r#"SELECT * FROM general_settings WHERE user_id = $1"#,
            user_id
        )
        .fetch_optional(&self.database)
        .await
        .map_err(ProfileError::Database)
    }
    pub async fn find_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, ProfileError> {
        let rows = sqlx::query!(
            "SELECT provider_type::text as provider_type FROM user_identities WHERE user_id = $1",
            user_id,
        )
        .fetch_all(&self.database)
        .await
        .map_err(ProfileError::Database)?;

        let providers = rows
            .iter()
            .filter_map(|r| AuthProvider::from_str(&r.provider_type).ok())
            .map(|s| AuthProvider::from(s))
            .collect();

        Ok(providers)
    }
    pub async fn find_avatar_key(&self, user_id: Uuid) -> Result<Option<String>, ProfileError> {
        sqlx::query_scalar!("SELECT avatar_id FROM users WHERE id = $1", user_id)
            .fetch_optional(&self.database)
            .await
            .map_err(ProfileError::Database)
            .map(|r| r.flatten())
    }
    pub async fn update_profile(
        &self,
        id: Uuid,
        full_name: Option<&str>,
        bio: Option<&str>,
        avatar_key: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<Profile, ProfileError> {
        sqlx::query_as!(
            Profile,
            r#"UPDATE users SET
                 full_name =  COALESCE($1, full_name),
                 bio = COALESCE($2, bio),
                 avatar_id = COALESCE($3, avatar_id),
                 avatar_url = COALESCE($4, avatar_url),
                 updated_at = NOW()
                WHERE id = $5 AND deleted_at IS NULL
                RETURNING id, full_name, bio, email, avatar_id, avatar_url,
                          verified, followers, following, broadcasts, created_at"#,
            full_name,
            bio,
            avatar_key,
            avatar_url,
            id,
        )
        .fetch_one(&self.database)
        .await
        .map_err(ProfileError::Database)
    }
    pub async fn is_following(
        &self,
        subscription_id: Uuid,
        subscriber_id: Uuid,
    ) -> Result<bool, ProfileError> {
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
        .map_err(ProfileError::Database)?;
        Ok(exists.unwrap_or(false))
    }
    pub async fn search_profiles(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
        current_user_id: Uuid,
    ) -> Result<Vec<ProfileSearchResult>, ProfileError> {
        let rows = sqlx::query!(
            r#"SELECT u.id, u.full_name, u.bio, u.avatar_url, u.followers, u.following, u.broadcasts,
               EXISTS(SELECT 1 FROM user_subscribers us WHERE us.subscriber_id = $1 AND us.subscription_id = u.id) AS is_following
               FROM users u
               WHERE u.deleted_at IS NULL
               AND u.search_vector @@ websearch_to_tsquery('english', $2)
               ORDER BY ts_rank(u.search_vector, websearch_to_tsquery('english', $2)) DESC, u.full_name
               LIMIT $3 OFFSET $4"#,
            current_user_id,
            query,
            limit,
            offset
        )
            .fetch_all(&self.database)
            .await
            .map_err(ProfileError::Database)?;

        let results = rows
            .iter()
            .map(|row| ProfileSearchResult {
                id: row.id,
                full_name: row.full_name.clone(),
                bio: row.bio.clone(),
                avatar_url: row.avatar_url.clone(),
                is_following: row.is_following.unwrap_or(false),
                followers: row.followers,
                following: row.following,
                broadcasts: row.broadcasts,
            })
            .collect();
        Ok(results)
    }
    pub async fn count_search_profiles(&self, query: &str) -> Result<i64, ProfileError> {
        sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!" FROM users
               WHERE deleted_at IS NULL
               AND search_vector @@ websearch_to_tsquery('english', $1)"#,
            query
        )
        .fetch_one(&self.database)
        .await
        .map_err(ProfileError::Database)
    }

    // Redis
    pub async fn cache_me(&self, value: MeResponse) -> Result<(), ProfileError> {
        let key = RedisService::profile_key(value.id);
        self.redis
            .set(&key, &value, Some(TTL_60_SECS))
            .await
            .map_err(ProfileError::Redis)
    }
    pub async fn cache_profile(&self, value: PublicProfileResponse) -> Result<(), ProfileError> {
        let key = RedisService::profile_key(value.id);
        self.redis
            .set(&key, &value, Some(TTL_60_SECS))
            .await
            .map_err(ProfileError::Redis)
    }
    pub async fn get_cached_me(&self, user_id: Uuid) -> Result<Option<MeResponse>, ProfileError> {
        let key = RedisService::profile_key(user_id);
        self.redis
            .get::<MeResponse>(&key)
            .await
            .map_err(ProfileError::Redis)
    }
    pub async fn get_cached_profile(
        &self,
        user_id: Uuid,
    ) -> Result<Option<PublicProfileResponse>, ProfileError> {
        let key = RedisService::profile_key(user_id);
        self.redis
            .get::<PublicProfileResponse>(&key)
            .await
            .map_err(ProfileError::Redis)
    }
    pub async fn invalidate_cached_profile(&self, user_id: Uuid) -> Result<(), ProfileError> {
        let key = RedisService::profile_key(user_id);
        self.redis.del(&key).await.map_err(ProfileError::Redis)
    }
    pub async fn cache_providers(
        &self,
        user_id: Uuid,
        providers: Vec<AuthProvider>,
    ) -> Result<(), ProfileError> {
        let key = RedisService::user_providers_key(user_id);
        self.redis
            .set(&key, &providers, Some(TTL_60_SECS))
            .await
            .map_err(ProfileError::Redis)
    }
    pub async fn get_cached_providers(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Vec<AuthProvider>>, ProfileError> {
        let key = RedisService::user_providers_key(user_id);
        self.redis
            .get::<Vec<AuthProvider>>(&key)
            .await
            .map_err(ProfileError::Redis)
    }
    pub fn search_results_cache_key(&self, query: &str, page: i64, limit: i64) -> String {
        let built_key_suffix = format!("{}:{}:{}", query, page, limit);
        RedisService::search_key(built_key_suffix)
    }
    pub async fn cache_search_results(
        &self,
        key: &str,
        results: Vec<ProfileSearchResult>,
    ) -> Result<(), ProfileError> {
        self.redis
            .set(&key, &results, Some(TTL_60_SECS))
            .await
            .map_err(ProfileError::Redis)?;
        Ok(())
    }
    pub async fn get_cached_search_results(
        &self,
        key: &str,
    ) -> Result<Option<Vec<ProfileSearchResult>>, ProfileError> {
        self.redis
            .get::<Vec<ProfileSearchResult>>(&key)
            .await
            .map_err(ProfileError::Redis)
    }

    // Storage
    pub async fn object_exists(&self, key: &str) -> Result<bool, ProfileError> {
        self.storage
            .object_exists(&key)
            .await
            .map_err(|e| ProfileError::StorageError(e.to_string()))
    }
    pub async fn delete_avatar(&self, user_id: Uuid) -> Result<(), ProfileError> {
        match self.find_avatar_key(user_id).await? {
            None => Ok(()),
            Some(old_key) => {
                let storage = self.storage.clone();
                let old_key = old_key.clone();
                tokio::spawn(async move {
                    if let Err(e) = storage.delete(&old_key).await {
                        tracing::warn!(error = %e, key = %old_key, "Failed to delete old avatar");
                    }
                });
                Ok(())
            }
        }
    }
    pub fn get_avatar_url(&self, key: &str) -> String {
        self.storage.public_url_for(&key)
    }
}
