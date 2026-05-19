use crate::modules::auth::model::AuthProvider;
use crate::modules::user::dto::{ MeResponse};
use crate::modules::user::errors::UserError;
use crate::modules::user::model::GeneralSettings;
use crate::modules::user::repository::UserRepository;
use crate::shared::services::redis::RedisService;
use uuid::Uuid;

#[derive(Clone)]
pub struct UserService {
    pub repo: UserRepository,
}
impl UserService {
    pub fn new(database: sqlx::PgPool, redis: RedisService) -> Self {
        Self {
            repo: UserRepository { database, redis },
        }
    }

    pub async fn get_me(&self, user_id: Uuid) -> Result<MeResponse, UserError> {
        if let Some(cached) = self.repo.get_cached_me(user_id).await? {
            return Ok(cached);
        }

        let user = self
            .repo
            .find_user(user_id)
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
}
