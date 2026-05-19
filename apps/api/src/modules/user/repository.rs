use crate::modules::auth::model::{AuthProvider, User, UserRole};
use crate::modules::user::errors::UserError;
use crate::modules::user::model::{GeneralSettings, UserWithSettings};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone)]
pub struct UserRepository {
    pub db: sqlx::PgPool,
}
impl UserRepository {
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }

    pub async fn find_user_with_settings(
        &self,
        user_id: Uuid,
    ) -> Result<Option<UserWithSettings>, UserError> {
        let record = sqlx::query!(
            r#"SELECT u.*,
                    gs.id as "settings_id",
                    gs.user_id,
                    gs.push_notifications,
                    gs.app_notifications,
                    gs.email_notifications,
                    gs.push_notification_token,
                    gs.notification_preferences,
                    gs.display,
                    gs.language,
                    ARRAY_AGG(DISTINCT ui.provider_type) as "provider_types!: Vec<Option<String>>"
               FROM users u LEFT JOIN general_settings gs ON gs.user_id = u.id
                             LEFT JOIN user_identities ui ON ui.user_id = u.id
               WHERE u.id = $1 AND u.deleted_at IS NULL
               GROUP BY u.id, u.full_name, u.bio, u.email, u.avatar_id, u.avatar_url,
                        u.verified, u.role, u.created_at, u.updated_at, u.deleted_at,
                        gs.id, gs.user_id, gs.push_notifications, gs.app_notifications,
                        gs.email_notifications, gs.push_notification_token,
                        gs.notification_preferences, gs.display, gs.language"#,
            user_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(UserError::Database)?;

        let record = match record {
            None => return Ok(None),
            Some(r) => r,
        };

        let user = User {
            id: record.id,
            full_name: record.full_name,
            bio: record.bio,
            email: record.email,
            avatar_id: record.avatar_id,
            avatar_url: record.avatar_url,
            verified: record.verified,
            role: UserRole::from(record.role),
            created_at: record.created_at,
            updated_at: record.updated_at,
            deleted_at: record.deleted_at,
        };
        let settings = GeneralSettings {
            id: record.settings_id,
            user_id: record.user_id,
            push_notifications: record.push_notifications,
            app_notifications: record.app_notifications,
            email_notifications: record.email_notifications,
            push_notification_token: record.push_notification_token,
            notification_settings: record.notification_preferences,
            display: record.display,
            language: record.language,
        };
        let providers: Vec<AuthProvider> = record
            .provider_types
            .into_iter()
            .filter_map(|opt| opt.and_then(|s| AuthProvider::from_str(&s).ok()))
            .collect();
        let user_with_settings = UserWithSettings {
            user,
            settings,
            providers,
        };

        Ok(Some(user_with_settings))
    }
}
