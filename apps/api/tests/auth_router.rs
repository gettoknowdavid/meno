//! §10 **Contract** layer: the auth router, driven end to end.
//!
//! # Why this suite exists
//!
//! `auth_service.rs` asserts on the *service*; these assert on the *wire*. A
//! handler can be correct in isolation and still answer 200 where the contract says
//! 401, or return a bare `{"error": "..."}` instead of the §4.2 envelope. Nothing below
//! the handler would catch either, which is why §10 ranks router tests separately from
//! unit tests rather than treating them as a longer version of the same thing.
//!
//! The router here is built by hand rather than reused from
//! [`meno_api::routes::build_routes`], because that needs a real `MenoState` — a live
//! Postgres pool and Redis client — and assembling those would turn this into an
//! integration test against shared infrastructure. Keeping the router local keeps the
//! suite hermetic; the cost is that `router()` must be kept in step with the real
//! table by hand.
//!
//! # What this cannot prove
//!
//! No Postgres, Redis or SMTP runs here; see `support::mod` for why that is a
//! deliberate split rather than a gap being papered over.

// The three panicking lints are denied workspace-wide but explicitly *allowed in
// tests* — see `clippy.toml`, whose comment states the policy as "deny in
// apps/api/src, allow in tests/". The `allow-*-in-tests` keys there cover `#[test]`
// functions; the shared helpers in `support/mod.rs` and the small `async fn` helpers
// below are not `#[test]` functions, so the allowance does not reach them.
//
// This is the same policy applied to the same code, not a relaxation of it: the rule
// exists so a *request path* cannot panic, and nothing here serves a request. Every
// `expect` below is a fixture that would make the test meaningless if it failed, and
// its message says so.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{Request as HttpRequest, StatusCode, header};
use axum::middleware::{self, Next};
use axum::routing::{get, post};
use http_body_util::BodyExt;
use meno_api::middleware::auth::{
    AuthState as GuardState, AuthUser, JwtVerifier, TokenBlocklist, auth_middleware,
};
use meno_api::modules::auth::model::{AuthProvider, UserRole};
use meno_api::modules::auth::state::AuthState;
use meno_core::Error as MenoError;
use support::{Harness, PASSWORD, harness, last_mail, registered};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tower::ServiceExt;
use uuid::Uuid;

/// The routes under test, with `AuthState` installed.
fn router(state: AuthState) -> Router {
    Router::new()
        .route(
            "/auth/register",
            post(meno_api::modules::auth::handlers::register),
        )
        .route(
            "/auth/login",
            post(meno_api::modules::auth::handlers::login),
        )
        .route(
            "/auth/refresh",
            post(meno_api::modules::auth::handlers::refresh),
        )
        .route(
            "/auth/logout",
            post(meno_api::modules::auth::handlers::logout),
        )
        .route(
            "/auth/sessions",
            get(meno_api::modules::auth::handlers::list_sessions),
        )
        .route(
            "/auth/forgot-password",
            post(meno_api::modules::auth::handlers::forgot_password),
        )
        .route(
            "/auth/reset-password",
            post(meno_api::modules::auth::handlers::reset_password),
        )
        .with_state(state)
}

fn json_post(uri: &str, body: String) -> HttpRequest<Body> {
    HttpRequest::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("the request is well formed")
}

fn json_get(uri: &str) -> HttpRequest<Body> {
    HttpRequest::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("the request is well formed")
}

fn bearer_get(uri: &str, token: &str) -> HttpRequest<Body> {
    HttpRequest::builder()
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("the request is well formed")
}

/// The production guard, with the harness's own blocklist.
fn guard(cache: Arc<dyn TokenBlocklist>) -> GuardState {
    GuardState::new(
        Arc::new(
            JwtVerifier::new("integration-access-secret")
                .expect("the fixture access secret builds a verifier"),
        ),
        cache,
    )
}

/// One protected route behind the real middleware, as the router wires it.
fn protected(guard: GuardState) -> Router {
    Router::new()
        .route("/protected", get(|| async { StatusCode::OK }))
        .layer(middleware::from_fn_with_state(guard, auth_middleware))
}

/// The status a bearer request gets through the production guard.
async fn guarded_status(guard: GuardState, token: &str) -> StatusCode {
    protected(guard)
        .oneshot(bearer_get("/protected", token))
        .await
        .expect("the router answers")
        .status()
}

/// A blocklist whose backend is down, for the infrastructure-error case.
#[derive(Debug)]
struct FailingBlocklist;

#[async_trait]
impl TokenBlocklist for FailingBlocklist {
    async fn is_revoked(&self, _jti: Uuid, _expires_at: i64) -> Result<bool, MenoError> {
        Err(MenoError::Upstream {
            service: "blocklist",
            detail: "unreachable in the fixture".to_owned(),
        })
    }

    async fn is_user_revoked(&self, _user_id: Uuid, _issued_at: i64) -> Result<bool, MenoError> {
        Err(MenoError::Upstream {
            service: "blocklist",
            detail: "unreachable in the fixture".to_owned(),
        })
    }
}

/// The same router with the identity [`auth_middleware`] would have inserted.
///
/// The three session handlers read `Extension<AuthUser>`, and this suite has no
/// middleware — so without this the endpoint is unreachable, and the wire shape of the
/// only response in the API that carries two timestamps could not be asserted at all.
fn router_as(state: AuthState, user: AuthUser) -> Router {
    router(state).layer(middleware::from_fn(
        move |mut request: Request, next: Next| {
            let user = user.clone();
            async move {
                request.extensions_mut().insert(user);
                next.run(request).await
            }
        },
    ))
}

/// The fixture identity, standing in for a verified access token.
fn caller(id: Uuid) -> AuthUser {
    AuthUser {
        id,
        jti: Uuid::from_u128(1),
        full_name: "Ada Lovelace".to_owned(),
        email: "ada@example.com".to_owned(),
        verified: true,
        providers: vec![AuthProvider::Password],
        role: UserRole::User,
    }
}

/// Assert that no value anywhere in `value` is `time`'s nine-number date tuple.
///
/// `time`'s default `Serialize` for `OffsetDateTime` emits
/// `[year, ordinal, hour, minute, second, nanosecond, offset_h, offset_m, offset_s]`
/// because the crate avoids strings for compactness in binary formats. It is the shape
/// this API shipped from `GET /auth/sessions` until `dto.rs` annotated its fields, and
/// it is unreadable: element 1 is a *day of year*, not a month. Walking the tree rather
/// than asserting two named fields is what keeps the guard honest — a date is still a
/// tuple if it is nested in `data`, in a list, or behind a `#[serde(flatten)]`.
fn assert_no_tuple_dates(value: &serde_json::Value, path: &str) {
    match value {
        serde_json::Value::Array(items) => {
            if items.len() == 9 && items.iter().all(serde_json::Value::is_i64) {
                panic!("`{path}` is a bare OffsetDateTime tuple: {value}");
            }
            for (i, item) in items.iter().enumerate() {
                assert_no_tuple_dates(item, &format!("{path}[{i}]"));
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                assert_no_tuple_dates(item, &format!("{path}.{key}"));
            }
        }
        _ => {}
    }
}

/// Assert that `value` is an RFC 3339 instant a client can actually parse.
///
/// Not just "it is a string": a wrong-but-stringy date fails here too, which is the
/// half of the contract that "is not an array" alone would not catch.
fn assert_rfc3339(value: &serde_json::Value, path: &str) {
    let text = value
        .as_str()
        .unwrap_or_else(|| panic!("`{path}` must be an RFC 3339 string, got {value}"));

    let parsed = OffsetDateTime::parse(text, &Rfc3339)
        .unwrap_or_else(|e| panic!("`{path}` = {text:?} is not RFC 3339: {e}"));

    assert!(
        parsed > OffsetDateTime::UNIX_EPOCH,
        "`{path}` = {text:?} parsed, but is not a real instant for this service"
    );
}

/// Send one request and read back the status and parsed body.
async fn send(router: Router, request: HttpRequest<Body>) -> (StatusCode, serde_json::Value) {
    let response = router.oneshot(request).await.expect("a response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body buffers")
        .to_bytes();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

/// A login body carrying the fixture password.
fn login_body() -> String {
    serde_json::json!({ "email": "ada@example.com", "password": PASSWORD }).to_string()
}

// ── the success envelope ───────────────────────────────────────────────────

#[tokio::test]
async fn a_correct_password_renders_the_success_envelope() {
    let h = harness();
    registered(&h).await;

    let (status, body) = send(router(h.state), json_post("/auth/login", login_body())).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], "OK");
    assert_eq!(body["status"], true);
    assert!(body["data"]["access_token"].is_string(), "{body}");
    assert!(body["data"]["refresh_token"].is_string(), "{body}");
    assert!(body["data"]["expires_in"].is_i64(), "{body}");
}

#[tokio::test]
async fn registration_answers_201_and_carries_a_session() {
    let h = harness();
    let request = serde_json::json!({
        "full_name": "Ada Lovelace",
        "email": "Ada@Example.com",
        "password": PASSWORD,
    })
    .to_string();

    let (status, body) = send(router(h.state), json_post("/auth/register", request)).await;

    // 201, not 200: the account did not exist before this request. Asserting it here
    // rather than accepting either is deliberate — a client that caches on 200 would
    // otherwise be told a fresh registration was a repeat.
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["status"], true);
    assert!(body["data"]["access_token"].is_string(), "{body}");
    // The address was normalised at the boundary (§9.5), so the caller sees the
    // canonical form rather than the mixed-case one it sent.
    assert_eq!(body["data"]["user"]["email"], "ada@example.com", "{body}");
}

// ── the §4.2 error taxonomy on the wire ─────────────────────────────────────

#[tokio::test]
async fn a_wrong_password_is_401_and_names_no_field() {
    let h = harness();
    registered(&h).await;

    let body = serde_json::json!({ "email": "ada@example.com", "password": "not it" }).to_string();
    let (status, body) = send(router(h.state), json_post("/auth/login", body)).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "INVALID_CREDENTIALS");
    assert_eq!(body["status"], false);
    // A validation failure carries per-field detail; an authentication failure must not,
    // or the response becomes an oracle for which part of the request was wrong.
    assert!(body.get("data").is_none(), "{body}");
}

#[tokio::test]
async fn an_unknown_address_is_answered_identically_to_a_wrong_password() {
    let h = harness();
    registered(&h).await;

    let unknown =
        serde_json::json!({ "email": "nobody@example.com", "password": PASSWORD }).to_string();
    let wrong = serde_json::json!({ "email": "ada@example.com", "password": "not it" }).to_string();

    let (unknown_status, unknown_body) =
        send(router(h.state.clone()), json_post("/auth/login", unknown)).await;
    let (wrong_status, wrong_body) = send(router(h.state), json_post("/auth/login", wrong)).await;

    assert_eq!(
        unknown_status, wrong_status,
        "status must not distinguish them"
    );
    assert_eq!(
        unknown_body, wrong_body,
        "byte-identical responses: an unknown address must not be observable"
    );
}

#[tokio::test]
async fn a_malformed_field_is_422_and_carries_the_field_name() {
    let h = harness();
    let body = serde_json::json!({ "email": "not-an-address", "password": "" }).to_string();

    let (status, body) = send(router(h.state), json_post("/auth/login", body)).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "VALIDATION_FAILED");
    assert!(
        body["data"]["email"].is_array(),
        "the client needs to know which field to fix: {body}"
    );
}

#[tokio::test]
async fn a_body_that_is_not_json_is_400_not_500() {
    let h = harness();

    let (status, body) = send(router(h.state), json_post("/auth/login", "{".to_owned())).await;

    // `JSON_SYNTAX_ERROR` rather than the generic `BAD_REQUEST`: §4.2's point is that a
    // client can branch on a specific code, and "your JSON is broken" is a different
    // fix from "a field is unacceptable". Both are 400 — the taxonomy refines within
    // a status, it does not replace it.
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "JSON_SYNTAX_ERROR");
    assert_eq!(body["status"], false);
}

// ── §4.7: sessions and revocation, over HTTP ────────────────────────────────

#[tokio::test]
async fn refreshing_rotates_the_pair_and_the_old_one_stops_working() {
    let h = harness();
    let (_, refresh) = registered(&h).await;

    let body = serde_json::json!({ "refresh_token": refresh }).to_string();
    let (status, rotated) = send(router(h.state.clone()), json_post("/auth/refresh", body)).await;
    assert_eq!(status, StatusCode::OK, "{rotated}");

    let new_refresh = rotated["data"]["refresh_token"]
        .as_str()
        .expect("a rotated refresh token")
        .to_owned();
    assert_ne!(
        new_refresh, refresh,
        "rotation must produce a different token"
    );

    // Replaying the superseded token is the §4.7 item 2 case: a refusal *and* the end
    // of every session for this user.
    let replay = serde_json::json!({ "refresh_token": refresh }).to_string();
    let (status, refused) = send(router(h.state.clone()), json_post("/auth/refresh", replay)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{refused}");

    // And the token issued by the rotation died with them.
    let after = serde_json::json!({ "refresh_token": new_refresh }).to_string();
    let (status, _) = send(router(h.state), json_post("/auth/refresh", after)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_forged_refresh_token_is_refused_and_revokes_nothing() {
    let h = harness();
    let (_, refresh) = registered(&h).await;

    let forged = serde_json::json!({ "refresh_token": "not.a.jwt.at.all" }).to_string();
    let (status, _) = send(router(h.state.clone()), json_post("/auth/refresh", forged)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The distinction that matters: a *forged* token proves nothing about a real
    // session, so it must not be able to sign a user out. Only a replay of a token that
    // genuinely existed may trigger the family revoke.
    let genuine = serde_json::json!({ "refresh_token": refresh }).to_string();
    let (status, body) = send(router(h.state), json_post("/auth/refresh", genuine)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a forged token must not have revoked this: {body}"
    );
}

#[tokio::test]
async fn every_date_on_the_wire_is_an_rfc3339_string() {
    // The regression: `GET /auth/sessions` answered
    //   "createdAt": [2026, 278, 20, 51, 45, 911000000, 0, 0, 0]
    // because a bare `OffsetDateTime` field carries no format instruction and `time`
    // serialises itself as a nine-number tuple. No client can read that — element 1 is a
    // day of year, not a month — so this walks every response the suite can reach and
    // refuses anything but an RFC 3339 string.
    let h = harness();
    let (user, _) = registered(&h).await;

    let (status, login) = send(
        router(h.state.clone()),
        json_post("/auth/login", login_body()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{login}");

    let (status, sessions) = send(
        router_as(h.state, caller(user.id)),
        json_get("/auth/sessions"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sessions}");
    assert!(
        sessions["data"].is_array() && !sessions["data"].as_array().expect("an array").is_empty(),
        "the fixture session must be listed, or this proves nothing: {sessions}"
    );

    for (name, body) in [("login", &login), ("sessions", &sessions)] {
        assert_no_tuple_dates(body, name);
    }

    // Named, so a failure says which field regressed rather than only that some
    // array of nine integers turned up somewhere in the document.
    let first = &sessions["data"][0];
    assert_rfc3339(&first["created_at"], "sessions[0].created_at");
    assert_rfc3339(&first["last_used_at"], "sessions[0].last_used_at");
    assert_rfc3339(
        &login["data"]["user"]["created_at"],
        "login.data.user.created_at",
    );

    // And the two are consistent, which is what makes the string meaningful: the
    // session's creation and the account's creation are both "now" for this fixture, so
    // they must not be decades apart in some other calendar.
    let session_created = OffsetDateTime::parse(
        first["created_at"].as_str().expect("asserted above"),
        &Rfc3339,
    )
    .expect("asserted above");
    let user_created = OffsetDateTime::parse(
        login["data"]["user"]["created_at"]
            .as_str()
            .expect("asserted above"),
        &Rfc3339,
    )
    .expect("asserted above");
    assert!(
        (session_created - user_created).abs() < time::Duration::minutes(1),
        "one fixture, two unrelated timestamps: {session_created} vs {user_created}"
    );
}

#[tokio::test]
async fn an_unknown_route_is_404_rather_than_a_500() {
    let h = harness();

    let (status, _) = send(
        router(h.state),
        json_post("/auth/nonexistent", "{}".to_owned()),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── Steps 65 and 67: revocation, through the real guard ─────────────────────

#[tokio::test]
async fn logout_all_invalidates_existing_access_tokens_but_not_a_fresh_login() {
    let h = harness();
    let (user, _) = registered(&h).await;
    let pair = h
        .tokens
        .issue_pair(&user, vec![AuthProvider::Password], Default::default())
        .await
        .expect("issuing an access token");

    let cache = Arc::clone(&h.cache) as Arc<dyn TokenBlocklist>;
    assert_eq!(
        guarded_status(guard(Arc::clone(&cache)), &pair.access_token).await,
        StatusCode::OK
    );

    h.service
        .logout_everywhere(user.id)
        .await
        .expect("logout-all");

    assert_eq!(
        guarded_status(guard(Arc::clone(&cache)), &pair.access_token).await,
        StatusCode::UNAUTHORIZED,
        "an access token minted before logout-all must stop working"
    );

    // The marker is a wall-clock second, so a token minted in the same second would
    // legitimately be covered by it. Wait past the boundary rather than weakening the
    // comparison to `<` in the implementation.
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    let fresh = h
        .tokens
        .issue_pair(&user, vec![AuthProvider::Password], Default::default())
        .await
        .expect("a fresh login");
    assert_eq!(
        guarded_status(guard(cache), &fresh.access_token).await,
        StatusCode::OK,
        "a token issued after the marker must survive"
    );
}

#[tokio::test]
async fn a_revoked_jti_is_still_refused() {
    let h = harness();
    let (user, _) = registered(&h).await;
    let pair = h
        .tokens
        .issue_pair(&user, vec![AuthProvider::Password], Default::default())
        .await
        .expect("issuing a token");

    h.tokens
        .revoke(&pair.refresh_token, Some(&pair.access_token))
        .await
        .expect("revoking one session");

    let cache = Arc::clone(&h.cache) as Arc<dyn TokenBlocklist>;
    assert_eq!(
        guarded_status(guard(cache), &pair.access_token).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn a_blocklist_outage_is_not_an_authentication_failure() {
    let h = harness();
    let (user, _) = registered(&h).await;
    let pair = h
        .tokens
        .issue_pair(&user, vec![AuthProvider::Password], Default::default())
        .await
        .expect("issuing a token");

    let status = guarded_status(guard(Arc::new(FailingBlocklist)), &pair.access_token).await;
    assert!(
        status.is_server_error(),
        "an unreachable blocklist must not log the user out: got {status}"
    );
}

#[tokio::test]
async fn a_reset_request_does_not_revoke_but_a_completed_reset_does() {
    let h = harness();
    let (user, _) = registered(&h).await;
    let pair = h
        .tokens
        .issue_pair(&user, vec![AuthProvider::Password], Default::default())
        .await
        .expect("issuing a token");
    let cache = Arc::clone(&h.cache) as Arc<dyn TokenBlocklist>;

    let forgot = serde_json::json!({ "email": "ada@example.com" }).to_string();
    let (status, _) = send(
        router(h.state.clone()),
        json_post("/auth/forgot-password", forgot),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        guarded_status(guard(Arc::clone(&cache)), &pair.access_token).await,
        StatusCode::OK,
        "requesting a reset is not ownership proof, so it must not revoke"
    );

    let reset = serde_json::json!({
        "email": "ada@example.com",
        "code": "000000",
        "new_password": "A completely new password",
    })
    .to_string();
    let (status, body) = send(
        router(h.state.clone()),
        json_post("/auth/reset-password", reset),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a wrong code is refused: {body}"
    );
    assert_eq!(
        guarded_status(guard(Arc::clone(&cache)), &pair.access_token).await,
        StatusCode::OK,
        "a failed code attempt must leave the session alone"
    );

    let code = last_mail(&h).expect("the reset code was mailed").code;
    let reset = serde_json::json!({
        "email": "ada@example.com",
        "code": code,
        "new_password": "A completely new password",
    })
    .to_string();
    let (status, body) = send(router(h.state), json_post("/auth/reset-password", reset)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        guarded_status(guard(cache), &pair.access_token).await,
        StatusCode::UNAUTHORIZED,
        "a completed reset must invalidate the old access token"
    );
}

// ── what the harness itself guarantees ──────────────────────────────────────

#[tokio::test]
async fn the_harness_records_the_message_it_asserts_on() {
    // A guard on the harness rather than on the module: if `mailer` ever became a
    // second, empty double again, every "a code was sent" assertion in this file would
    // pass vacuously. This fails loudly instead.
    let h: Harness = harness();

    assert_eq!(h.mailer.count(), 0, "nothing sent yet");

    h.service
        .register(&support::register_request())
        .await
        .expect("registering");

    assert_eq!(
        h.mailer.count(),
        1,
        "registration dispatched exactly one message"
    );

    let mail = last_mail(&h).expect("a recorded message");
    assert_eq!(mail.to, "ada@example.com");
    assert_eq!(mail.code.len(), 6, "six digits: {}", mail.code);
}
