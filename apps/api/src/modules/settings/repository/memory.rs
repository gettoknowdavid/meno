//! The in-memory double of [`SettingsRepo`] (§10).
//!
//! The point of this file is that it is *not* more permissive than the SQL: `update`
//! creates a row from [`Settings::new`] when there is none (the INSERT branch), and
//! applies only the `Some` fields (the COALESCE branch), so a test that passes here
//! describes something the upsert really does.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use uuid::Uuid;

use super::{SettingsInput, SettingsRepo};
use crate::modules::settings::model::Settings;
use meno_core::Error as MenoError;

/// The double the settings suite (and `profile_router.rs`) runs against.
#[derive(Debug, Default)]
pub struct InMemorySettingsRepo {
    rows: Mutex<HashMap<Uuid, Settings>>,
}

impl InMemorySettingsRepo {
    /// An empty repository — every user starts with no row, exactly as in Postgres.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The stored row, for assertions that do not go through the trait.
    #[must_use]
    pub fn row(&self, user_id: Uuid) -> Option<Settings> {
        self.lock().get(&user_id).cloned()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Uuid, Settings>> {
        self.rows.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait::async_trait]
impl SettingsRepo for InMemorySettingsRepo {
    async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<Settings>, MenoError> {
        Ok(self.lock().get(&user_id).cloned())
    }

    async fn update(&self, input: &SettingsInput<'_>) -> Result<Settings, MenoError> {
        let mut rows = self.lock();
        // INSERT branch: no row yet means defaults, exactly like `Settings::new`.
        let row = rows
            .entry(input.user_id)
            .or_insert_with(|| Settings::new(input.user_id));

        // COALESCE branch: `Some` overwrites, `None` keeps — including on the row
        // just created, where "keeps" means the default.
        if let Some(value) = input.push_notifications {
            row.push_notifications = value;
        }
        if let Some(value) = input.app_notifications {
            row.app_notifications = value;
        }
        if let Some(value) = input.email_notifications {
            row.email_notifications = value;
        }
        if let Some(value) = input.push_notification_token {
            row.push_notification_token = Some(value.to_owned());
        }
        if let Some(value) = input.notification_preferences {
            row.notification_preferences = value.clone();
        }
        if let Some(value) = input.display {
            row.display = *value;
        }
        if let Some(value) = input.language {
            row.language = value.to_owned();
        }

        Ok(row.clone())
    }

    async fn clear_push_token(&self, user_id: Uuid) -> Result<(), MenoError> {
        if let Some(row) = self.lock().get_mut(&user_id) {
            row.push_notification_token = None;
        }
        // No row → no token → already cleared. Same as the SQL's zero-row UPDATE.
        Ok(())
    }
}
