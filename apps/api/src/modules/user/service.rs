use crate::modules::auth::model::AuthProvider;
use crate::modules::user::dto::{AvatarUploadUrlResponse, MeResponse, UpdateProfileRequest};
use crate::modules::user::errors::UserError;
use crate::modules::user::model::GeneralSettings;
use crate::modules::user::repository::UserRepository;
use crate::shared::services::redis::RedisService;
use crate::shared::services::storage::StorageService;
use crate::state::MenoState;
use uuid::Uuid;

#[derive(Clone)]
pub struct UserService {
    pub repo: UserRepository,
}
impl UserService {
    pub fn new(database: sqlx::PgPool, redis: RedisService, storage: StorageService) -> Self {
        Self {
            repo: UserRepository {
                database,
                redis,
                storage,
            },
        }
    }

    pub async fn get_me(&self, user_id: Uuid) -> Result<MeResponse, UserError> {
        if let Some(cached) = self.repo.get_cached_me(user_id).await? {
            return Ok(cached);
        }

        let user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(UserError::NotFound)?;

        let settings = self
            .repo
            .find_user_settings(user_id)
            .await?
            .unwrap_or_else(|| GeneralSettings::new(user_id));

        let providers = self.repo.find_user_providers(user_id).await?;

        let response = MeResponse {
            id: user.id,
            full_name: user.full_name,
            bio: user.bio,
            email: user.email,
            verified: user.verified,
            avatar_id: user.avatar_id,
            avatar_url: user.avatar_url,
            role: user.role,
            created_at: user.created_at,
            deleted_at: user.deleted_at,
            settings: settings.into(),
            providers,
        };

        self.repo.cache_me(response.clone()).await?;
        Ok(response)
    }

    pub async fn find_user_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, UserError> {
        self.repo.find_user_providers(user_id).await
    }

    pub async fn update_me(
        &self,
        user_id: Uuid,
        req: &UpdateProfileRequest,
    ) -> Result<MeResponse, UserError> {
        if req.full_name.is_none() && req.bio.is_none() && req.avatar_key.is_none() {
            return self.get_me(user_id).await;
        }

        let _ = self.repo.invalidate_cached_profile(user_id).await?;

        let (new_avatar_key, new_avatar_url) = if let Some(ref avatar_key) = req.avatar_key {
            self.repo.update_avatar_url(user_id, avatar_key).await?
        } else {
            (None, None)
        };

        let user = self
            .repo
            .update_user(
                user_id,
                req.full_name.as_deref(),
                req.bio.as_deref(),
                new_avatar_key.as_deref(),
                new_avatar_url.as_deref(),
            )
            .await?;

        let settings = self
            .repo
            .find_user_settings(user_id)
            .await?
            .unwrap_or_else(|| GeneralSettings::new(user_id));

        let providers = self.repo.find_user_providers(user_id).await?;

        let response = MeResponse {
            id: user.id,
            full_name: user.full_name,
            bio: user.bio,
            email: user.email,
            verified: user.verified,
            avatar_id: user.avatar_id,
            avatar_url: user.avatar_url,
            role: user.role,
            created_at: user.created_at,
            deleted_at: user.deleted_at,
            settings: settings.into(),
            providers,
        };

        Ok(response)
    }

    pub async fn get_avatar_upload_url(
        &self,
        app: &MenoState,
        user_id: Uuid,
        content_type: &str,
    ) -> Result<AvatarUploadUrlResponse, UserError> {
        let extension = match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            _ => return Err(UserError::InvalidFileType),
        };

        let file_id = Uuid::new_v4();
        let avatar_id = format!("avatars/{}/{}.{}", user_id, file_id, extension);

        let avatar_url = app
            .storage
            .presigned_upload_url(&avatar_id)
            .await
            .map_err(|e| UserError::StorageError(e.to_string()))?;

        Ok(AvatarUploadUrlResponse {
            avatar_url,
            avatar_id,
        })
    }
}
