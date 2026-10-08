//! The profile module — the caller's own record and other people's.
//!
//! | Endpoint | Handler | Notes |
//! | --- | --- | --- |
//! | `GET /users/me` | [`handlers::get_me`] | embedded settings + providers; read-through cached |
//! | `PATCH /users/me` | [`handlers::update_me`] | partial update; invalidates the cache |
//! | `POST /users/me/avatar` | [`handlers::upload_avatar`] | one-step multipart upload (§3.3) |
//! | `GET /users/{id}` | [`handlers::get_profile`] | another user's public profile |
//! | `GET /users?q=` | [`handlers::search_profiles`] | keyset-paginated search |
//!
//! # What changed from `master`
//!
//! - **No presigning.** `master` minted a pre-signed PUT URL and then had the client
//!   PATCH `avatar_key` back; §3.3 retires signing, so the upload is one proxied
//!   `POST` that stores the object, updates the row and answers with the new keys.
//!   The two-step dance — and the `avatar_key` field, which accepted arbitrary
//!   client-chosen keys — are gone with it.
//! - **`errors.rs` → [`error`].** One taxonomy (§4.2); the twenty-variant enum and its
//!   `IntoResponse` impl are replaced by named constructors on `meno_core::Error`.
//! - **`repository.rs` → [`repository`]**, split into [`repository::pg`] (the
//!   adapter) and [`repository::memory`] (the §10 double) — the split `broadcast`
//!   landed first.
//! - **The cursor works.** `master`'s search never applied its cursor to the SQL
//!   (`rank > $n` where `$n` was never bound), so `?cursor=` silently returned page
//!   one forever. The filter here binds it, with the row-value comparison from
//!   [`crate::infrastructure::query`].
//!
//! # The layer rule
//!
//! Reaches down to `crate::types`, `crate::middleware`, `crate::infrastructure` and
//! `meno_core`, exactly as `modules/mod.rs` allows. It also reaches into
//! [`crate::modules::settings`] for one type — the settings block embedded in
//! [`dto::MeResponse`] — because `/users/me` is documented as returning settings and
//! splitting that block in two would let the two structs drift. That is the module's
//! only sideways edge; `state.rs` is where the concrete `SettingsRepository` is chosen.

pub mod cache;
pub mod dto;
pub mod error;
pub mod handlers;
pub mod model;
pub mod repository;
pub mod service;
pub mod state;
pub mod storage;
