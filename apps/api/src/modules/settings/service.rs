//! The settings rules, as orchestration (§5.1's `service` row).
//!
//! # Validation is not here, on purpose
//!
//! `master` re-checked the language tag in `update` with a `regex`-compiled pattern
//! *and* in the DTO with `validator`. That is two homes for one rule, and they drift
//! the moment one changes. §9.5 says the boundary validates; `dto::validate` does,
//! and this file assumes what reached it is well-formed — the same contract
//! `broadcast::service` works under.

use std::sync::Arc;

use uuid::Uuid;

use super::dto::{SettingsResponse, UpdateSettingsRequest};
use super::model::Settings;
use super::repository::{SettingsInput, SettingsRepo};
use crate::modules::profile::cache::ProfileCache;
use meno_core::Error as MenoError;

/// The settings module's rules.
#[derive(Clone)]
pub struct SettingsService {
    repo: Arc<dyn SettingsRepo>,
    /// Not this module's cache — it invalidates `/users/me`, whose payload embeds
    /// this one's data. The dependency runs settings → profile for exactly this
    /// method; `profile` → settings is the DTO pair. See both module docs.
    profile_cache: Arc<dyn ProfileCache>,
}

impl SettingsService {
    /// Build from its collaborators. `state.rs` supplies the concrete ones.
    #[must_use]
    pub fn new(repo: Arc<dyn SettingsRepo>, profile_cache: Arc<dyn ProfileCache>) -> Self {
        Self {
            repo,
            profile_cache,
        }
    }

    /// `GET /settings` — the stored row, or the defaults for an account that has
    /// never saved one.
    ///
    /// Never 404: "no row" is a default, not a missing resource (see
    /// [`Settings::new`]).
    ///
    /// # Errors
    ///
    /// A repository failure only.
    pub async fn get(&self, user_id: Uuid) -> Result<SettingsResponse, MenoError> {
        let settings = self
            .repo
            .find_by_user_id(user_id)
            .await?
            .unwrap_or_else(|| Settings::new(user_id));
        Ok(settings.into())
    }

    /// `PATCH /settings` — upsert, then invalidate the embedded copy on `/users/me`.
    ///
    /// # Errors
    ///
    /// A repository failure only. The boundary already refused an empty or invalid
    /// patch.
    pub async fn update(
        &self,
        user_id: Uuid,
        update: &UpdateSettingsRequest,
    ) -> Result<SettingsResponse, MenoError> {
        let input = SettingsInput {
            user_id,
            push_notifications: update.push_notifications,
            app_notifications: update.app_notifications,
            email_notifications: update.email_notifications,
            push_notification_token: update.push_notification_token.as_deref(),
            notification_preferences: update.notification_preferences.as_ref(),
            display: update.display.as_ref(),
            language: update.language.as_deref(),
        };

        let settings = self.repo.update(&input).await?;

        // `/users/me` embeds this block and is cached; a user who cannot see their
        // own change for the TTL would report it as "settings don't save". A failed
        // invalidation is logged rather than fatal: `GET /settings` reads the
        // database directly and is always fresh, and the stale copy is bounded by
        // the key's TTL.
        if let Err(error) = self.profile_cache.invalidate_cached_profile(user_id).await {
            tracing::warn!(%error, user_id = %user_id, "profile cache invalidation failed after a settings update");
        }

        Ok(settings.into())
    }

    /// `DELETE /settings/push-token` — drop the device token.
    ///
    /// Idempotent by construction: clearing a token that was never set is the state
    /// the caller asked for. The embedded `/users/me` block is invalidated for the
    /// same reason as in [`Self::update`].
    ///
    /// # Errors
    ///
    /// A repository failure only.
    pub async fn clear_push_token(&self, user_id: Uuid) -> Result<(), MenoError> {
        self.repo.clear_push_token(user_id).await?;
        if let Err(error) = self.profile_cache.invalidate_cached_profile(user_id).await {
            tracing::warn!(%error, user_id = %user_id, "profile cache invalidation failed after clearing a push token");
        }
        Ok(())
    }
}
