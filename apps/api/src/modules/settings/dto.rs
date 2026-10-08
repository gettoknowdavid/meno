//! The settings wire contract, and the boundary rules for it (§9.5).
//!
//! # Casing
//!
//! Snake_case, like `master` shipped and like `profile`'s DTOs: `push_notifications`,
//! `has_push_token`, `notification_preferences`. Field names in a `FieldErrors`
//! response are the same strings as the JSON keys, so a client can bind an error back
//! to the input that caused it.
//!
//! # Validation is hand-written
//!
//! `master` used the `validator` derive for the token length and the service held a
//! `regex`-compiled language check. Both rules are now written out here, at the
//! boundary the plan names (§9.5), with no `regex`, `once_cell` or `validator`
//! dependency: [`is_language_tag`] is `master`'s `^[a-zA-Z]{2}(-[a-zA-Z]{2,4})?$
//! expressed as a match, so what the old pattern accepted is what this accepts.

use serde::{Deserialize, Serialize};

use super::model::{Display, Settings};
use meno_core::Error as MenoError;
use meno_core::error::FieldErrors;

/// The longest device token this API accepts, in characters.
///
/// Mirrors `master`'s `length(max = 4096)`; FCM's own tokens are far smaller, and the
/// column is `TEXT`, so this is a sanity bound rather than a storage one.
pub const PUSH_TOKEN_MAX_CHARS: usize = 4096;

/// A request that can check itself before a handler looks at it.
pub trait Validatable {
    /// Run every rule, accumulating rather than short-circuiting.
    ///
    /// # Errors
    ///
    /// [`MenoError::Validation`] listing every field that failed and why.
    fn validate(&self) -> Result<(), MenoError>;
}

/// `PATCH /settings` — every field optional, but not *all* of them.
///
/// An empty patch is a 422 rather than a round trip that persists defaults nobody
/// asked for — the same rule `broadcast`'s and `profile`'s `PATCH` apply, so all
/// three modules answer `{}` the same way.
#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    /// Push (FCM) notifications on/off.
    pub push_notifications: Option<bool>,
    /// In-app notifications on/off.
    pub app_notifications: Option<bool>,
    /// Email notifications on/off.
    pub email_notifications: Option<bool>,
    /// A new device token for FCM. `null`/absent means "keep the current one" —
    /// use `DELETE /settings/push-token` to clear it, which is the one operation
    /// that means "none".
    pub push_notification_token: Option<String>,
    /// UI theme: `system`, `light` or `dark`.
    pub display: Option<Display>,
    /// Language tag, e.g. `en` or `en-GB`.
    pub language: Option<String>,
    /// Free-form notification preferences. The API stores and returns it verbatim —
    /// it has no opinion about the client's shape, only that it is JSON.
    pub notification_preferences: Option<serde_json::Value>,
}

impl Validatable for UpdateSettingsRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        if let Some(token) = &self.push_notification_token
            && token.chars().count() > PUSH_TOKEN_MAX_CHARS
        {
            fields.push(
                "push_notification_token",
                format!("must be at most {PUSH_TOKEN_MAX_CHARS} characters"),
            );
        }

        if let Some(language) = &self.language
            && !is_language_tag(language)
        {
            fields.push("language", "must be a language tag like `en` or `en-GB`");
        }

        let touches_anything = self.push_notifications.is_some()
            || self.app_notifications.is_some()
            || self.email_notifications.is_some()
            || self.push_notification_token.is_some()
            || self.display.is_some()
            || self.language.is_some()
            || self.notification_preferences.is_some();
        if !touches_anything {
            fields.push("language", "at least one field is required");
        }

        fields.into_result()
    }
}

/// `master`'s language rule: `^[a-zA-Z]{2}(-[a-zA-Z]{2,4})?$`, written out.
///
/// A deliberate subset of BCP-47 — two-letter primary subtag, optional region —
/// which is what every locale the app ships with looks like. Anything richer
/// (`en-US-x-private`) is refused rather than half-parsed, because a locale the
/// client then cannot load is worse than a 422 at save time.
#[must_use]
pub fn is_language_tag(value: &str) -> bool {
    let mut parts = value.split('-');

    let Some(primary) = parts.next() else {
        return false;
    };
    if primary.len() != 2 || !primary.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }

    let Some(region) = parts.next() else {
        return true; // just `en`
    };
    // `{2,4}` in the pattern this replaces: one letter and three more would be a
    // different tag, not a region.
    if !(2..=4).contains(&region.len()) || !region.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    parts.next().is_none() // no third subtag
}

/// `GET /settings` and the `PATCH` answer — the seven switches, never the raw token.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsResponse {
    /// Push (FCM) notifications on/off.
    pub push_notifications: bool,
    /// In-app notifications on/off.
    pub app_notifications: bool,
    /// Email notifications on/off.
    pub email_notifications: bool,
    /// UI theme.
    pub display: Display,
    /// Language tag.
    pub language: String,
    /// Free-form client preferences, verbatim.
    pub notification_preferences: serde_json::Value,
    /// Whether a device token is on file. Never the token itself — echoing a
    /// credential back is how one ends up in a log line (§4.2).
    pub has_push_token: bool,
}

impl From<Settings> for SettingsResponse {
    fn from(settings: Settings) -> Self {
        Self {
            push_notifications: settings.push_notifications,
            app_notifications: settings.app_notifications,
            email_notifications: settings.email_notifications,
            display: settings.display,
            language: settings.language,
            notification_preferences: settings.notification_preferences,
            has_push_token: settings.push_notification_token.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The hand-written rules, checked against the pattern they replace.

    use super::*;

    #[test]
    fn language_accepts_what_masters_regex_accepted() {
        // `^[a-zA-Z]{2}(-[a-zA-Z]{2,4})?$`
        for good in ["en", "EN", "en-GB", "pt-BR", "zh-Hans"] {
            assert!(is_language_tag(good), "{good} should be accepted");
        }
        for bad in [
            "",
            "e",
            "eng",
            "en-",
            "en-U",
            "en-GB-extra",
            "english",
            "en_US",
            "1n",
            "-en",
        ] {
            assert!(!is_language_tag(bad), "{bad} should be refused");
        }
    }

    #[test]
    fn an_empty_patch_is_refused_and_names_a_field() {
        let request = UpdateSettingsRequest {
            push_notifications: None,
            app_notifications: None,
            email_notifications: None,
            push_notification_token: None,
            display: None,
            language: None,
            notification_preferences: None,
        };
        let MenoError::Validation { fields } = request.validate().expect_err("nothing to do")
        else {
            panic!("an empty patch must be a validation failure");
        };
        assert!(fields.contains_key("language"), "{fields:?}");
    }

    #[test]
    fn an_oversized_token_is_refused() {
        let request = UpdateSettingsRequest {
            push_notifications: None,
            app_notifications: None,
            email_notifications: None,
            push_notification_token: Some("x".repeat(PUSH_TOKEN_MAX_CHARS + 1)),
            display: None,
            language: None,
            notification_preferences: None,
        };
        let MenoError::Validation { fields } = request.validate().expect_err("too long") else {
            panic!("an oversized token must be a validation failure");
        };
        assert!(fields.contains_key("push_notification_token"), "{fields:?}");
    }
}
