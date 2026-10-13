//! §10 **contract** layer: the subscribers router, driven end to end through axum.
//!
//! `subscribers_service.rs` asserts on the *service*; these assert on the *wire* — the §4.2
//! envelope, camelCase keys, RFC 3339 dates, and the status codes the contract promises.
//!
//! The router is built by hand with the in-memory double, so the suite is hermetic;
//! `routes::subscribers::SUBSCRIBERS_PATHS` exists so `routes/mod.rs` keeps the real table
//! in step with this one by hand.
//!
//! # Why the paths are spelled out rather than imported
//!
//! [`routes::subscribers::routes`] needs an `AuthState` and a Redis handle, neither of which
//! this suite should have. The paths are therefore repeated here — which is also what makes
//! this file a check on them: a path that axum refuses (or that the real table spells
//! differently) fails here first.

// The panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{Method, Request as HttpRequest, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use http_body_util::BodyExt;
use meno_api::middleware::auth::AuthUser;
use meno_api::modules::auth::model::UserRole;
use meno_api::modules::subscribers::handlers;
use meno_api::modules::subscribers::repository::{InMemorySubscribersRepo, SubscribersRepo};
use meno_api::modules::subscribers::state::SubscribersState;
use serde_json::json;
use time::OffsetDateTime;
use tower::ServiceExt;
use uuid::Uuid;

// ── fixtures ──────────────────────────────────────────────────────────────

fn ada() -> Uuid {
    Uuid::from_u128(0x1)
}

fn grace() -> Uuid {
    Uuid::from_u128(0x2)
}

fn unknown() -> Uuid {
    Uuid::from_u128(0xff)
}

fn caller(id: Uuid) -> AuthUser {
    AuthUser {
        id,
        jti: id,
        full_name: "Ada Lovelace".to_owned(),
        email: "ada@example.com".to_owned(),
        verified: true,
        providers: vec![],
        role: UserRole::User,
    }
}

fn instant(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds).expect("a plausible instant")
}

/// A repository with Ada, Grace and Adam, and Ada following Grace.
fn seeded() -> Arc<InMemorySubscribersRepo> {
    let repo = Arc::new(InMemorySubscribersRepo::new());
    repo.seed_user(ada(), "Ada Lovelace", Some("first programmer"), None);
    repo.seed_user(grace(), "Grace Hopper", None, Some("https://cdn/g.png"));
    repo.seed_user(Uuid::from_u128(0x3), "Adam Smith", None, None);
    repo.seed_follow(ada(), grace(), instant(15));
    repo
}

/// The wired state, with a repo the test can read afterwards.
fn state(repo: Arc<InMemorySubscribersRepo>) -> SubscribersState {
    SubscribersState::from_parts(Arc::clone(&repo) as Arc<dyn SubscribersRepo>)
}

/// The subscribers routes under test, mirroring `routes::subscribers` for these paths.
fn router(state: SubscribersState) -> Router {
    Router::new()
        .route(
            "/subscribers/{id}",
            post(handlers::subscribe).delete(handlers::unsubscribe),
        )
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
        .with_state(state)
}

fn router_as(state: SubscribersState, user: AuthUser) -> Router {
    router(state).layer(axum::middleware::from_fn(
        move |mut request: Request, next: Next| {
            let user = user.clone();
            async move {
                request.extensions_mut().insert(user);
                next.run(request).await
            }
        },
    ))
}

/// The auth guard's own refusal, so "unauthenticated is a 401" can be asserted without
/// standing up Redis.
fn router_with_auth_gate(state: SubscribersState) -> Router {
    router(state).layer(axum::middleware::from_fn(
        |request: Request, next: Next| async move {
            if request.extensions().get::<AuthUser>().is_some() {
                next.run(request).await
            } else {
                let body = axum::Json(meno_core::to_body(&meno_core::Error::Unauthorized {
                    code: meno_core::ErrorCode::Unauthorized,
                    message: "an access token is required".to_owned(),
                }));
                (StatusCode::UNAUTHORIZED, body).into_response()
            }
        },
    ))
}

/// Send one request and read back the status and parsed body.
async fn send(
    router: Router,
    method: Method,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = HttpRequest::builder().method(method).uri(uri);
    let payload = match &body {
        Some(value) => {
            builder = builder.header(axum::http::header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let request = builder.body(payload).expect("a valid request");

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

// ── following and unfollowing ─────────────────────────────────────────────

#[tokio::test]
async fn following_a_user_is_200_with_no_data_key() {
    let repo = seeded();
    let (status, response) = send(
        router_as(state(Arc::clone(&repo)), caller(ada())),
        Method::POST,
        &format!("/subscribers/{}", Uuid::from_u128(0x3)),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["code"], "OK");
    assert_eq!(response["status"], true);
    assert_eq!(response["message"], "Subscribed to user successfully");
    assert!(
        response.get("data").is_none(),
        "no payload is sent, not a null one: {response}"
    );
    assert!(repo.follows(ada(), Uuid::from_u128(0x3)));
}

#[tokio::test]
async fn following_the_same_user_twice_is_200_and_stores_one_row() {
    // A retried tap. There is no idempotency middleware on this router (§4.5's layer buys
    // nothing for a write that cannot mint a second resource), so this proves the statement
    // itself is idempotent.
    let repo = seeded();
    let target = Uuid::from_u128(0x3);

    for _ in 0..2 {
        let (status, response) = send(
            router_as(state(Arc::clone(&repo)), caller(ada())),
            Method::POST,
            &format!("/subscribers/{target}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{response}");
    }

    assert_eq!(repo.stored_follow_count(), 2, "Ada follows Grace and Adam");
}

#[tokio::test]
async fn following_yourself_is_400_and_writes_nothing() {
    let repo = seeded();
    let before = repo.stored_follow_count();

    let (status, response) = send(
        router_as(state(Arc::clone(&repo)), caller(ada())),
        Method::POST,
        &format!("/subscribers/{}", ada()),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["code"], "BAD_REQUEST");
    assert_eq!(response["message"], "You cannot subscribe to yourself");
    assert_eq!(repo.stored_follow_count(), before);
}

#[tokio::test]
async fn following_an_unknown_user_is_404_not_found() {
    let repo = seeded();

    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::POST,
        &format!("/subscribers/{}", unknown()),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{response}");
    assert_eq!(response["code"], "NOT_FOUND");
    assert_eq!(response["message"], "user not found");
}

#[tokio::test]
async fn unfollowing_is_200_even_when_there_was_nothing_to_remove() {
    let repo = seeded();

    // Ada follows Grace; the second call has nothing to delete and must still succeed.
    for _ in 0..2 {
        let (status, response) = send(
            router_as(state(Arc::clone(&repo)), caller(ada())),
            Method::DELETE,
            &format!("/subscribers/{}", grace()),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{response}");
        assert_eq!(response["message"], "Unsubscribed successfully");
    }

    assert!(!repo.follows(ada(), grace()));
}

#[tokio::test]
async fn unfollowing_an_unknown_user_is_404() {
    let repo = seeded();

    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::DELETE,
        &format!("/subscribers/{}", unknown()),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{response}");
    assert_eq!(response["code"], "NOT_FOUND");
}

// ── the lists ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_followers_page_is_the_success_envelope_in_camel_case_with_rfc3339_dates() {
    let repo = seeded();

    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::GET,
        &format!("/subscribers/{}/subscribers", grace()),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["code"], "OK");
    assert_eq!(response["message"], "Subscribers retrieved");

    let page = response["data"].as_object().expect("a page object");
    assert_eq!(page["hasNextPage"], json!(false));
    assert!(
        page["data"].is_array(),
        "a bare null is not a page: {response}"
    );

    let item = &page["data"][0];
    assert_eq!(item["fullName"], "Ada Lovelace");
    assert_eq!(item["isFollowing"], json!(false));
    assert!(item.get("avatarUrl").is_some(), "camelCase key: {response}");
    assert!(
        item["subscribedAt"].is_string(),
        "an RFC 3339 string: {response}"
    );
    assert!(
        !response.to_string().contains("subscribed_at"),
        "a snake_case key leaked: {response}"
    );
}

#[tokio::test]
async fn the_callers_own_two_lists_are_readable() {
    let repo = seeded();

    let (status, followers) = send(
        router_as(state(Arc::clone(&repo)), caller(grace())),
        Method::GET,
        "/subscribers/me/subscribers",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{followers}");
    assert_eq!(followers["message"], "Subscribers retrieved");
    assert_eq!(followers["data"]["data"][0]["id"], ada().to_string());

    let (status, following) = send(
        router_as(state(repo), caller(ada())),
        Method::GET,
        "/subscribers/me/subscriptions",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{following}");
    assert_eq!(following["message"], "Subscriptions retrieved");
    assert_eq!(following["data"]["data"][0]["id"], grace().to_string());
}

#[tokio::test]
async fn a_user_with_no_followers_answers_an_empty_page_rather_than_a_404() {
    // The wired check has to distinguish "nobody follows them" from "no such user" — the
    // second is the next test.
    let repo = seeded();

    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::GET,
        &format!("/subscribers/{}/subscribers", Uuid::from_u128(0x3)),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(
        response["data"]["data"].as_array().expect("a list").len(),
        0
    );
    assert_eq!(response["data"]["hasNextPage"], json!(false));
}

#[tokio::test]
async fn a_list_for_an_unknown_user_is_404() {
    let repo = seeded();

    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::GET,
        &format!("/subscribers/{}/subscriptions", unknown()),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{response}");
    assert_eq!(response["code"], "NOT_FOUND");
}

#[tokio::test]
async fn an_undecodable_cursor_is_400_invalid_cursor() {
    let repo = seeded();

    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::GET,
        "/subscribers/me/subscribers?cursor=not-a-cursor",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["code"], "INVALID_CURSOR");
}

#[tokio::test]
async fn an_unauthenticated_request_is_401() {
    let repo = seeded();

    let (status, _) = send(
        router_with_auth_gate(state(repo)),
        Method::GET,
        "/subscribers/me/subscribers",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
