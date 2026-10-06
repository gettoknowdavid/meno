//! The auth module's endpoints (§4.7), with their layers attached.
//!
//! Extracted from [`super::mod`] so the route table can grow a module at a time
//! without `mod.rs` becoming the file nobody re-reads. The table is still assembled in
//! one place — [`super::build_routes`] names every mount — because the table *is* the
//! security surface, and a reader should be able to answer "what does this API expose"
//! without opening eight files.
//!
//! # Why this nest is two routers
//!
//! Three routes read `Extension<AuthUser>` and therefore need
//! [`crate::middleware::auth::auth_middleware`] in front of them:
//! [`list_sessions`](handlers::list_sessions),
//! [`revoke_session`](handlers::revoke_session) and
//! [`logout_everywhere`](handlers::logout_everywhere). Gating the whole nest would be a
//! lockout — `register` and `login` are how a caller obtains a token — and gating
//! nothing is what `master` did, which left those three answering **500 "Missing request
//! extension"** rather than 401: a route that exists, compiles, and is unreachable.
//!
//! So the nest is two routers, each with its own layer stack in the order
//! [`crate::middleware`] requires (outermost last):
//!
//! | half | outermost → innermost |
//! | --- | --- |
//! | public (11 routes) | rate limit → idempotency |
//! | protected (3 routes) | auth → rate limit → idempotency |
//!
//! The protected order is the load-bearing one: the guard runs first so it can insert
//! [`AuthUser`](crate::middleware::auth::AuthUser), the limiter runs second so §4.4's
//! "identify by authenticated user id" can read it, and idempotency runs last so §4.5's
//! caller scoping is neither anonymous nor spoofable.
//!
//! # What an earlier attempt got wrong
//!
//! A previous revision documented that `from_fn_with_state(auth_state, auth_middleware)`
//! did not compile against these handlers, with `FromFn<_, _, Route, _>: Service<_>`
//! unsatisfied. The layer has to be applied to the `Router` — which owns the state
//! plumbing — not to a single route's method router; `Router::layer` is what makes
//! `FromFn` a service here.

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};

use crate::infrastructure::redis::Redis;
use crate::middleware::auth::auth_middleware;
use crate::middleware::idempotency::idempotency_middleware;
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::auth::handlers;
use crate::modules::auth::state::AuthState;

/// Every path this nest serves, written out.
///
/// Written out rather than derived from the router, because a list derived from the
/// thing it is meant to check cannot fail. [`crate::routes::tests`] asserts against it.
pub const AUTH_PATHS: &[&str] = &[
    "/auth/register",
    "/auth/login",
    "/auth/google/url",
    "/auth/google/callback",
    "/auth/google/mobile",
    "/auth/refresh",
    "/auth/logout",
    "/auth/logout-all",
    "/auth/sessions",
    "/auth/sessions/00000000-0000-0000-0000-000000000000",
    "/auth/verify-email",
    "/auth/resend-otp",
    "/auth/forgot-password",
    "/auth/reset-password",
];

/// The routes that read `Extension<AuthUser>` and so must sit behind the guard.
///
/// A separate list from [`AUTH_PATHS`] because the two answer different questions —
/// "is it mounted" versus "is it gated" — and a guard that quietly covers the wrong half
/// is the failure worth pinning.
pub const PROTECTED_PATHS: &[&str] = &[
    "/auth/sessions",
    "/auth/sessions/00000000-0000-0000-0000-000000000000",
    "/auth/logout-all",
];

/// The auth sub-router, before it is lifted to [`AuthState`] by the caller.
///
/// `Router<AuthState>` rather than a plain `Router`, because every handler here takes
/// `State<AuthState>` — the module's own state, never [`crate::state::MenoState`].
pub fn routes(
    guard: crate::middleware::auth::AuthState,
    limiter: RateLimitState,
    idempotency: Arc<Redis>,
) -> Router<AuthState> {
    let public = Router::new()
        // Registration and sign-in.
        .route("/register", post(handlers::register))
        .route("/login", post(handlers::login))
        .route("/google/url", get(handlers::google_authorize_url))
        .route("/google/callback", get(handlers::google_web_callback))
        .route("/google/mobile", post(handlers::google_mobile_auth))
        // Session lifecycle that presents its tokens in the body, so it needs no
        // request-extension identity to work.
        .route("/refresh", post(handlers::refresh))
        .route("/logout", post(handlers::logout))
        // One-time codes and password resets.
        .route("/verify-email", post(handlers::verify_email))
        .route("/resend-otp", post(handlers::resend_otp))
        .route("/forgot-password", post(handlers::forgot_password))
        .route("/reset-password", post(handlers::reset_password))
        // Innermost first: applied before the limiter so the limiter is outermost of
        // this half's stack.
        .layer(from_fn_with_state(
            Arc::clone(&idempotency),
            idempotency_middleware,
        ))
        .layer(from_fn_with_state(limiter.clone(), rate_limit_middleware));

    let protected = Router::new()
        .route("/sessions", get(handlers::list_sessions))
        .route("/sessions/{id}", post(handlers::revoke_session))
        .route("/logout-all", post(handlers::logout_everywhere))
        // Reverse of the required outermost-first order: auth last, because the last
        // layer applied is the one a request meets first.
        .layer(from_fn_with_state(idempotency, idempotency_middleware))
        .layer(from_fn_with_state(limiter, rate_limit_middleware))
        .layer(from_fn_with_state(guard, auth_middleware));

    public.merge(protected)
}

/// The rate-limit policy for the auth surface: 5/min, **fail closed** (§4.4).
///
/// A login endpoint whose limiter is down must not become an unthrottled login
/// endpoint, so this is the one policy that refuses rather than degrades.
#[must_use]
pub fn policy() -> LimitPolicy {
    LimitPolicy::auth()
}
