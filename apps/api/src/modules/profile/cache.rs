use crate::modules::auth::model::AuthProvider;
use crate::modules::profile::dto::{MeResponse, ProfileSearchResult, PublicProfileResponse};
use crate::modules::profile::errors::ProfileError;
use crate::shared::services::redis::Redis;

use crate::shared::constants::TTL_60_SECS;
use crate::shared::services::redis::keys::RedisKey;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProfileCache {
    redis: Redis,
}
impl ProfileCache {
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }

    pub async fn cache_me(&self, value: MeResponse) -> Result<(), ProfileError> {
        let key = RedisKey::profile(value.id);
        self.redis
            .set(&key, &value, Some(TTL_60_SECS))
            .await
            .map_err(ProfileError::Redis)
    }
    pub async fn cache_profile(&self, value: PublicProfileResponse) -> Result<(), ProfileError> {
        let key = RedisKey::profile(value.id);
        self.redis
            .set(&key, &value, Some(TTL_60_SECS))
            .await
            .map_err(ProfileError::Redis)
    }
    pub async fn get_cached_me(&self, user_id: Uuid) -> Result<Option<MeResponse>, ProfileError> {
        let key = RedisKey::profile(user_id);
        self.redis
            .get::<MeResponse>(&key)
            .await
            .map_err(ProfileError::Redis)
    }
    pub async fn get_cached_profile(
        &self,
        user_id: Uuid,
    ) -> Result<Option<PublicProfileResponse>, ProfileError> {
        let key = RedisKey::profile(user_id);
        self.redis
            .get::<PublicProfileResponse>(&key)
            .await
            .map_err(ProfileError::Redis)
    }
    pub async fn invalidate_cached_profile(&self, user_id: Uuid) -> Result<(), ProfileError> {
        let key = RedisKey::profile(user_id);
        let _ = self.redis.del(&key).await.map_err(ProfileError::Redis)?;
        Ok(())
    }
    pub async fn cache_providers(
        &self,
        user_id: Uuid,
        providers: Vec<AuthProvider>,
    ) -> Result<(), ProfileError> {
        let key = RedisKey::user_providers(user_id);
        self.redis
            .set(&key, &providers, Some(TTL_60_SECS))
            .await
            .map_err(ProfileError::Redis)
    }
    pub async fn get_cached_providers(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Vec<AuthProvider>>, ProfileError> {
        let key = RedisKey::user_providers(user_id);
        self.redis
            .get::<Vec<AuthProvider>>(&key)
            .await
            .map_err(ProfileError::Redis)
    }

    pub async fn cache_search_results(
        &self,
        key: &RedisKey,
        results: Vec<ProfileSearchResult>,
    ) -> Result<(), ProfileError> {
        self.redis
            .set(key, &results, Some(TTL_60_SECS))
            .await
            .map_err(ProfileError::Redis)?;
        Ok(())
    }
    pub async fn get_cached_search_results(
        &self,
        key: &RedisKey,
    ) -> Result<Option<Vec<ProfileSearchResult>>, ProfileError> {
        self.redis
            .get::<Vec<ProfileSearchResult>>(key)
            .await
            .map_err(ProfileError::Redis)
    }
}
