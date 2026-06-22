use crate::modules::auth::model::AuthProvider;
use crate::modules::profile::cache::{ProfileCache, ProfileRedisCache};
use crate::modules::profile::dto::{
    AvatarUploadUrlResponse, MeResponse, ProfileSearchQuery, ProfileSearchResult,
    PublicProfileResponse, UpdateProfileRequest,
};
use crate::modules::profile::errors::ProfileError;
use crate::modules::profile::repository::{ProfileRepo, ProfileRepository};
use crate::modules::profile::storage::ProfileStorage;
use crate::modules::settings::model::Settings;
use crate::modules::settings::repository::{SettingsRepo, SettingsRepository};
use crate::shared::pagination::{Cursor, CursorPage};
use crate::shared::services::redis::Redis;
use crate::shared::services::redis::keys::RedisKey;
use crate::shared::services::storage::StorageService;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProfileService {
    pub repo: Arc<dyn ProfileRepo>,
    pub settings_repo: Arc<dyn SettingsRepo>,
    pub cache: Arc<dyn ProfileCache>,
    pub storage: ProfileStorage,
}
impl ProfileService {
    pub fn new(db: sqlx::PgPool, redis: Redis, storage: StorageService) -> Self {
        let repo: Arc<dyn ProfileRepo> = Arc::new(ProfileRepository::new(db.clone()));
        let settings_repo: Arc<dyn SettingsRepo> = Arc::new(SettingsRepository::new(db));
        let cache: Arc<dyn ProfileCache> = Arc::new(ProfileRedisCache::new(redis));
        Self {
            repo: Arc::clone(&repo),
            settings_repo: Arc::clone(&settings_repo),
            cache: Arc::clone(&cache),
            storage: ProfileStorage::new(storage),
        }
    }

    pub async fn get_me(&self, user_id: Uuid) -> Result<MeResponse, ProfileError> {
        if let Some(cached) = self.cache.get_cached_me(user_id).await? {
            return Ok(cached);
        }

        let profile = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(ProfileError::NotFound)?;

        let settings = self
            .settings_repo
            .find_by_user_id(user_id)
            .await
            .map_err(|e| ProfileError::Internal(e.into()))?
            .unwrap_or_else(|| Settings::new(user_id));

        let providers = self.repo.find_providers(user_id).await?;

        let response = MeResponse {
            id: profile.id,
            full_name: profile.full_name,
            bio: profile.bio,
            email: profile.email,
            verified: profile.verified,
            avatar_id: profile.avatar_id,
            avatar_url: profile.avatar_url,
            created_at: profile.created_at,
            settings: settings.into(),
            providers,
        };

        self.cache.cache_me(response.clone()).await?;
        Ok(response)
    }

    pub async fn find_user_providers(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<AuthProvider>, ProfileError> {
        if let Some(cached_providers) = self.cache.get_cached_providers(user_id).await? {
            return Ok(cached_providers);
        }

        let providers = self.repo.find_providers(user_id).await?;

        self.cache
            .cache_providers(user_id, providers.clone())
            .await?;

        Ok(providers)
    }

    pub async fn update_me(
        &self,
        user_id: Uuid,
        req: &UpdateProfileRequest,
    ) -> Result<MeResponse, ProfileError> {
        if req.full_name.is_none() && req.bio.is_none() && req.avatar_key.is_none() {
            return self.get_me(user_id).await;
        }

        self.cache.invalidate_cached_profile(user_id).await?;

        let (new_avatar_key, new_avatar_url) = if let Some(ref avatar_key) = req.avatar_key {
            self.update_avatar_url(user_id, avatar_key).await?
        } else {
            (None, None)
        };

        let _ = self
            .repo
            .update_profile(
                user_id,
                req.full_name.as_deref(),
                req.bio.as_deref(),
                new_avatar_key.as_deref(),
                new_avatar_url.as_deref(),
            )
            .await?;

        self.get_me(user_id).await
    }

    pub async fn get_avatar_upload_url(
        &self,
        user_id: Uuid,
        content_type: &str,
    ) -> Result<AvatarUploadUrlResponse, ProfileError> {
        let extension = match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            _ => return Err(ProfileError::InvalidFileType),
        };

        let file_id = Uuid::new_v4();
        let avatar_id = format!("avatars/{}/{}.{}", user_id, file_id, extension);
        let avatar_url = self.storage.get_avatar_upload_url(&avatar_id).await?;

        Ok(AvatarUploadUrlResponse {
            avatar_url,
            avatar_id,
        })
    }

    pub async fn get_user_by_id(
        &self,
        auth_user_id: Uuid,
        user_id: Uuid,
    ) -> Result<PublicProfileResponse, ProfileError> {
        if let Some(cached) = self.cache.get_cached_profile(user_id).await? {
            return Ok(cached);
        };

        let user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(ProfileError::NotFound)?;

        let is_following = self.repo.is_following(user_id, auth_user_id).await?;

        let response = PublicProfileResponse {
            id: user.id,
            full_name: user.full_name,
            bio: user.bio,
            avatar_url: user.avatar_url,
            is_following,
            broadcasts: user.broadcasts,
            following: user.following,
            followers: user.followers,
            created_at: user.created_at,
        };

        self.cache.cache_profile(response.clone()).await?;

        Ok(response)
    }

    pub async fn search_profiles(
        &self,
        query: &ProfileSearchQuery,
        current_user_id: Uuid,
    ) -> Result<CursorPage<ProfileSearchResult>, ProfileError> {
        let q = &query.q.trim().to_lowercase();

        let limit = query.limit();
        let cache_key = RedisKey::search_results(q, 0, limit);

        if let Some(cached_results) = self.cache.get_cached_search_results(&cache_key).await? {
            return self.apply_cursor(cached_results, limit);
        }

        let results = self.repo.search_profiles(query, current_user_id).await?;

        if query.cursor().is_none() && results.len() <= 100 {
            self.cache
                .cache_search_results(&cache_key, results.clone())
                .await?;
        }

        self.apply_cursor(results, limit)
    }

    async fn update_avatar_url(
        &self,
        user_id: Uuid,
        new_avatar_key: &str,
    ) -> Result<(Option<String>, Option<String>), ProfileError> {
        if !self.storage.object_exists(new_avatar_key).await? {
            return Err(ProfileError::AvatarNotUploaded);
        }

        self.delete_avatar(user_id).await?;

        let public_url = self.storage.get_avatar_url(new_avatar_key);
        Ok((Some(new_avatar_key.to_string()), Some(public_url)))
    }

    async fn delete_avatar(&self, user_id: Uuid) -> Result<(), ProfileError> {
        match self.repo.find_avatar_key(user_id).await? {
            None => Ok(()),
            Some(old_key) => {
                let storage = self.storage.clone();
                let old_key = old_key.clone();
                tokio::spawn(async move {
                    if let Err(e) = storage.delete_avatar(&old_key).await {
                        tracing::warn!(error = %e, key = %old_key, "Failed to delete old avatar");
                    }
                });
                Ok(())
            }
        }
    }

    fn apply_cursor(
        &self,
        results: Vec<ProfileSearchResult>,
        limit: i64,
    ) -> Result<CursorPage<ProfileSearchResult>, ProfileError> {
        Ok(CursorPage::from_rows(results, limit, |p| {
            Cursor::from_timestamp_id(p.created_at, p.id)
        }))
    }
}
