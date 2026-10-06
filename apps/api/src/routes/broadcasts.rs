//! The broadcast module's endpoints, with their layers attached.
//!
//! Extracted alongside [`super::auth`] so the route table can grow a module at a time
//! without `mod.rs` becoming the file nobody re-reads. [`super::build_routes`] still
//! names every mount, so the API's surface stays answerable from one place.
//!
//! # Everything here is behind the auth guard
//!
//! Not a decision this file makes for itself: `crate::middleware::auth::auth_middleware`
//! is applied to the *whole* nest below, and every handler reads
//! `Extension<AuthUser>`. There is no public half, because there is nothing to be
//! public about — a broadcast catalogue without a caller has no viewer, no role and no
//! participant count, and answering it anonymously would mean inventing those.
//!
//! # Rate limiting, and which policy
//!
//! [`LimitPolicy::auth`] is *not* the right policy for this surface: it is five requests
//! a minute, chosen because every one of them hashes a password. A participant joining
//! and leaving is not credential work, so the generic API policy applies — and it is
//! what makes the media-token endpoint survivable when a room fills up and every client
//! reconnects at once.
//!
//! # Idempotency
//!
//! §4.5's middleware is applied to the mutating endpoints only. `POST …/live` is the
//! one that must be: a retried go-live must not mint a second room, and the service
//! answers the second attempt with 409 anyway — the layer makes the *retry* cheap
//! instead of merely correct.

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, patch, post};

use crate::infrastructure::redis::Redis;
use crate::middleware::auth::auth_middleware;
use crate::middleware::idempotency::idempotency_middleware;
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::broadcast::handlers;
use crate::modules::broadcast::state::BroadcastState;

/// Every path this nest serves, written out.
///
/// Written out rather than derived from the router, because a list derived from the
/// thing it is meant to check cannot fail. [`crate::routes::tests`] asserts against it.
pub const BROADCAST_PATHS: &[&str] = &[
    "/broadcasts",
    "/broadcasts/00000000-0000-0000-0000-000000000000",
    "/broadcasts/00000000-0000-0000-0000-000000000000/live",
    "/broadcasts/00000000-0000-0000-0000-000000000000/end",
    "/broadcasts/00000000-0000-0000-0000-000000000000/join",
    "/broadcasts/00000000-0000-0000-0000-000000000000/leave",
    "/broadcasts/00000000-0000-0000-0000-000000000000/cohosts",
    "/broadcasts/00000000-0000-0000-0000-000000000000/cohosts/00000000-0000-0000-0000-000000000000",
    "/broadcasts/00000000-0000-0000-0000-000000000000/participants",
];

/// The broadcast sub-router, before it is lifted to [`BroadcastState`] by the caller.
pub fn routes(
    guard: crate::middleware::auth::AuthState,
    redis: Arc<Redis>,
) -> Router<BroadcastState> {
    // Reads: no idempotency layer, because there is nothing to replay -- and putting
    // one on a GET would cache a response nobody asked to cache.
    let reads = Router::new()
        .route("/broadcasts", get(handlers::list))
        .route("/broadcasts/{id}", get(handlers::get))
        .route("/broadcasts/{id}/participants", get(handlers::participants))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::read()),
            rate_limit_middleware,
        ));

    // `join` sits apart from the other writes because it is the endpoint a room full
    // of reconnecting clients hits at once, and `LimitPolicy::broadcast_join` is the
    // policy sized for that burst.
    let join = Router::new()
        .route("/broadcasts/{id}/join", post(handlers::join))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::broadcast_join()),
            rate_limit_middleware,
        ))
        .layer(from_fn_with_state(
            Arc::clone(&redis),
            idempotency_middleware,
        ));

    // The rest of the writes: idempotency innermost, so a retried request is answered
    // from the stored response rather than re-entering the service.
    let writes = Router::new()
        .route("/broadcasts", post(handlers::create))
        .route("/broadcasts/{id}", patch(handlers::update))
        .route("/broadcasts/{id}", delete(handlers::delete))
        .route("/broadcasts/{id}/live", post(handlers::go_live))
        .route("/broadcasts/{id}/end", post(handlers::end))
        .route("/broadcasts/{id}/leave", post(handlers::leave))
        .route("/broadcasts/{id}/cohosts", post(handlers::add_cohost))
        .route(
            "/broadcasts/{id}/cohosts/{cohostId}",
            delete(handlers::remove_cohost),
        )
        .layer(from_fn_with_state(
            Arc::clone(&redis),
            idempotency_middleware,
        ))
        .layer(from_fn_with_state(
            // `LimitPolicy::auth` is five requests a minute because every request it
            // guards hashes a password. Applying it here would have a room full of
            // reconnecting clients lock each other out.
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::write()),
            rate_limit_middleware,
        ));

    reads
        .merge(join)
        .merge(writes)
        .layer(from_fn_with_state(guard, auth_middleware))
}
