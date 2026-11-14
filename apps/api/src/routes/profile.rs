//! The profile module's endpoints, with their layers attached.
//!
//! Extracted alongside `routes/auth.rs` and `routes/broadcasts.rs` so the route table
//! can grow a module at a time without `mod.rs` becoming the file nobody re-reads.
//! [`crate::routes::build_routes`] still names every mount.
//!
//! # Paths are relative here; the nest carries the prefix
//!
//! This module spells its routes `/`, `/me`, `/{id}` and the mount spells `/users` —
//! the `auth` pattern (`route("/register")` + `nest("/auth")`), not `broadcast`'s
//! absolute-paths-merged-at-root. One convention for the new modules, and the nest
//! prefix means `PROFILE_PATHS` and the router cannot disagree about where a path
//! lives without `routes::tests` noticing.
//!
//! # Why there is no idempotency layer
//!
//! §4.5's middleware exists so a retried *non-idempotent* write does not mint a
//! second resource. `PATCH /users/me` and `PATCH /settings` are last-write-wins by
//! definition, `DELETE /settings/push-token` is an unset, and `POST /users/me/avatar`
//! derives its key from the user id and the sniffed extension — replaying any of them
//! lands on the same end state as not replaying. `broadcast`'s `go_live` mints a
//! room; nothing here mints anything.

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, patch, post};

use crate::infrastructure::redis::Redis;
use crate::middleware::auth::{AuthState, auth_middleware};
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::profile::handlers;
use crate::modules::profile::state::ProfileState;
use crate::modules::profile::storage::MAX_AVATAR_BYTES;

/// Every path this nest serves, written out.
///
/// Written out rather than derived from the router, because a list derived from the
/// thing it is meant to check cannot fail. [`crate::routes::tests`] asserts against it.
/// The `{id}` path uses an all-zero UUID for the same reason `broadcast`'s list does:
/// a literal placeholder that is unmistakably not a real id.
pub const PROFILE_PATHS: &[&str] = &[
    "/users",
    "/users/me",
    "/users/me/avatar",
    "/users/00000000-0000-0000-0000-000000000000",
];

/// Body limit for the avatar route: the file itself plus a megabyte of multipart
/// framing (boundary, headers, the discarded text fields). The *content* limit stays
/// [`MAX_AVATAR_BYTES`] — this one only decides when axum stops reading a body that
/// cannot possibly be a legal upload, which is cheaper than a 422 after the fact.
const AVATAR_BODY_LIMIT: usize = MAX_AVATAR_BYTES + 1024 * 1024;

/// The profile sub-router, before it is lifted to [`ProfileState`] by the caller.
pub fn routes(guard: AuthState, redis: Arc<Redis>) -> Router<ProfileState> {
    // Reads: profile lookup, `/users/me`, search. The generic API policy — none of
    // this hashes a credential or mints a room, and a profile screen that refetches
    // on focus is normal use.
    let reads = Router::new()
        .route("/", get(handlers::search))
        .route("/me", get(handlers::get_me))
        .route("/{id}", get(handlers::get_profile))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::read()),
            rate_limit_middleware,
        ));

    // The self-update: same policy as the reads' sibling writes elsewhere —
    // `LimitPolicy::write()`.
    let writes = Router::new()
        .route("/me", patch(handlers::update_me))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::write()),
            rate_limit_middleware,
        ));

    // The upload sits apart for one reason: the body limit. A PATCH carries a few
    // hundred bytes and should keep axum's default ceiling; an upload carries five
    // megabytes and must not be cut off by it. Applying the limit to this router
    // alone is what keeps that difference a routing decision rather than a
    // handler-sized special case. Rate-limited under `write()` like every other
    // mutation.
    let uploads = Router::new()
        .route("/me/avatar", post(handlers::upload_avatar))
        .layer(DefaultBodyLimit::max(AVATAR_BODY_LIMIT))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::write()),
            rate_limit_middleware,
        ));

    // Auth last, so it is outermost of this sub-router's stack — the required layer
    // order (`middleware/mod.rs`): auth wraps rate limiting, not the other way round,
    // so an unauthenticated flood is a 401 rather than a 429 against a limiter it
    // should never have reached.
    reads
        .merge(writes)
        .merge(uploads)
        .layer(from_fn_with_state(guard, auth_middleware))
}
