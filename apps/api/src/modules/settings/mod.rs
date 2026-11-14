//! The settings module — the caller's own switches.
//!
//! | Endpoint | Handler | Notes |
//! | --- | --- | --- |
//! | `GET /settings` | [`handlers::get_settings`] | defaults when no row exists yet |
//! | `PATCH /settings` | [`handlers::update_settings`] | upsert; invalidates `/users/me` |
//! | `DELETE /settings/push-token` | [`handlers::clear_push_token`] | idempotent unset |
//!
//! # What changed from `master`
//!
//! - **`errors.rs` → [`error`].** The `SettingsError` enum with its per-variant
//!   `IntoResponse` is the "nine independent error enums" pattern §4.2 retires; there
//!   is now one taxonomy and named constructors.
//! - **`repository.rs` → [`repository`]**, split into `pg`/`memory` like every other
//!   module's, and `update` became an upsert** — `master`'s plain `UPDATE … RETURNING`
//!   500'd every first save, because nothing inserts a settings row at signup.
//! - **A missing row is defaults, not 404.** `master`'s `get` answered
//!   `SETTINGS_NOT_FOUND` for every user who had never saved a setting — i.e. every
//!   user. [`model::Settings::new`] answers instead, and the upsert's INSERT branch
//!   agrees with it field for field.
//! - **`validator` + `regex` → hand-written [`dto::validate`].** Same rule,
//!   `^[a-zA-Z]{2}(-[a-zA-Z]{2,4})?$` written out as [`dto::is_language_tag`], no
//!   new dependencies.
//!
//! # The two edges to `profile`
//!
//! This module depends on `profile` twice, deliberately: [`dto::SettingsResponse`] is
//! the block embedded in `profile`'s `MeResponse`, and the service holds a
//! `profile::cache::ProfileCache` so a change here invalidates that embedded copy —
//! otherwise a user would not see their own settings change until the cache TTL
//! expired. The reverse edge (`profile` → settings) is the DTO pair and the settings
//! repository `profile`'s `/users/me` reads. Mutual, narrow, and documented on both
//! sides.

pub mod dto;
pub mod error;
pub mod handlers;
pub mod model;
pub mod repository;
pub mod service;
pub mod state;
