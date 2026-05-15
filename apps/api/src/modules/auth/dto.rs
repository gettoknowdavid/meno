use crate::modules::auth::model::{AccountProvider, User};
use crate::modules::auth::validators::{validate_email, validate_password};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;
use validator::Validate;

// Request DTOS
#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(
        min = 3,
        max = 100,
        message = "Full name must be between 3 and 100 characters"
    ))]
    pub full_name: String,

    #[validate(custom(function = "validate_email"))]
    pub email: String,

    #[validate(custom(function = "validate_password"))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(custom(function = "validate_email"))]
    pub email: String,

    #[validate(length(min = 1, message = "Password is required"))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RefreshTokenRequest {
    #[validate(length(min = 1, message = "Refresh token is required"))]
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct GoogleAuthRequest {
    #[validate(length(min = 1, message = "ID token is required"))]
    pub id_token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ForgotPasswordRequest {
    #[validate(custom(function = "validate_email"))]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordRequest {
    #[validate(custom(function = "validate_email"))]
    pub email: String,

    #[validate(length(min = 1, message = "Code is required"))]
    pub code: String,

    #[validate(custom(function = "validate_password"))]
    pub new_password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct VerifyEmailRequest {
    #[validate(custom(function = "validate_email"))]
    pub email: String,

    #[validate(length(min = 1, message = "Code is required"))]
    pub code: String,
}

// Response DTOS
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub email: String,
    pub account_provider: AccountProvider,
    pub verified: bool,
    pub avatar_id: Option<String>,
    pub avatar_url: Option<String>,
    pub created_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}
impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            full_name: user.full_name,
            bio: user.bio,
            email: user.email,
            account_provider: user.account_provider,
            verified: user.verified,
            avatar_id: user.avatar_id,
            avatar_url: user.avatar_url,
            created_at: user.created_at,
            deleted_at: user.deleted_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserResponse,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
}
