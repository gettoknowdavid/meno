use crate::modules::auth::model::{AuthProvider, UserRole};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;
use validator::Validate;

// Requests DTOS
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProfileRequest {
    #[validate(length(min = 3, max = 100))]
    pub full_name: Option<String>,

    #[validate(length(max = 244, message = "Bio length exceeded (Max. 244)"))]
    pub bio: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UserSearchParam {
    #[validate(length(min = 3))]
    pub query: String,

    #[validate(range(min = 1))]
    pub page: Option<i64>,

    #[validate(range(min = 20, max = 50))]
    pub limit: Option<i64>,
}

// Response DTOS
#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub email: String,
    pub verified: bool,
    pub avatar_id: Option<String>,
    pub avatar_url: Option<String>,
    pub providers: Vec<AuthProvider>,
    pub role: UserRole,
    pub settings: GeneralSettingsResponse,
    pub created_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize)]
pub struct PublicProfileResponse {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub verified: bool,
    pub follower_count: i64,
    pub following_count: i64,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
pub struct UserSearchResult {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub verified: bool,
    pub is_subscribed: bool,
}

#[derive(Debug, Serialize)]
pub struct AvatarUploadResponse {
    pub avatar_id: String,
    pub avatar_url: String,
}

#[derive(Debug, Serialize)]
pub struct GeneralSettingsResponse {
    pub push_notifications: bool,
    pub app_notifications: bool,
    pub email_notifications: bool,
    pub display: String,
    pub language: String,
    pub notification_settings: serde_json::Value,
}
