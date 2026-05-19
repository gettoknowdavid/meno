use crate::modules::user::dto::{GeneralSettingsResponse, MeResponse};
use crate::modules::user::errors::UserError;
use crate::modules::user::repository::UserRepository;
use crate::state::MenoState;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct UserService {
    pub repo: UserRepository,
}
impl UserService {
    pub fn new(db: sqlx::PgPool) -> Self {
        Self {
            repo: UserRepository { db },
        }
    }

    pub async fn get_me(&self, app: &MenoState, user_id: Uuid) -> Result<MeResponse, UserError> {
        let uws = match self.repo.find_user_with_settings(user_id).await? {
            None => return Err(UserError::NotFound),
            Some(u) => u,
        };
        Ok(MeResponse {
            id: uws.user.id,
            full_name: uws.user.full_name,
            bio: uws.user.bio,
            email: uws.user.email,
            verified: uws.user.verified,
            avatar_id: uws.user.avatar_id,
            avatar_url: uws.user.avatar_url,
            role: uws.user.role,
            created_at: uws.user.created_at,
            deleted_at: uws.user.deleted_at,
            settings: uws.settings.into(),
            providers: uws.providers,
        })
    }
}
