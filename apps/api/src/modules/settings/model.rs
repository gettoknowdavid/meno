use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use strum::{AsRefStr, EnumString};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Settings {
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
impl Settings {
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
