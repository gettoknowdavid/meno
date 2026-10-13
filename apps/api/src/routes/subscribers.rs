//! The subscribers endpoints, with their layers attached.
//!
//! # Why this is merged rather than nested
//!
//! The paths are written out in full (`/subscribers/…`) and mounted at the root with
//! `.merge`, like [`super::broadcasts`], [`super::chat`] and [`super::notes`]. The module
//! serves two shapes — a write against a user (`/subscribers/{id}`) and a page of a user's
//! followers or followees (`/subscribers/{id}/subscribers`) — and one flat table says that
//! more plainly than a nest plus a path list that has to agree with it.
//!
//! # The paths are `master`'s, on purpose
//!
//! `POST /subscribers/{creatorId}` and `DELETE /subscribers/{creatorId}` are what the
//! migration guide documents for the clients' subscribe button, and the four list paths are
//! the ones `master`'s router mounted. The subscriptions *feature* on the mobile side is
//! not built yet, so this file is currently the only written copy of that contract —
//! changing `/subscribers/{id}/subscribers` to something shorter would be a client-visible
//! change to make deliberately, with the client, rather than as a side effect of a
//! server-side refactor.
//!
//! # Layers
//!
//! `LimitPolicy::read()` on the pages, `LimitPolicy::write()` on the follow and unfollow.
//!
//! There is no §4.5 idempotency layer, and its absence is a decision rather than an
//! oversight: that middleware stores a response so a retried *non-idempotent* write does not
//! mint a second resource. This module's writes cannot mint one — the insert is
//! `ON CONFLICT DO NOTHING` against the join table's primary key and the delete removes at
//! most the one row — so a retry reaches the same end state as not retrying, and a Redis
//! round trip per tap would buy nothing. `profile` reaches the same conclusion for its own
//! writes.

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, post};

use crate::infrastructure::redis::Redis;
use crate::middleware::auth::{AuthState, auth_middleware};
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::subscribers::handlers;
use crate::modules::subscribers::state::SubscribersState;

/// Every path this table serves, written out.
///
/// Written out rather than derived from the router, because a list derived from the thing
/// it is meant to check cannot fail. The route-table test in `crate::routes` asserts against
/// it. `{id}` is spelled as an all-zero UUID, as the notes and profile tables do: a literal
/// placeholder that is unmistakably not a real id.
pub const SUBSCRIBERS_PATHS: &[&str] = &[
    "/subscribers/00000000-0000-0000-0000-000000000000",
    "/subscribers/00000000-0000-0000-0000-000000000000/subscribers",
    "/subscribers/00000000-0000-0000-0000-000000000000/subscriptions",
    "/subscribers/me/subscribers",
    "/subscribers/me/subscriptions",
];

/// The subscribers sub-router, before [`super::build_routes`] lifts it to
/// [`SubscribersState`].
pub fn routes(guard: AuthState, redis: Arc<Redis>) -> Router<SubscribersState> {
    let reads = Router::new()
        .route(
            "/subscribers/me/subscribers",
            get(handlers::get_my_subscribers),
        )
        .route(
            "/subscribers/me/subscriptions",
            get(handlers::get_my_subscriptions),
        )
        .route(
            "/subscribers/{id}/subscribers",
            get(handlers::get_user_subscribers),
        )
        .route(
            "/subscribers/{id}/subscriptions",
            get(handlers::get_user_subscriptions),
        )
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::read()),
            rate_limit_middleware,
        ));

    let writes = Router::new()
        .route("/subscribers/{id}", post(handlers::subscribe))
        .route("/subscribers/{id}", delete(handlers::unsubscribe))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::write()),
            rate_limit_middleware,
        ));

    // Auth last, so it is outermost of this sub-router's stack — the required layer order
    // (`middleware/mod.rs`): auth wraps rate limiting, not the other way round, so an
    // unauthenticated flood is a 401 rather than a 429 against a limiter it should never
    // have reached.
    reads
        .merge(writes)
        .layer(from_fn_with_state(guard, auth_middleware))
}
