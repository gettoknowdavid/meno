use crate::modules::profile::cache::ProfileCache;
use crate::modules::settings::dto::{SettingsResponse, UpdateSettingsRequest};
use crate::modules::settings::errors::SettingsError;
use crate::modules::settings::repository::{SettingsInput, SettingsRepo};
use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::Arc;
use uuid::Uuid;

static LANG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z]{2}(-[a-zA-Z]{2,4})?$").unwrap());

#[derive(Clone)]
pub struct SettingsService {
    repo: Arc<dyn SettingsRepo>,
    profile_cache: Arc<dyn ProfileCache>,
}
impl SettingsService {
    pub fn new(repo: Arc<dyn SettingsRepo>, profile_cache: Arc<dyn ProfileCache>) -> Self {
        Self {
            repo,
            profile_cache,
        }
    }

    pub async fn get(&self, user_id: Uuid) -> Result<SettingsResponse, SettingsError> {
        let s = self
            .repo
            .find_by_user_id(user_id)
            .await?
            .ok_or(SettingsError::NotFound)?;
        Ok(s.into())
    }

    pub async fn update(
        &self,
        user_id: Uuid,
        req: &UpdateSettingsRequest,
    ) -> Result<SettingsResponse, SettingsError> {
        if let Some(lang) = &req.language
            && !LANG_RE.is_match(lang)
        {
            return Err(SettingsError::InvalidLanguage);
        }

        let input = SettingsInput {
            user_id,
            push_notifications: req.push_notifications,
            app_notifications: req.app_notifications,
            email_notifications: req.email_notifications,
            push_notification_token: req.push_notification_token.as_deref(),
            notification_preferences: req.notification_preferences.as_ref(),
            display: req.display.as_ref(),
            language: req.language.as_deref(),
        };
        let updated = self.repo.update(&input).await?;

        // `GET /users/me` embeds settings and is cached 60s — must invalidate
        // or the user won't see their own change for up to a minute.
        let _ = self.profile_cache.invalidate_cached_profile(user_id).await;

        Ok(updated.into())
    }

    pub async fn clear_push_token(&self, user_id: Uuid) -> Result<(), SettingsError> {
        self.repo.clear_push_token(user_id).await?;
        let _ = self.profile_cache.invalidate_cached_profile(user_id).await;
        Ok(())
    }
}
