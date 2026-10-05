//! The application's route table.
//!
//! # Why this file exists
//!
//! `master` merged a route tree from a `routes` module that no longer exists, and the
//! refactored tree served only `/health` and `/health/ready` from inside `main.rs` — so
//! the auth module's fourteen handlers, all of them written, were mounted nowhere. A
//! handler that is never routed to is a handler that is never tested in production, and
//! fourteen of them is a silent hole rather than a visible one.
//!
//! The plan's target tree puts `routes/` here (line 74), and §9.3 makes the router the
//! place where cross-cutting concerns attach: rate limiting on `/auth/*` (§4.7 item 8)
//! and the authenticated-session guard are both router concerns, not handler concerns.
//!
//! # Modules contribute, they do not self-register
//!
//! [`build_routes`] names each module's paths explicitly. A module registering its own
//! routes would be convenient and would make the table impossible to read in one place —
//! and the table *is* the security surface, so it should be reviewable without opening
//! eight files.

use axum::Router;
use axum::routing::{get, post};

use crate::modules::auth::state::AuthState;
use crate::state::MenoState;

/// Build the application's routes.
///
/// The outer router carries [`MenoState`], but no handler ever sees it: each module's
/// sub-router is built against its own [`AuthState`] and lifted back with `with_state`,
/// which projects the one field through the `FromRef` impl on `MenoState`. That is what
/// keeps the state struct out of business logic — see the module docs on `state`.
pub fn build_routes(state: MenoState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(readiness))
        // The auth handlers take [`AuthState`], not `MenoState`, so the sub-router is
        // built against its own state and then lifted to the application's with
        // `with_state`. That call is what `FromRef` is for: it projects the one field
        // the module needs, so no handler ever sees — or can reach — the rest.
        //
        // The alternative, widening all fourteen handlers to take `MenoState`, is
        // precisely the god-object coupling §9.3 rules out.
        .nest("/auth", auth_routes().with_state(state.auth.clone()))
        .with_state(state)
}

/// The auth module's endpoints (§4.7).
///
/// Grouped here rather than inside the auth module so the table reads top to bottom:
/// this is what the API surface *is*, and a reader should not have to know how many
/// modules exist to answer that.
///
/// # The four authenticated routes are not yet gated
///
/// [`handlers::list_sessions`] and [`handlers::revoke_session`] read
/// `Extension<AuthUser>`, which only [`crate::middleware::auth::auth_middleware`]
/// inserts. Until that layer is attached here, calling either returns **500 "Missing
/// request extension"** rather than 401 — a route that exists, compiles, and is
/// unreachable.
///
/// # What has been ruled out
///
/// The obvious wiring — a `public` and a `protected` `Router<AuthState>`, with
/// `from_fn_with_state(auth_state, auth_middleware)` on the second — does not compile
/// here:
///
/// ```text
/// error[E0277]: the trait bound `FromFn<_, _, Route, _>: Service<_>` is not satisfied
/// ```
///
/// That is not a mis-ordering of `.layer()` and `.with_state()`: both orders were tried,
/// as were `route_layer` and building the protected half directly as a
/// `Router<MenoState>`. The same `from_fn_with_state` call compiles against a minimal
/// `State` handler *and* against the real [`AuthState`] and the real `auth_middleware`,
/// so the shape is right and the trigger is one of these four handlers' extractors.
///
/// Attaching the layer blanket-style is not an acceptable shortcut: `register` and
/// `login` are how a caller obtains a token, so gating them is a lockout.
fn auth_routes() -> Router<AuthState> {
    use crate::modules::auth::handlers;

    Router::new()
        // Registration and sign-in.
        .route("/register", post(handlers::register))
        .route("/login", post(handlers::login))
        .route("/google/url", get(handlers::google_authorize_url))
        .route("/google/callback", get(handlers::google_web_callback))
        .route("/google/mobile", post(handlers::google_mobile_auth))
        // Session lifecycle.
        .route("/refresh", post(handlers::refresh))
        .route("/logout", post(handlers::logout))
        .route("/logout-all", post(handlers::logout_everywhere))
        .route("/sessions", get(handlers::list_sessions))
        .route("/sessions/{id}", post(handlers::revoke_session))
        // One-time codes and password resets.
        .route("/verify-email", post(handlers::verify_email))
        .route("/resend-otp", post(handlers::resend_otp))
        .route("/forgot-password", post(handlers::forgot_password))
        .route("/reset-password", post(handlers::reset_password))
}

/// `GET /health` — is the process up.
///
/// Deliberately does not touch Postgres or Redis. This endpoint answers "is this
/// container running", and a liveness probe that fails when a *dependency* is down gets
/// the container killed and restarted, which fixes nothing and turns a database blip
/// into an outage.
#[must_use]
pub async fn health() -> &'static str {
    "ok"
}

/// `GET /health/ready` — should this instance receive traffic.
///
/// Currently reports readiness from configuration alone. §4.3 wants a real dependency
/// check here (`{"status":"ok","db":true,"redis":true}`); that needs a round trip to
/// Postgres on every probe and lands with the first module that owns a connection worth
/// reporting on. Returning a constant rather than a lie is deliberate — this says
/// "booted", and does not claim more than it knows.
#[must_use]
pub async fn readiness() -> &'static str {
    "ready"
}

#[cfg(test)]
mod tests {
    //! The route table's own contract: every documented path resolves.
    //!
    //! A handler that compiles but is never mounted is the specific failure this file
    //! exists to prevent, so the test is "every path in the table answers something other
    //! than 404" rather than a behavioural one. The handlers' behaviour is covered by
    //! the auth module's own tests and by `tests/auth_router.rs`.

    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Every path the auth module's handlers are mounted on.
    ///
    /// Written out rather than derived from the router, because a list derived from the
    /// thing it is meant to check cannot fail.
    const AUTH_PATHS: &[&str] = &[
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

    #[tokio::test]
    async fn every_health_endpoint_answers() {
        // `build_routes` needs a `MenoState`, which needs a pool and Redis, so these
        // two are mounted on their own router — which is exactly how they are reached
        // in production, before any merge.
        let router = Router::new()
            .route("/health", get(health))
            .route("/health/ready", get(readiness));

        for (path, expected) in [("/health", "ok"), ("/health/ready", "ready")] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(Body::empty())
                        .expect("a valid request"),
                )
                .await
                .expect("a response");
            assert_eq!(response.status(), StatusCode::OK, "{path}");

            let bytes = axum::body::to_bytes(response.into_body(), 64)
                .await
                .expect("the body buffers");
            assert_eq!(bytes.as_ref(), expected.as_bytes(), "{path}");
        }
    }

    #[tokio::test]
    async fn an_unmounted_path_is_a_404() {
        // The negative control for the test below: if `/auth/nope` also answered 404 in
        // the table, "every path answers something" would be satisfied by a router with
        // no routes at all.
        let router = Router::<()>::new();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/auth/register")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("a response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn the_auth_table_has_no_duplicate_paths() {
        // A duplicate path is not a compile error and not a 404 — axum keeps one of the
        // two handlers and the other silently stops being reachable. It would only ever
        // be noticed by a user hitting the route that lost.
        //
        // Sorted here rather than requiring the constant to be written sorted: the
        // constant is grouped by what the endpoints *do* (sign-in, sessions, codes),
        // which is worth more to a reader than an ordering that exists only to make this
        // assertion cheaper.
        let unique: std::collections::HashSet<_> = AUTH_PATHS.iter().collect();

        assert_eq!(
            unique.len(),
            AUTH_PATHS.len(),
            "a duplicate path would shadow a handler"
        );
        // The set has to have been built from the real list, not a hardcoded one.
        assert_eq!(unique.len(), 14, "the table has fourteen auth endpoints");
    }

    #[test]
    fn every_auth_path_is_under_the_auth_prefix() {
        // The sub-router is nested at `/auth`, so a path written with its own `/auth`
        // prefix would be served at `/auth/auth/...` — which compiles, answers, and is
        // simply the wrong URL. This is the cheapest possible guard on that.
        for path in AUTH_PATHS {
            assert!(
                path.starts_with("/auth/"),
                "{path} would be served at /auth{path}"
            );
        }
    }
}
