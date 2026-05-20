use crate::modules::auth::model::{AuthProvider, User};
use crate::modules::user::dto::GeneralSettingsResponse;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct UserWithSettings {
    pub user: User,
    pub settings: GeneralSettings,
    pub providers: Vec<AuthProvider>,
}

#[derive(Clone, Debug, Serialize, Deserialize, strum::Display, AsRefStr, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum Display {
    System,
    Light,
    Dark,
}
impl From<String> for Display {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "dark" => Display::Dark,
            "light" => Display::Light,
            _ => Display::System,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GeneralSettings {
    pub id: Uuid,
    pub user_id: Uuid,
    pub push_notifications: bool,
    pub app_notifications: bool,
    pub email_notifications: bool,
    pub push_notification_token: Option<String>,
    pub notification_preferences: serde_json::Value,
    pub display: Display,
    pub language: String,
}
impl GeneralSettings {
    pub fn new(user_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            push_notifications: true,
            app_notifications: true,
            email_notifications: true,
            push_notification_token: None,
            notification_preferences: serde_json::json!({}),
            display: Display::System,
            language: "en".to_string(),
        }
    }
}
impl Into<GeneralSettingsResponse> for GeneralSettings {
    fn into(self) -> GeneralSettingsResponse {
        GeneralSettingsResponse {
            push_notifications: self.push_notifications,
            app_notifications: self.app_notifications,
            email_notifications: self.email_notifications,
            display: self.display,
            language: self.language,
            notification_preferences: self.notification_preferences,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserStats {
    pub broadcasts: i64,
    pub followers: i64,
    pub following: i64,
}
impl Default for UserStats {
    fn default() -> Self {
        Self {
            broadcasts: 0,
            followers: 0,
            following: 0,
        }
    }
}
