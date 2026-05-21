use crate::modules::auth::model::AuthProvider;
use crate::modules::profile::dto::{
    AvatarUploadUrlResponse, MeResponse, ProfileSearchParam, ProfileSearchResult,
    PublicProfileResponse, UpdateProfileRequest,
};
use crate::modules::profile::errors::ProfileError;
use crate::modules::profile::model::GeneralSettings;
use crate::modules::profile::repository::ProfileRepository;
use crate::shared::pagination::{PaginationParams, PaginationResponse};
use crate::shared::services::redis::RedisService;
use crate::shared::services::storage::StorageService;
use crate::state::MenoState;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProfileService {
    pub repo: ProfileRepository,
}
impl ProfileService {
    pub fn new(database: sqlx::PgPool, redis: RedisService, storage: StorageService) -> Self {
        Self {
            repo: ProfileRepository {
                database,
                redis,
                storage,
            },
        }
    }

    pub async fn get_me(&self, user_id: Uuid) -> Result<MeResponse, ProfileError> {
        if let Some(cached) = self.repo.get_cached_me(user_id).await? {
            return Ok(cached);
        }

        let profile = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(ProfileError::NotFound)?;

        let settings = self
            .repo
            .find_user_settings(user_id)
            .await?
            .unwrap_or_else(|| GeneralSettings::new(user_id));

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

        self.repo.cache_me(response.clone()).await?;
        Ok(response)
    }

    pub async fn find_user_providers(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<AuthProvider>, ProfileError> {
        self.repo.find_providers(user_id).await
    }

    pub async fn update_me(
        &self,
        user_id: Uuid,
        req: &UpdateProfileRequest,
    ) -> Result<MeResponse, ProfileError> {
        if req.full_name.is_none() && req.bio.is_none() && req.avatar_key.is_none() {
            return self.get_me(user_id).await;
        }

        let _ = self.repo.invalidate_cached_profile(user_id).await?;

        let (new_avatar_key, new_avatar_url) = if let Some(ref avatar_key) = req.avatar_key {
            self.repo.update_avatar_url(user_id, avatar_key).await?
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
        app: &MenoState,
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

        let avatar_url = app
            .storage
            .presigned_upload_url(&avatar_id)
            .await
            .map_err(|e| ProfileError::StorageError(e.to_string()))?;

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
        if let Some(cached) = self.repo.get_cached_profile(user_id).await? {
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
            following: user.followers,
            followers: user.followers,
            created_at: user.created_at,
        };

        let _ = self.repo.cache_profile(response.clone()).await?;

        Ok(response)
    }

    pub async fn search_profiles(
        &self,
        current_user_id: Uuid,
        params: &ProfileSearchParam,
    ) -> Result<PaginationResponse<ProfileSearchResult>, ProfileError> {
        let q = &params.q.trim().to_lowercase();
        let page = params.page.unwrap_or(1);
        let limit = params.limit.unwrap_or(50);

        let pagination = PaginationParams::new(page, limit);

        let results = self
            .repo
            .search_profiles(&q, limit, pagination.offset(), current_user_id)
            .await?;

        let total = self.repo.count_search_profiles(&q).await?;

        let total_pages = if total == 0 {
            0
        } else {
            (total + limit - 1) / limit
        };

        Ok(PaginationResponse {
            total_pages,
            current_page: page,
            total_items: total,
            data: results,
        })
    }
}
