use crate::modules::auth::model::{AuthProvider, User};
use crate::modules::user::dto::{MeResponse, PublicProfileResponse};
use crate::modules::user::errors::UserError;
use crate::modules::user::model::GeneralSettings;
use crate::shared::services::redis::RedisService;
use std::str::FromStr;
use uuid::Uuid;

const USER_PROFILE_CACHE_PREFIX: &str = "USER.PROFILE";
const USER_PROFILE_TTL_SECS: i64 = 60;

#[derive(Clone)]
pub struct UserRepository {
    pub database: sqlx::PgPool,
    pub redis: RedisService,
}
impl UserRepository {
    pub fn new(database: sqlx::PgPool, redis: RedisService) -> Self {
        Self { database, redis }
    }
    pub async fn find_user(&self, user_id: Uuid) -> Result<Option<User>, UserError> {
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

    // Redis
    fn profile_cache_key(&self, user_id: Uuid) -> String {
        format!("{}:{}", USER_PROFILE_CACHE_PREFIX, user_id)
    }
    pub async fn cache_me(&self, value: MeResponse) -> Result<(), UserError> {
        let key = self.profile_cache_key(value.id);
        self.redis
            .set(&key, &value, Some(USER_PROFILE_TTL_SECS))
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
        let key = self.profile_cache_key(user_id);
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
}
