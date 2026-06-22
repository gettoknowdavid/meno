use crate::modules::auth::model::AuthProvider;
use crate::shared::pagination::CursorParams;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
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
pub struct ProfileSearchQuery {
    #[validate(length(min = 3, message = "Keywords must be at least 3 characters"))]
    pub q: String,

    #[serde(flatten)]
    pub pagination: CursorParams,
}
impl ProfileSearchQuery {
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }
    pub fn cursor(&self) -> Option<&crate::shared::pagination::Cursor> {
        self.pagination.cursor.as_ref()
    }
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
    pub providers: Vec<AuthProvider>,
    pub settings: crate::modules::settings::dto::SettingsResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicProfileResponse {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub is_following: bool,
    pub followers: i64,
    pub following: i64,
    pub broadcasts: i64,
    #[serde(with = "rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct ProfileSearchResult {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub is_following: bool,
    pub followers: i64,
    pub following: i64,
    pub broadcasts: i64,
    #[serde(with = "rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AvatarUploadUrlResponse {
    pub avatar_id: String,
    pub avatar_url: String,
}
