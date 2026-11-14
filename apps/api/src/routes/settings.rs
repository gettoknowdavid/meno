//! The settings module's endpoints, with their layers attached.
//!
//! Same shape as `routes/profile.rs`: relative paths inside, the nest carries
//! `/settings`, the guard wraps everything.
//!
//! # Rate limiting
//!
//! `LimitPolicy::read()` on `GET`, `LimitPolicy::write()` on the two mutations —
//! the generic API policy, for the same reason profile uses it: nothing here hashes
//! a credential, and a settings screen that saves twice in a minute is normal use.
//!
//! # No idempotency layer
//!
//! A `PATCH` that is last-write-wins and a `DELETE` that unsets are idempotent by
//! definition; see `routes/profile.rs` for the longer version.

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, patch};

use crate::infrastructure::redis::Redis;
use crate::middleware::auth::{AuthState, auth_middleware};
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::settings::handlers;
use crate::modules::settings::state::SettingsState;

/// Every path this nest serves, written out.
///
/// Written out rather than derived from the router, because a list derived from the
/// thing it is meant to check cannot fail. [`crate::routes::tests`] asserts against it.
pub const SETTINGS_PATHS: &[&str] = &["/settings", "/settings/push-token"];

/// The settings sub-router, before it is lifted to [`SettingsState`] by the caller.
pub fn routes(guard: AuthState, redis: Arc<Redis>) -> Router<SettingsState> {
    let reads = Router::new()
        .route("/", get(handlers::get_settings))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::read()),
            rate_limit_middleware,
        ));

    let writes = Router::new()
        .route("/", patch(handlers::update_settings))
        .route("/push-token", delete(handlers::clear_push_token))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::write()),
            rate_limit_middleware,
        ));

    // Auth outermost of this stack — the required layer order, same as profile.
    reads
        .merge(writes)
        .layer(from_fn_with_state(guard, auth_middleware))
}
