use crate::modules::auth::model::{AuthProvider, UserRole};
use crate::modules::user::model::{Display, UserStats};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, serde::rfc3339};
use uuid::Uuid;
use validator::Validate;

// Requests DTOS
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProfileRequest {
    #[validate(length(min = 3, max = 100, message = "Name must be 3-100 characters"))]
    pub full_name: Option<String>,

    #[validate(length(max = 500, message = "Bio must be under 500 characters"))]
    pub bio: Option<String>,

    pub avatar_key: Option<String>,
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

#[derive(Debug, serde::Deserialize)]
pub struct AvatarUploadUrlParams {
    pub content_type: String,
}

// Response DTOS
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub email: String,
    pub verified: bool,
    pub avatar_id: Option<String>,
    pub avatar_url: Option<String>,
    #[serde(with = "rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "rfc3339::option")]
    pub deleted_at: Option<OffsetDateTime>,
    pub providers: Vec<AuthProvider>,
    pub role: UserRole,
    pub settings: GeneralSettingsResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicProfileResponse {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub verified: bool,
    pub is_following: bool,
    #[serde(with = "rfc3339")]
    pub created_at: OffsetDateTime,
    pub stats: UserStats,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserSearchResult {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub verified: bool,
    pub is_subscribed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AvatarUploadUrlResponse {
    pub avatar_id: String,
    pub avatar_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeneralSettingsResponse {
    pub push_notifications: bool,
    pub app_notifications: bool,
    pub email_notifications: bool,
    pub display: Display,
    pub language: String,
    pub notification_preferences: serde_json::Value,
}
