use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum UserRole {
    User,
    Admin,
}
impl FromStr for UserRole {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(UserRole::User),
            "admin" => Ok(UserRole::Admin),
            _ => Err(anyhow::anyhow!("Invalid role")),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AccountProvider {
    Email,
    Google,
    Apple,
    Facebook,
}
impl FromStr for AccountProvider {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "email" => Ok(AccountProvider::Email),
            "google" => Ok(AccountProvider::Google),
            "apple" => Ok(AccountProvider::Apple),
            "facebook" => Ok(AccountProvider::Facebook),
            _ => Err(anyhow::anyhow!("Invalid account provider")),
        }
    }
}

#[derive(Clone, Debug)]
pub enum OtpType {
    VerifyEmail,
    ResetPassword,
}
impl FromStr for OtpType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "verify_email" => Ok(OtpType::VerifyEmail),
            "reset_password" => Ok(OtpType::ResetPassword),
            _ => Err(anyhow::anyhow!("Invalid OTP type")),
        }
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
    pub account_provider: AccountProvider,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
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
