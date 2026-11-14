//! What settings knows about the `settings` table — as a port, not as SQL.
//!
//! The same split as `profile::repository` and `broadcast::repository`: [`pg`] is
//! Postgres, [`memory`] is the §10 double, and [`SettingsRepo`] is the only name the
//! service knows. `state.rs` chooses between them (§5.6).
//!
//! # Why `update` is an upsert
//!
//! `master` ran a plain `UPDATE … WHERE user_id = $1 RETURNING *` and then
//! `fetch_one`, so a user whose row did not exist yet — every user, since nothing
//! inserts one at signup — got `RowNotFound` surfacing as a 500 on their first
//! settings save. The `ON CONFLICT` here is that bug's fix, and it is why
//! [`SettingsRepo::update`] is named for what it does rather than for `master`'s
//! `update`.

pub mod memory;
pub mod pg;

use uuid::Uuid;

use meno_core::Error as MenoError;

use super::model::{Display, Settings};

#[cfg(any(test, feature = "test-support"))]
pub use memory::InMemorySettingsRepo;
pub use pg::PgSettingsRepo;

/// One `PATCH /settings`, as the repository receives it: `None` means "leave it".
///
/// Mirrors the SQL's `COALESCE($n, column)` word for word — a `None` here must not
/// blank a column, and the in-memory double enforces the same rule so a test cannot
/// pass against behaviour the database would not have.
#[derive(Debug)]
pub struct SettingsInput<'a> {
    /// Whose row this is.
    pub user_id: Uuid,
    /// `None` keeps the stored value.
    pub push_notifications: Option<bool>,
    /// `None` keeps the stored value.
    pub app_notifications: Option<bool>,
    /// `None` keeps the stored value.
    pub email_notifications: Option<bool>,
    /// `None` keeps the stored token; use [`SettingsRepo::clear_push_token`] to unset.
    pub push_notification_token: Option<&'a str>,
    /// `None` keeps the stored JSON.
    pub notification_preferences: Option<&'a serde_json::Value>,
    /// `None` keeps the stored theme.
    pub display: Option<&'a Display>,
    /// `None` keeps the stored language.
    pub language: Option<&'a str>,
}

/// The user's switches — how settings reads and writes them.
#[async_trait::async_trait]
pub trait SettingsRepo: Send + Sync + 'static {
    /// The stored row, or `None` for an account that has never saved settings.
    async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<Settings>, MenoError>;

    /// Apply a partial update, creating the row if this is the first write.
    ///
    /// Every `COALESCE` has two jobs: keep the stored value for a `None` input on an
    /// existing row, and fall back to [`Settings::new`]'s defaults for a `None` input
    /// on the row being inserted — so a first save of `{display: "dark"}` produces a
    /// row identical to what `GET` showed a moment earlier.
    async fn update(&self, input: &SettingsInput<'_>) -> Result<Settings, MenoError>;

    /// Unset the device token.
    ///
    /// A no-op success when there is no row or no token: clearing what is not there
    /// is the state the caller asked for, not an error they could retry differently.
    async fn clear_push_token(&self, user_id: Uuid) -> Result<(), MenoError>;
}
