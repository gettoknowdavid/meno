use crate::modules::auth::model::{AuthProvider, User};
use crate::modules::user::dto::GeneralSettingsResponse;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct UserWithSettings {
    pub user: User,
    pub settings: GeneralSettings,
    pub providers: Vec<AuthProvider>,
}

#[derive(Debug, Clone)]
pub struct GeneralSettings {
    pub id: Uuid,
    pub user_id: Uuid,
    pub push_notifications: bool,
    pub app_notifications: bool,
    pub email_notifications: bool,
    pub push_notification_token: Option<String>,
    pub notification_settings: serde_json::Value,
    pub display: String,
    pub language: String,
}
impl Into<GeneralSettingsResponse> for GeneralSettings {
    fn into(self) -> GeneralSettingsResponse {
        GeneralSettingsResponse {
            push_notifications: self.push_notifications,
            app_notifications: self.app_notifications,
            email_notifications: self.email_notifications,
            display: self.display,
            language: self.language,
            notification_settings: self.notification_settings,
        }
    }
}
