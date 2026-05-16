use crate::modules::auth::dto::UserResponse;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use strum::{AsRefStr, Display, EnumString};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, Display, AsRefStr, EnumString)]
#[strum(serialize_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum UserRole {
    User,
    Admin,
}
#[derive(Clone, Debug, Serialize, Deserialize, Display, AsRefStr, EnumString)]
#[strum(serialize_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum AccountProvider {
    Email,
    Google,
    Apple,
    Facebook,
}

impl From<String> for UserRole {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "admin" => UserRole::Admin,
            _ => UserRole::User,
        }
    }
}

impl From<String> for AccountProvider {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "google" => AccountProvider::Google,
            "apple" => AccountProvider::Apple,
            "facebook" => AccountProvider::Facebook,
            _ => AccountProvider::Email,
        }
    }
}

#[derive(Clone, Debug, EnumString)]
#[strum(serialize_all = "snake_case")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum OtpType {
    VerifyEmail,
    ResetPassword,
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
    pub account_provider: AccountProvider,
    pub role: UserRole,
    pub password: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}
impl User {
    pub fn into_response(self) -> UserResponse {
        UserResponse {
            id: self.id,
            full_name: self.full_name,
            bio: self.bio,
            email: self.email,
            account_provider: self.account_provider,
            verified: self.verified,
            avatar_id: self.avatar_id,
            avatar_url: self.avatar_url,
            created_at: self.created_at,
            deleted_at: self.deleted_at,
        }
    }
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
