//! The notifications endpoints, with their layers attached.
//!
//! One collection, five paths. Same shape as `routes/notes.rs`: the paths are written
//! out in full and mounted at the root with `.merge`, because `nest("/")` is refused
//! by axum 0.8 and a nest at `/notifications` would strip the prefix its sub-router
//! already carries.
//!
//! # Layers
//!
//! `LimitPolicy::read()` on the two reads, `LimitPolicy::write()` on the three
//! mutations. No idempotency layer: `PATCH …/read` and `PATCH …/read-all` are
//! postconditions rather than resource creations, and `DELETE` is idempotent by
//! definition — see `routes/profile.rs`'s note on the same decision.

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, patch};

use crate::infrastructure::redis::Redis;
use crate::middleware::auth::{AuthState, auth_middleware};
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::notifications::handlers;
use crate::modules::notifications::state::NotificationsState;

/// Every path this table serves, written out.
///
/// Written out rather than derived from the router, because a list derived from the
/// thing it is meant to check cannot fail. [`crate::routes::tests`] asserts against it.
pub const NOTIFICATIONS_PATHS: &[&str] = &[
    "/notifications",
    "/notifications/unread-count",
    "/notifications/read-all",
    "/notifications/00000000-0000-0000-0000-000000000000",
    "/notifications/00000000-0000-0000-0000-000000000000/read",
];

/// The notifications sub-router, before [`super::build_routes`] lifts it to
/// [`NotificationsState`].
pub fn routes(guard: AuthState, redis: Arc<Redis>) -> Router<NotificationsState> {
    let reads = Router::new()
        .route("/notifications", get(handlers::get_notifications))
        .route(
            "/notifications/unread-count",
            get(handlers::get_unread_count),
        )
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::read()),
            rate_limit_middleware,
        ));

    let writes = Router::new()
        .route("/notifications/read-all", patch(handlers::mark_all_read))
        .route(
            "/notifications/{id}/read",
            patch(handlers::mark_read),
        )
        .route(
            "/notifications/{id}",
            delete(handlers::delete_notification),
        )
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::write()),
            rate_limit_middleware,
        ));

    reads
        .merge(writes)
        .layer(from_fn_with_state(guard, auth_middleware))
}
