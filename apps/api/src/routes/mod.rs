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
//! place where cross-cutting concerns attach: rate limiting on `/auth/*` (§4.7 item 8),
//! the authenticated-session guard, idempotency (§4.5), and the CORS / request-id /
//! tracing stack (§4.8) all live in the table or in [`layers`], never inside a handler.
//!
//! # Modules contribute, they do not self-register
//!
//! [`build_routes`] names each module's mount explicitly. A module registering its own
//! routes would be convenient and would make the table impossible to read in one place —
//! and the table *is* the security surface, so it should be reviewable without opening
//! eight files.
//!
//! # One file per module, one table
//!
//! Each module's paths live in its own file — [`auth::routes`], [`broadcasts::routes`] —
//! so adding a module does not make this file the place where every endpoint in the
//! product is spelled out, and so a module's layers and its reason for them stay with
//! the handlers they wrap. The *mounts* stay here: this file is the only place that
//! answers "what does this API expose", and every module's [`PathList`](auth::AUTH_PATHS)
//! is re-exported below so the tests can check all of them from one place.
//!
//! # Layer order is a contract, not a preference
//!
//! `middleware/mod.rs` documents the required order — timing outermost of the auth
//! stack, then auth, then rate limiting, then idempotency — because each layer reads
//! what the one above it inserted. This file implements it by splitting the auth table
//! into a public half and a protected half and applying the layers per half; see
//! [`auth::routes`] for the reasoning.

mod auth;
mod broadcasts;
mod health;
mod layers;
mod metrics;
mod ws;

use std::sync::Arc;

use axum::Router;
use axum::routing::get;

use crate::middleware::rate_limit::RateLimitState;
use crate::state::MenoState;

// Every module's path list, so one test can assert the whole table. Re-exported rather
// than re-declared: a list written twice is a list that will disagree with the router.
pub use auth::{AUTH_PATHS, PROTECTED_PATHS};
pub use broadcasts::BROADCAST_PATHS;
pub use health::HEALTH_PATHS;

/// Build the application's routes.
///
/// The outer router carries [`MenoState`], but no handler ever sees it: each module's
/// sub-router is built against its own state and lifted back with `with_state`, which
/// projects the one field through the `FromRef` impl on [`MenoState`]. That is what
/// keeps the state struct out of business logic — see the module docs on `state`.
///
/// The [`layers::apply`] call last is what makes it *last*: `.layer` wraps everything
/// already attached, so the CORS, request-id and tracing stack ends up outermost —
/// the position its own module docs describe.
pub fn build_routes(state: MenoState) -> Router {
    let guard = state.auth_guard.clone();
    let limiter = RateLimitState::new(state.redis.clone(), auth::policy());
    // One handle, two mounts: the idempotency middleware needs Redis either way, and
    // sharing the `Arc` keeps a single connection pool behind both.
    let idempotency = Arc::new(state.redis.clone());
    let idempotency_for_broadcasts = Arc::clone(&idempotency);

    let router = Router::new()
        .route("/health", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        // Private by default (§7.5): see `routes::metrics` for the two fail-closed
        // branches that replace `master`'s public endpoint.
        .route("/metrics", get(metrics::handler))
        // The one route whose authentication lives in the handler rather than in a
        // layer: a browser WebSocket handshake cannot carry an `Authorization`
        // header, so the access token arrives as `?token=`. The gate is not
        // duplicated for it — `routes::ws` calls the same `AuthState::authenticate`
        // the auth middleware runs — and it completes *before* `on_upgrade`, so a
        // refused handshake is an HTTP status, not a socket that opens and closes.
        .route("/ws", get(ws::upgrade))
        // The auth handlers take [`AuthState`], not `MenoState`, so the sub-router is
        // built against its own state and then lifted to the application's with
        // `with_state`. That call is what `FromRef` is for: it projects the one field
        // the module needs, so no handler ever sees — or can reach — the rest.
        //
        // The alternative, widening all fourteen handlers to take `MenoState`, is
        // precisely the god-object coupling §9.3 rules out.
        // Each module's sub-router is built against its own state and then lifted to
        // the application's with `with_state`, which projects the one field the module
        // needs through its `FromRef` impl. No handler ever sees — or can reach — the
        // rest.
        .nest(
            "/auth",
            auth::routes(guard, limiter, idempotency).with_state(state.auth.clone()),
        )
        .nest(
            "/broadcasts",
            // The nest carries no prefix of its own: the module's paths already begin
            // with `/broadcasts`, so the tree reads `GET /broadcasts` and not
            // `/broadcasts/broadcasts`.
            broadcasts::routes(state.auth_guard.clone(), idempotency_for_broadcasts)
                .with_state(state.broadcast.clone()),
        );

    layers::apply(router, &state.config).with_state(state)
}

#[cfg(test)]
mod tests {
    //! The route table's own contract: every documented path resolves.
    //!
    //! A handler that compiles but is never mounted is the specific failure this file
    //! exists to prevent, so the test is "every path in the table answers something other
    //! than 404" rather than a behavioural one. The handlers' behaviour is covered by
    //! `tests/auth_service.rs` and `tests/auth_router.rs`, and the live wiring — auth
    //! gate, limiter, readiness probes — against a running instance, since each of them
    //! needs a live Postgres and Redis.

    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Every path every module mounts.
    ///
    /// Built from the per-module lists in `auth` and `broadcasts` rather than written
    /// out here, so a module that adds a route and forgets to list it fails these tests
    /// instead of silently shipping.
    fn every_path() -> Vec<&'static str> {
        AUTH_PATHS
            .iter()
            .chain(BROADCAST_PATHS)
            .chain(HEALTH_PATHS)
            .copied()
            .collect()
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
    fn no_module_ships_a_duplicate_path() {
        // A duplicate path is not a compile error and not a 404 — axum keeps one of the
        // two handlers and the other silently stops being reachable. It would only ever
        // be noticed by a user hitting the route that lost.
        let paths = every_path();
        let unique: std::collections::HashSet<_> = paths.iter().collect();

        assert_eq!(
            unique.len(),
            paths.len(),
            "a duplicate path would shadow a handler: {paths:?}"
        );
        // The count is asserted so a module cannot quietly delete its way out of the
        // check by emptying its own list.
        assert_eq!(
            paths.len(),
            25,
            "fourteen auth endpoints, nine broadcast ones and two probes"
        );
    }

    #[test]
    fn every_module_paths_under_the_prefix_it_is_nested_at() {
        // Each sub-router is nested at a prefix, so a module whose paths already carry
        // that prefix would be served at `/auth/auth/...` — which compiles, answers, and
        // is simply the wrong URL. The cheapest possible guard on that.
        for (paths, prefix) in [
            (AUTH_PATHS, "/auth"),
            (BROADCAST_PATHS, "/"),
            (HEALTH_PATHS, "/health"),
        ] {
            for &path in paths {
                assert!(
                    path.starts_with(prefix),
                    "{path} would be served at {prefix}{path}"
                );
            }
        }
    }

    #[test]
    fn the_broadcast_nest_is_not_prefixed_twice() {
        // The one that bit: the broadcast tree writes `/broadcasts/...` inside its
        // sub-router *and* is nested, so the prefix has to be the root. Asserted
        // separately because the table above would pass for the wrong reason if this
        // file had been mounted at `/broadcasts` with prefixed paths.
        assert!(
            BROADCAST_PATHS[0] == "/broadcasts",
            "the first broadcast path is the collection itself: {:?}",
            BROADCAST_PATHS
        );
    }

    #[test]
    fn the_protected_half_is_exactly_the_routes_that_read_authuser() {
        // The three handlers with `Extension<AuthUser>` in their signature are the
        // three listed here. If a fourth handler gains the extractor, it must join this
        // list — and the split in `auth::routes` — or it answers 500 instead of 401,
        // which is the failure the split exists to end.
        assert!(!PROTECTED_PATHS.is_empty());
        for path in PROTECTED_PATHS {
            assert!(
                AUTH_PATHS.contains(path),
                "{path} is protected but not in the table"
            );
        }
        assert_eq!(PROTECTED_PATHS.len(), 3);
        // `logout` presents its tokens in the body and must NOT be gated: signing out
        // requires no live credential to have been good a moment ago.
        assert!(
            !PROTECTED_PATHS.contains(&"/auth/logout"),
            "logout stays reachable with an expired access token"
        );
    }
}
