//! The Postgres adapter for [`SettingsRepo`].
//!
//! Plain `sqlx` runtime queries, not `query!` macros — same trade as the rest of the
//! refactored tree: `cargo check` works with no database standing (§15), and the
//! `ON CONFLICT` logic below is asserted by the suite against the in-memory double
//! plus an `#[sqlx::test]` when the database is available.

use sqlx::PgPool;
use uuid::Uuid;

use super::{SettingsInput, SettingsRepo};
use crate::modules::settings::error;
use crate::modules::settings::model::Settings;
use meno_core::Error as MenoError;

/// The settings module's Postgres adapter. One pool, no state.
#[derive(Clone)]
pub struct PgSettingsRepo {
    pool: PgPool,
}

impl PgSettingsRepo {
    /// Wrap the application's pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl SettingsRepo for PgSettingsRepo {
    async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<Settings>, MenoError> {
        sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| error::internal("settings_find_by_user_id", e))
    }

    async fn update(&self, input: &SettingsInput<'_>) -> Result<Settings, MenoError> {
        // The double duty of each parameter, spelled out because the two branches
        // disagree on purpose:
        //
        // - the INSERT branch pairs every `None` with the *new-row* default, which is
        //   `Settings::new`'s value (`TRUE`, `TRUE`, `TRUE`, `'{}'`, `'system'`,
        //   `'en'`) — the migration's column defaults are `FALSE` and would make the
        //   first save disagree with what `GET` just showed;
        // - the DO UPDATE branch pairs every `None` with the *stored* value, which is
        //   what `COALESCE($n, column)` means for a partial patch.
        //
        // Reusing `$2..$8` in both branches is legal Postgres and is what keeps the
        // input and the fallback bound once rather than copied.
        sqlx::query_as::<_, Settings>(
            r"INSERT INTO settings (user_id, push_notifications, app_notifications,
                                    email_notifications, push_notification_token,
                                    notification_preferences, display, language)
              VALUES ($1,
                      COALESCE($2, TRUE),
                      COALESCE($3, TRUE),
                      COALESCE($4, TRUE),
                      $5,
                      COALESCE($6, '{}'::jsonb),
                      COALESCE($7, 'system'),
                      COALESCE($8, 'en'))
              ON CONFLICT (user_id) DO UPDATE SET
                  push_notifications       = COALESCE($2, settings.push_notifications),
                  app_notifications        = COALESCE($3, settings.app_notifications),
                  email_notifications      = COALESCE($4, settings.email_notifications),
                  push_notification_token  = COALESCE($5, settings.push_notification_token),
                  notification_preferences = COALESCE($6, settings.notification_preferences),
                  display                  = COALESCE($7, settings.display),
                  language                 = COALESCE($8, settings.language)
              RETURNING *",
        )
        .bind(input.user_id)
        .bind(input.push_notifications)
        .bind(input.app_notifications)
        .bind(input.email_notifications)
        .bind(input.push_notification_token)
        .bind(input.notification_preferences.cloned())
        .bind(input.display.copied())
        .bind(input.language)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| error::internal("settings_upsert", e))
    }

    async fn clear_push_token(&self, user_id: Uuid) -> Result<(), MenoError> {
        sqlx::query("UPDATE settings SET push_notification_token = NULL WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| error::internal("settings_clear_push_token", e))?;
        // Zero rows affected is success, not `not_found`: see the trait docs.
        Ok(())
    }
}
