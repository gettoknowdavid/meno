use crate::modules::settings::model::Settings;
use crate::modules::settings::model::{Display};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateSettingsRequest {
    pub push_notifications: Option<bool>,
    pub app_notifications: Option<bool>,
    pub email_notifications: Option<bool>,

    #[validate(length(max = 4096, message = "Push token too long"))]
    pub push_notification_token: Option<String>,

    pub display: Option<Display>,

    #[validate(length(min = 2, max = 10, message = "Invalid language code"))]
    pub language: Option<String>,

    pub notification_preferences: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsResponse {
    pub push_notifications: bool,
    pub app_notifications: bool,
    pub email_notifications: bool,
    pub display: Display,
    pub language: String,
    pub notification_preferences: serde_json::Value,
    /// Never echo the raw device token back to the client.
    pub has_push_token: bool,
}
impl From<Settings> for SettingsResponse {
    fn from(s: Settings) -> Self {
        Self {
            push_notifications: s.push_notifications,
            app_notifications: s.app_notifications,
            email_notifications: s.email_notifications,
            display: s.display,
            language: s.language,
            notification_preferences: s.notification_preferences,
            has_push_token: s.push_notification_token.is_some(),
        }
    }
}
