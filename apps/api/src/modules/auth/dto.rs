use crate::modules::auth::model::{AuthProvider, OtpType};
use crate::modules::auth::validators::{validate_email, validate_password};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::serde::rfc3339;
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

    #[validate(email(message = "Invalid email format"))]
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
pub struct LogoutRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RefreshTokenRequest {
    #[validate(length(min = 1, message = "Refresh token is required"))]
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct GoogleWebAuthRequest {
    #[validate(length(min = 1, message = "Authorization code is required"))]
    pub code: String,

    #[validate(length(min = 1, message = "State token is required"))]
    pub state: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct GoogleMobileAuthRequest {
    #[validate(length(min = 1, message = "ID token is required"))]
    pub id_token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ForgotPasswordRequest {
    #[validate(custom(function = "validate_email"))]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResendOtpRequest {
    #[validate(custom(function = "validate_email"))]
    pub email: String,

    pub otp_type: OtpType,
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
    pub verified: bool,
    pub avatar_id: Option<String>,
    pub avatar_url: Option<String>,
    pub providers: Vec<AuthProvider>,

    #[serde(with = "rfc3339")]
    pub created_at: OffsetDateTime,

    #[serde(with = "rfc3339::option")]
    pub deleted_at: Option<OffsetDateTime>,
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

#[derive(Debug, Serialize)]
pub struct GoogleUrlResponse {
    pub url: String,
}
