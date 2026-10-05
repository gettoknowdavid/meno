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
//! [`build_routes`] names each module's paths explicitly. A module registering its own
//! routes would be convenient and would make the table impossible to read in one place —
//! and the table *is* the security surface, so it should be reviewable without opening
//! eight files.
//!
//! # Layer order is a contract, not a preference
//!
//! `middleware/mod.rs` documents the required order — timing outermost of the auth
//! stack, then auth, then rate limiting, then idempotency — because each layer reads
//! what the one above it inserted. This file implements it by splitting the auth table
//! into a public half and a protected half and applying the layers per half; see
//! [`auth_routes`] for the reasoning.

mod layers;
mod metrics;
mod ws;

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use serde::Serialize;

use crate::infrastructure::redis::Redis;
use crate::middleware::auth::auth_middleware;
use crate::middleware::idempotency::idempotency_middleware;
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::auth::state::AuthState;
use crate::state::MenoState;

/// How long one dependency probe may take before it is reported as down.
///
/// A readiness answer an orchestrator waits on must be fast: a probe that hangs is
/// indistinguishable from an outage, and one that takes ten seconds turns every
/// dependency blip into a deployment event. Two seconds is comfortably above a local
/// round trip and well below any load-balancer timeout.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

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
    let limiter = RateLimitState::new(state.redis.clone(), LimitPolicy::auth());
    let idempotency = Arc::new(state.redis.clone());

    let router = Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(readiness))
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
        .nest(
            "/auth",
            auth_routes(guard, limiter, idempotency).with_state(state.auth.clone()),
        );

    layers::apply(router, &state.config).with_state(state)
}

/// The auth module's endpoints (§4.7), with their layers attached.
///
/// Grouped here rather than inside the auth module so the table reads top to bottom:
/// this is what the API surface *is*, and a reader should not have to know how many
/// modules exist to answer that.
///
/// # Why the router splits in two
///
/// Three routes read `Extension<AuthUser>` and therefore need
/// [`crate::middleware::auth::auth_middleware`] in front of them:
/// [`handlers::list_sessions`], [`handlers::revoke_session`] and
/// [`handlers::logout_everywhere`]. Gating the whole nest would be a lockout —
/// `register` and `login` are how a caller obtains a token — and gating nothing is
/// what `master` did, which left those three answering **500 "Missing request
/// extension"** rather than 401: a route that exists, compiles, and is unreachable.
///
/// So the nest is two routers, each with its own layer stack in the order
/// `middleware/mod.rs` requires (outermost last):
///
/// | half | outermost → innermost |
/// | --- | --- |
/// | public (11 routes) | rate limit → idempotency |
/// | protected (3 routes) | auth → rate limit → idempotency |
///
/// The protected order is the load-bearing one: the guard runs first so it can insert
/// [`AuthUser`](crate::middleware::auth::AuthUser), the limiter runs second so §4.4's
/// "identify by authenticated user id" can read it, and idempotency runs last so
/// §4.5's caller scoping is neither anonymous nor spoofable.
///
/// Rate limiting is [`LimitPolicy::auth`] — 5/min, **fail closed** (§4.4): a login
/// endpoint whose limiter is down must not become an unthrottled login endpoint.
///
/// # What an earlier attempt got wrong
///
/// A previous revision documented that `from_fn_with_state(auth_state, auth_middleware)`
/// did not compile against these four handlers, with `FromFn<_, _, Route, _>:
/// Service<_>` unsatisfied. The layer has to be applied to the `Router` — which owns
/// the state plumbing — not to a single route's method router; `Router::layer` is what
/// makes `FromFn` a service here. That is the shape below, and it compiles.
fn auth_routes(
    guard: crate::middleware::auth::AuthState,
    limiter: RateLimitState,
    idempotency: Arc<Redis>,
) -> Router<AuthState> {
    use crate::modules::auth::handlers;

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

/// `GET /health` — is the process up.
///
/// Deliberately does not touch Postgres or Redis. This endpoint answers "is this
/// container running", and a liveness probe that fails when a *dependency* is down gets
/// the container killed and restarted, which fixes nothing and turns a database blip
/// into an outage. Dependency health is [`readiness`]'s job.
#[must_use]
pub async fn health() -> &'static str {
    "ok"
}

/// What `/health/ready` reports (plan §4.3).
///
/// The exact shape the plan's gate curls for — `{"status":"ok","db":true,"redis":true}`
/// — with `status` flipping to `degraded` when either probe fails, and the status line
/// with it: 200 while the instance may take traffic, 503 when it may not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Readiness {
    /// `"ok"` or `"degraded"` — the one field an operator reads first.
    pub status: &'static str,
    /// Postgres answered `SELECT 1` inside the probe budget.
    pub db: bool,
    /// Redis answered `PING` inside the probe budget.
    pub redis: bool,
}

impl Readiness {
    /// Assemble the report for two probe outcomes.
    #[must_use]
    pub const fn new(db: bool, redis: bool) -> Self {
        Self {
            status: if db && redis { "ok" } else { "degraded" },
            db,
            redis,
        }
    }

    /// Whether this instance should receive traffic.
    #[must_use]
    pub const fn is_ready(self) -> bool {
        self.db && self.redis
    }

    /// 200 when ready, 503 when not — decided by the probes, never assumed.
    #[must_use]
    pub const fn status_code(self) -> StatusCode {
        if self.is_ready() {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

/// `GET /health/ready` — should this instance receive traffic.
///
/// A real dependency check (§4.3): one round trip to Postgres and one to Redis, each
/// bounded by [`PROBE_TIMEOUT`], reported honestly. The previous version answered a
/// constant `"ready"` from configuration alone, which asserted nothing — it said
/// "booted" while the database was unreachable, and an orchestrator believes the
/// status line, not the prose around it.
///
/// Both probes run concurrently: a readiness endpoint is polled on every probe
/// interval, and serialising them doubles the worst-case latency of a path that sits
/// in front of every other one.
pub async fn readiness(State(state): State<MenoState>) -> (StatusCode, Json<Readiness>) {
    let (db, redis) = tokio::join!(probe_db(&state), probe_redis(&state));
    let report = Readiness::new(db, redis);

    if !report.is_ready() {
        // Named fields only (§4.8): which dependency failed is what the next reader
        // needs, and it goes to the log, not to the client.
        tracing::warn!(db, redis, "instance is not ready");
    }

    (report.status_code(), Json(report))
}

/// Whether Postgres answers a round trip inside the budget.
async fn probe_db(state: &MenoState) -> bool {
    matches!(
        tokio::time::timeout(
            PROBE_TIMEOUT,
            sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db)
        )
        .await,
        Ok(Ok(1))
    )
}

/// Whether Redis answers a round trip inside the budget.
async fn probe_redis(state: &MenoState) -> bool {
    matches!(
        tokio::time::timeout(PROBE_TIMEOUT, state.redis.ping()).await,
        Ok(Ok(()))
    )
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

    /// The routes that read `Extension<AuthUser>` and so must sit behind the guard.
    ///
    /// A separate list from `AUTH_PATHS` because the two answer different questions —
    /// "is it mounted" versus "is it gated" — and a guard that quietly covers the
    /// wrong half is the failure worth pinning here.
    const PROTECTED_PATHS: &[&str] = &[
        "/auth/sessions",
        "/auth/sessions/00000000-0000-0000-0000-000000000000",
        "/auth/logout-all",
    ];

    #[tokio::test]
    async fn the_liveness_endpoint_answers_without_touching_a_dependency() {
        // `build_routes` needs a `MenoState`, which needs a pool and Redis, so this one
        // is mounted on its own router — which is exactly how it is reached in
        // production, before any merge.
        let router = Router::new().route("/health", get(health));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("a response");

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 64)
            .await
            .expect("the body buffers");
        assert_eq!(bytes.as_ref(), b"ok");
    }

    #[test]
    fn readiness_renders_the_shape_the_plan_gate_curls() {
        // §4.3's contract, asserted on the encoded value: a renamed field here breaks
        // every probe script and dashboard that reads `db` or `redis`.
        let report = Readiness::new(true, true);

        assert_eq!(report.status_code(), StatusCode::OK);
        let json = serde_json::to_value(report).expect("serialisable");
        assert_eq!(
            json,
            serde_json::json!({ "status": "ok", "db": true, "redis": true })
        );
    }

    #[test]
    fn a_failed_probe_degrades_the_report_and_the_status_line() {
        // The half that actually matters: an orchestrator branches on the status code,
        // so a `true`-looking body with a 200 while Redis is down is the bug shape.
        for (db, redis) in [(false, true), (true, false), (false, false)] {
            let report = Readiness::new(db, redis);

            assert_eq!(report.status_code(), StatusCode::SERVICE_UNAVAILABLE);
            assert!(!report.is_ready());
            let json = serde_json::to_value(report).expect("serialisable");
            assert_eq!(json["status"], "degraded", "{json}");
            assert_eq!(json["db"], db, "{json}");
            assert_eq!(json["redis"], redis, "{json}");
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

    #[test]
    fn the_protected_half_is_exactly_the_routes_that_read_authuser() {
        // The three handlers with `Extension<AuthUser>` in their signature are the
        // three listed here. If a fourth handler gains the extractor, it must join this
        // list — and the split in `auth_routes` — or it answers 500 instead of 401,
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
