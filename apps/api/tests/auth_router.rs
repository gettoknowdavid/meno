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
//! The router here is built by hand because `routes.rs` has not landed yet. When it
//! does, this file's `router()` should be replaced by the real one and these tests
//! should keep passing unchanged — that is the property being protected.
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

use axum::Router;
use axum::body::Body;
use axum::http::{Request as HttpRequest, StatusCode, header};
use axum::routing::{get, post};
use http_body_util::BodyExt;
use support::{Harness, PASSWORD, harness, last_mail, registered};
use tower::ServiceExt;

/// The routes under test, with `AuthState` installed.
fn router(state: meno_api::modules::auth::state::AuthState) -> Router {
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
async fn an_unknown_route_is_404_rather_than_a_500() {
    let h = harness();

    let (status, _) = send(
        router(h.state),
        json_post("/auth/nonexistent", "{}".to_owned()),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
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
