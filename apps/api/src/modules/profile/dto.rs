//! The profile wire contract, and the boundary rules for it (§9.5).
//!
//! # Casing
//!
//! These structs serialise as they are written — snake_case: `full_name`,
//! `avatar_url`, `email_verified`. That is the shape `master` shipped for
//! `/users/me` and the shape `auth` still ships for its payloads; `broadcast`'s
//! camelCase is `broadcast`'s own choice, and the envelope docs are explicit that
//! casing is "applied per struct". What matters is that a field's JSON key and the
//! name a `FieldErrors` entry uses are the *same string*, so a client can bind the
//! error to the input that caused it.
//!
//! # Validation is hand-written
//!
//! `master` used the `validator` derive; this is the `broadcast` approach instead —
//! a `validate` method that accumulates every problem into one
//! [`FieldErrors`] so the client learns about all of them in one round trip, and
//! so the message and the rule quote the same constant. There is no `regex` or
//! `once_cell` in this file: the two rules that are not arithmetic are written out.
//!
//! # Dates
//!
//! Every `OffsetDateTime` carries `#[serde(with = "time::serde::rfc3339")]`, so a
//! date cannot be `time`'s nine-number tuple on the wire; `tests/wire_dates.rs`
//! enforces it.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::middleware::auth::AuthProvider;
use crate::modules::settings::dto::SettingsResponse;
use meno_core::error::FieldErrors;
use meno_core::{Cursor, CursorParams, Error as MenoError};

/// The shortest full name `PATCH /users/me` accepts, in characters.
///
/// Matches `auth`'s registration minimum: a name the signup form accepted must not
/// become invalid the moment the user tries to edit their profile.
pub const NAME_MIN: usize = 2;

/// The longest full name, in characters. No column constraint (`TEXT`), but unbounded
/// names break layouts before they break the database.
pub const NAME_MAX: usize = 100;

/// The longest bio, in characters.
///
/// Matches the column's `VARCHAR(244)`, so a bio that passes validation cannot be
/// refused by the database with a 500 — the exact failure the plan records for
/// `master`'s unbounded validator.
pub const BIO_MAX: usize = 244;

/// The shortest search term, in characters. Below this, `plainto_tsquery` matches
/// enough of the table to be a scan dressed up as a search.
pub const SEARCH_QUERY_MIN: usize = 3;

/// A request that can check itself before a handler looks at it.
pub trait Validatable {
    /// Run every rule, accumulating rather than short-circuiting.
    ///
    /// # Errors
    ///
    /// [`MenoError::Validation`] listing every field that failed and why. Returning on
    /// the first failure would make a client with two mistakes need two round trips.
    fn validate(&self) -> Result<(), MenoError>;
}

/// `PATCH /users/me` — every field optional, but not *all* of them.
///
/// An empty patch is a 422 rather than a round trip that reports success without
/// changing anything, the same rule `broadcast`'s `PATCH` applies.
#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    /// New display name. `null`/absent means "leave it alone".
    pub full_name: Option<String>,
    /// New bio. `null`/absent means "leave it alone"; send `""` to clear it.
    ///
    /// # Why clearing needs an empty string
    ///
    /// The repository writes with `COALESCE($2, bio)`, so SQL `NULL` cannot
    /// distinguish "absent" from "clear" — `master` had the same asymmetry. The DTO
    /// documents it instead of pretending the wire has a third state.
    pub bio: Option<String>,
}

impl Validatable for UpdateProfileRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        if let Some(name) = &self.full_name {
            let len = name.trim().chars().count();
            if !(NAME_MIN..=NAME_MAX).contains(&len) {
                fields.push(
                    "full_name",
                    format!("must be between {NAME_MIN} and {NAME_MAX} characters"),
                );
            }
        }

        if let Some(bio) = &self.bio
            && bio.chars().count() > BIO_MAX
        {
            fields.push("bio", format!("must be at most {BIO_MAX} characters"));
        }

        if self.full_name.is_none() && self.bio.is_none() {
            fields.push("full_name", "at least one of full_name or bio is required");
        }

        fields.into_result()
    }
}

/// `GET /users?q=&limit=&cursor=`.
///
/// Flattened onto [`CursorParams`] so this endpoint parses `?limit=` and `?cursor=`
/// exactly like every other paginated list, with the search term alongside.
#[derive(Debug, Deserialize)]
pub struct ProfileSearchQuery {
    /// The search term. Matched against `users.search_vector` (name + bio).
    ///
    /// `#[serde(default)]` so an *absent* term and a *too-short* term are the same
    /// failure: without it, a missing `q` is axum's transport-level 400 with a
    /// plain-text body, while `?q=a` is a §4.2 envelope 422 — two shapes for one
    /// mistake. With it, both arrive as `VALIDATION_FAILED` naming `q`.
    #[serde(default)]
    pub q: String,
    /// The shared limit/cursor pair, flattened into the query string.
    #[serde(flatten)]
    pub pagination: CursorParams,
}

impl ProfileSearchQuery {
    /// Validated limit, clamped to [1, 100]. Default: 20.
    #[must_use]
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }

    /// `limit + 1` — the "fetch one extra to detect next page" trick (§9.4).
    #[must_use]
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }

    /// The opaque cursor from the previous page, undecoded.
    #[must_use]
    pub fn cursor(&self) -> Option<&Cursor> {
        self.pagination.cursor.as_ref()
    }
}

impl Validatable for ProfileSearchQuery {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        if self.q.trim().chars().count() < SEARCH_QUERY_MIN {
            fields.push(
                "q",
                format!("must be at least {SEARCH_QUERY_MIN} characters"),
            );
        }

        fields.into_result()
    }
}

/// `GET /users/me` — the caller's own record, settings included.
///
/// Derives `Deserialize` as well as `Serialize` because it round-trips through the
/// read-through cache: the Redis entry *is* this struct, so a field that could be
/// written but not read would make every cache hit a miss.
///
/// # Why settings live here
///
/// `/users/me` is one round trip: the profile screen needs the name and avatar and
/// the notification/language/display switches together, and `master` shipped them
/// embedded. The block is [`SettingsResponse`] itself — not a copy — so the two
/// endpoints cannot disagree about what a setting is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeResponse {
    /// The user's id.
    pub id: Uuid,
    /// The display name.
    pub full_name: String,
    /// The email address. Never exposed on anyone else's profile.
    pub email: String,
    /// Whether the email address has been confirmed (`users.verified`).
    pub email_verified: bool,
    /// The free-text bio, at most [`BIO_MAX`] characters.
    pub bio: Option<String>,
    /// Storage key of the current avatar, if one has been uploaded.
    pub avatar_id: Option<String>,
    /// CDN URL for that avatar, captured at upload time. `null` when the object
    /// store has no public base URL configured — the key alone still resolves it.
    pub avatar_url: Option<String>,
    /// The notification, display, language and preference switches.
    pub settings: SettingsResponse,
    /// The identity providers linked to this account (`password`, `google`).
    pub providers: Vec<AuthProvider>,
    /// Follower count, maintained by the `user_subscribers` trigger.
    pub followers: i64,
    /// Following count.
    pub following: i64,
    /// Broadcast count.
    pub broadcasts: i64,
    /// Account creation, in UTC.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// `GET /users/{id}` — someone else's profile.
///
/// No email, no providers, no settings: those exist only for the caller themself.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PublicProfileResponse {
    /// The profile owner's id.
    pub id: Uuid,
    /// The display name.
    pub full_name: String,
    /// The bio, if any.
    pub bio: Option<String>,
    /// CDN URL of the avatar, if any.
    pub avatar_url: Option<String>,
    /// Whether the *viewer* follows this profile.
    pub is_following: bool,
    /// Follower count.
    pub followers: i64,
    /// Following count.
    pub following: i64,
    /// Broadcast count.
    pub broadcasts: i64,
    /// Account creation, in UTC.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// One hit from `GET /users?q=`, shaped for the cursor page.
///
/// A row type as well as a wire type: `repository::pg` maps the search's `EXISTS`
/// subquery straight onto `is_following`, which keeps the viewer-specific part of the
/// row in the same query as the profile columns.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProfileSearchResult {
    /// The profile owner's id.
    pub id: Uuid,
    /// The display name.
    pub full_name: String,
    /// The bio, if any.
    pub bio: Option<String>,
    /// CDN URL of the avatar, if any.
    pub avatar_url: Option<String>,
    /// Whether the *viewer* follows this profile.
    pub is_following: bool,
    /// Follower count.
    pub followers: i64,
    /// Following count.
    pub following: i64,
    /// Broadcast count.
    pub broadcasts: i64,
    /// Account creation, in UTC — also the search's sort key and cursor.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// The text fields of `POST /users/me/avatar`.
///
/// The upload takes a single file part named `file` and no text fields; the type
/// exists because [`crate::middleware::extractors::MenoMultipartForm`] deserialises
/// whatever text parts *are* sent, and `{}` is the honest shape for "there are none".
/// Anything a client sends as a file part is handled in the handler, never here.
#[derive(Debug, Default, Deserialize)]
pub struct UploadAvatarForm {}

/// The upload's answer: the keys the row now carries.
///
/// Kept small — and intentionally shaped like `master`'s `AvatarUploadUrlResponse`
/// (`avatar_id` + `avatar_url`) so a client that merged that response can merge this
/// one. There is no `expires_at`: nothing was pre-signed.
#[derive(Debug, Clone, Serialize)]
pub struct UploadAvatarResponse {
    /// The storage key now stored in `users.avatar_id`.
    pub avatar_id: String,
    /// The CDN URL now stored in `users.avatar_url`. `null` when storage has no
    /// public base URL.
    pub avatar_url: Option<String>,
}
