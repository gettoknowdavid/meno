//! The moderation endpoints, with their layers attached.
//!
//! Merged at the root like [`super::subscribers`], because the paths are written out in
//! full (`/moderation/…`) inside this file's table.
//!
//! # Everything here is behind the auth guard
//!
//! Filing a report and managing your own block list need a caller; the operator queue
//! needs a caller *and* a role, and the role check is the service's — this file can only
//! prove the caller holds a session. That split is deliberate: the guard is a fact about
//! the request, `AuthUser::role` is a fact about the account, and only the service has
//! the second.
//!
//! # Layers
//!
//! `LimitPolicy::read()` on the two reads, `LimitPolicy::write()` on the three writes —
//! the same pair [`super::subscribers`] uses, and for the same reasons. No idempotency
//! layer: blocking twice is one row (`ON CONFLICT DO NOTHING`), unblocking what is not
//! there is a success, and a report is the one write it *would* matter for — but a
//! retried report is a duplicate a human operator can see and dismiss, which is cheaper
//! than a Redis round trip on every filing.

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};

use crate::infrastructure::redis::Redis;
use crate::middleware::auth::{AuthState, auth_middleware};
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::moderation::handlers;
use crate::modules::moderation::state::ModerationState;

/// Every path this table serves, written out.
///
/// Written out rather than derived from the router, because a list derived from the thing
/// it is meant to check cannot fail. [`crate::routes::tests`] asserts against it. `{id}`
/// is spelled as an all-zero UUID, as every other module's table does.
pub const MODERATION_PATHS: &[&str] = &[
    "/moderation/reports",
    "/moderation/reports/00000000-0000-0000-0000-000000000000",
    "/moderation/blocks",
    "/moderation/blocks/00000000-0000-0000-0000-000000000000",
];

/// The moderation sub-router, before [`super::build_routes`] lifts it to
/// [`ModerationState`].
pub fn routes(guard: AuthState, redis: Arc<Redis>) -> Router<ModerationState> {
    let reads = Router::new()
        .route("/moderation/reports", get(handlers::list_reports))
        .route("/moderation/blocks", get(handlers::blocked))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::read()),
            rate_limit_middleware,
        ));

    let writes = Router::new()
        .route("/moderation/reports", post(handlers::create_report))
        .route("/moderation/reports/{id}", post(handlers::resolve_report))
        .route(
            "/moderation/blocks/{id}",
            post(handlers::block).delete(handlers::unblock),
        )
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::write()),
            rate_limit_middleware,
        ));

    // Auth last, so it is outermost of this sub-router's stack — the required layer
    // order (`middleware/mod.rs`): auth wraps rate limiting, so an unauthenticated flood
    // is a 401 rather than a 429 against a limiter it should never have reached.
    reads
        .merge(writes)
        .layer(from_fn_with_state(guard, auth_middleware))
}
