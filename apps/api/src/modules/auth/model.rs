use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt::Display;
use strum::{AsRefStr, Display, EnumString};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, Display, AsRefStr, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum UserRole {
    User,
    Admin,
}
impl From<String> for UserRole {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "admin" => UserRole::Admin,
            _ => UserRole::User,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Display, AsRefStr, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum AuthProvider {
    Email,
    Google,
    Apple,
    Facebook,
}
impl From<String> for AuthProvider {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "google" => AuthProvider::Google,
            "apple" => AuthProvider::Apple,
            "facebook" => AuthProvider::Facebook,
            _ => AuthProvider::Email,
        }
    }
}

#[derive(Clone, Debug, EnumString, Deserialize, Serialize, AsRefStr, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum OtpType {
    VerifyEmail,
    ResetPassword,
}
impl Display for OtpType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            OtpType::VerifyEmail => "verify_email",
            OtpType::ResetPassword => "reset_password",
        }
        .to_string();
        write!(f, "{str}")
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub email: String,
    pub avatar_id: Option<String>,
    pub avatar_url: Option<String>,
    pub verified: bool,
    pub role: UserRole,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, FromRow)]
pub struct UserIdentity {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider_type: AuthProvider,
    pub provider_user_id: String,
    pub password_hash: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: Option<OffsetDateTime>,
}

pub struct UserWithIdentity {
    pub user: User,
    pub identity: UserIdentity,
}

#[derive(Debug, Clone, FromRow)]
pub struct RefreshToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct Otp {
    pub id: Uuid,
    pub email: String,
    pub code: String,
    pub r#type: OtpType,
    pub used: bool,
    pub create_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}
