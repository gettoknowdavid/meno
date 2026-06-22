use crate::modules::settings::errors::SettingsError;
use crate::modules::settings::model::{Display, Settings};
use uuid::Uuid;

#[derive(Clone)]
pub struct SettingsRepository {
    db: sqlx::PgPool,
}
impl SettingsRepository {
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
pub trait SettingsRepo: Send + Sync + 'static {
    async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<Settings>, SettingsError>;
    async fn update(&self, input: &SettingsInput<'_>) -> Result<Settings, SettingsError>;
    async fn clear_push_token(&self, user_id: Uuid) -> Result<(), SettingsError>;
}

#[async_trait::async_trait]
impl SettingsRepo for SettingsRepository {
    async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<Settings>, SettingsError> {
        sqlx::query_as!(
            Settings,
            "SELECT * FROM settings WHERE user_id = $1",
            user_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(SettingsError::Database)
    }

    async fn update(&self, input: &SettingsInput<'_>) -> Result<Settings, SettingsError> {
        sqlx::query_as!(
            Settings,
            r"UPDATE settings SET
                   push_notifications = COALESCE($1, push_notifications),
                   app_notifications = COALESCE($2, app_notifications),
                   email_notifications = COALESCE($3, email_notifications),
                   push_notification_token = COALESCE($4, push_notification_token),
                   notification_preferences = COALESCE($5, notification_preferences),
                   display = COALESCE($6, display),
                   language = COALESCE($7, language)
            WHERE user_id = $8
            RETURNING *",
            input.push_notifications,
            input.app_notifications,
            input.email_notifications,
            input.push_notification_token,
            input.notification_preferences,
            input.display as _,
            input.language,
            input.user_id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(SettingsError::Database)
    }

    async fn clear_push_token(&self, user_id: Uuid) -> Result<(), SettingsError> {
        sqlx::query!(
            "UPDATE settings SET push_notification_token = NULL WHERE user_id = $1",
            user_id
        )
        .execute(&self.db)
        .await
        .map_err(SettingsError::Database)?;
        Ok(())
    }
}

pub struct SettingsInput<'a> {
    pub user_id: Uuid,
    pub push_notifications: Option<bool>,
    pub app_notifications: Option<bool>,
    pub email_notifications: Option<bool>,
    pub push_notification_token: Option<&'a str>,
    pub notification_preferences: Option<&'a serde_json::Value>,
    pub display: Option<&'a Display>,
    pub language: Option<&'a str>,
}
