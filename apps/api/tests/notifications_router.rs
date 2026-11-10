//! §10 **contract** layer: the notifications router, driven end to end through axum.
//!
//! # Why this suite exists
//!
//! `notifications_service.rs` asserts on the *service*; these assert on the *wire*. A
//! handler can be correct in isolation and still answer 200 where the contract says 401,
//! emit a bare `{"error": "…"}` instead of the §4.2 envelope, or send `type_code` where
//! the clients read `typeCode`. Nothing below the handler would catch any of those.
//!
//! The router is built by hand rather than reused from [`meno_api::routes::build_routes`],
//! because that needs a real `MenoState` — a live Postgres pool and Redis client. Keeping
//! the router local keeps the suite hermetic; the cost is that `router()` must be kept in
//! step with the real table by hand, which is why `routes::notifications::NOTIFICATIONS_PATHS`
//! exists and [`every_documented_path_is_served_by_this_router`] checks this file against it.
//!
//! # What this cannot prove
//!
//! No Postgres runs here; the storage is the in-memory double. `pg.rs` is covered by
//! `notifications_repository_pg.rs`, which needs a live database.

// Panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
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
use axum::routing::{delete, get, patch};
use http_body_util::BodyExt;
use meno_api::infrastructure::push::NoopPushSender;
use meno_api::middleware::auth::AuthUser;
use meno_api::modules::auth::model::UserRole;
use meno_api::modules::notifications::counter::{InMemoryUnreadCounter, UnreadCounter};
use meno_api::modules::notifications::handlers;
use meno_api::modules::notifications::model::codes;
use meno_api::modules::notifications::publisher::RecordingNotifier;
use meno_api::modules::notifications::repository::{InMemoryNotificationsRepo, NotificationsRepo};
use meno_api::modules::notifications::state::NotificationsState;
use meno_api::routes::NOTIFICATIONS_PATHS;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tower::ServiceExt;
use uuid::Uuid;

// ── building the router ────────────────────────────────────────────────────

/// The notifications routes under test, mirroring `routes::notifications` path for path.
fn router(state: NotificationsState) -> Router {
    Router::new()
        .route("/notifications", get(handlers::get_notifications))
        .route(
            "/notifications/unread-count",
            get(handlers::get_unread_count),
        )
        .route("/notifications/read-all", patch(handlers::mark_all_read))
        .route("/notifications/{id}/read", patch(handlers::mark_read))
        .route("/notifications/{id}", delete(handlers::delete_notification))
        .with_state(state)
}

/// The same router with the identity the auth middleware would have inserted.
fn router_as(state: NotificationsState, user: AuthUser) -> Router {
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

/// A router that gates the routes on the presence of an `AuthUser` extension, returning
/// 401 when it is missing — the real middleware's behaviour, without a token verifier.
fn router_with_auth_gate(state: NotificationsState) -> Router {
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
async fn send(router: Router, method: Method, uri: &str) -> (StatusCode, serde_json::Value) {
    let request = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("a valid request");

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

/// Assert that no value anywhere in `value` is `time`'s nine-number date tuple.
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
fn assert_rfc3339(value: &serde_json::Value, path: &str) {
    let text = value
        .as_str()
        .unwrap_or_else(|| panic!("`{path}` must be an RFC 3339 string, got {value}"));

    OffsetDateTime::parse(text, &Rfc3339)
        .unwrap_or_else(|e| panic!("`{path}` = {text:?} is not RFC 3339: {e}"));
}

// ── fixtures ──────────────────────────────────────────────────────────────

/// The caller every request acts as.
fn ada() -> Uuid {
    Uuid::from_u128(0x1)
}

/// A second account, for the "not yours" cases.
fn grace() -> Uuid {
    Uuid::from_u128(0x3)
}

/// The account whose action triggered the notifications.
fn actor_id() -> Uuid {
    Uuid::from_u128(0x2)
}

/// The broadcast a notification talks about.
fn broadcast_id() -> Uuid {
    Uuid::from_u128(0x100)
}

fn instant() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("a plausible instant")
}

/// The fixture identity, standing in for a verified access token.
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

/// The wired module, with the repository and counter left concrete so a test can seed and
/// read them.
fn fixture() -> (
    NotificationsState,
    Arc<InMemoryNotificationsRepo>,
    Arc<InMemoryUnreadCounter>,
) {
    let repo = Arc::new(InMemoryNotificationsRepo::new());
    repo.seed_template(
        codes::USER_SUBSCRIBED,
        "New Follower",
        "{actor} started following you",
    );
    repo.seed_template(
        codes::LIVE_BROADCAST_STARTED,
        "Live Now",
        "{actor} is live: {title}",
    );
    repo.seed_user(actor_id(), "Ada Lovelace");
    repo.seed_broadcast(broadcast_id(), "Morning Notes");
    repo.seed_user(ada(), "Grace Hopper");
    repo.seed_user(grace(), "Alan Turing");

    let counter = Arc::new(InMemoryUnreadCounter::new());
    let state = NotificationsState::from_parts(
        Arc::clone(&repo) as Arc<dyn NotificationsRepo>,
        Arc::new(RecordingNotifier::new()),
        Arc::clone(&counter) as Arc<dyn UnreadCounter>,
        Arc::new(NoopPushSender),
    );

    (state, repo, counter)
}

/// Seed one unread broadcast notification for `owner`.
fn seed(owner: Uuid, repo: &InMemoryNotificationsRepo, seconds: i64) -> Uuid {
    repo.seed_notification(
        owner,
        codes::LIVE_BROADCAST_STARTED,
        Some(actor_id()),
        Some(broadcast_id()),
        OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds).expect("a plausible instant"),
        false,
    )
}

// ── the success envelope ───────────────────────────────────────────────────

#[tokio::test]
async fn reading_the_inbox_returns_200_and_the_success_envelope() {
    let (state, repo, _counter) = fixture();
    let id = seed(ada(), &repo, 0);

    let (status, response) = send(
        router_as(state, caller(ada())),
        Method::GET,
        "/notifications",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["code"], "OK");
    assert_eq!(response["status"], true);
    assert_eq!(response["message"], "Notifications retrieved");

    // The page is nested inside the envelope: `data.data` is the list.
    let page = response["data"].as_object().expect("the page is an object");
    assert_eq!(page["hasNextPage"], serde_json::json!(false));
    assert!(page["nextCursor"].is_null(), "one row is one page");

    let items = page["data"].as_array().expect("the list");
    assert_eq!(items.len(), 1, "{response}");
    assert_eq!(items[0]["id"], id.to_string());
    assert_eq!(items[0]["typeCode"], codes::LIVE_BROADCAST_STARTED);
    assert_eq!(items[0]["title"], "Live Now");
    assert_eq!(items[0]["body"], "Ada Lovelace is live: Morning Notes");
    assert_eq!(items[0]["read"], serde_json::json!(false));
    assert_eq!(items[0]["broadcastId"], broadcast_id().to_string());
    assert_eq!(items[0]["actor"]["id"], actor_id().to_string());
    assert_eq!(items[0]["actor"]["fullName"], "Ada Lovelace");
    assert_eq!(
        items[0]["deepLink"],
        format!("meno://broadcasts/{}", broadcast_id())
    );

    // Camel case, and nothing snake-leaking beside it.
    assert!(
        items[0].get("type_code").is_none(),
        "a snake_case key leaked: {response}"
    );
}

#[tokio::test]
async fn every_date_on_the_notifications_wire_is_an_rfc3339_string() {
    let (state, repo, _counter) = fixture();
    seed(ada(), &repo, 0);

    let (status, response) = send(
        router_as(state, caller(ada())),
        Method::GET,
        "/notifications",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");

    assert_no_tuple_dates(&response, "inbox");
    assert_rfc3339(&response["data"]["data"][0]["createdAt"], "createdAt");
}

#[tokio::test]
async fn the_page_is_cursor_paginated_when_asked_for_a_smaller_page() {
    let (state, repo, _counter) = fixture();
    let oldest = seed(ada(), &repo, 0);
    let middle = seed(ada(), &repo, 60);
    let newest = seed(ada(), &repo, 120);

    let (status, first) = send(
        router_as(state.clone(), caller(ada())),
        Method::GET,
        "/notifications?limit=2",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{first}");
    let page = &first["data"];
    assert_eq!(page["data"].as_array().expect("a list").len(), 2);
    assert_eq!(page["hasNextPage"], serde_json::json!(true));
    assert_eq!(page["data"][0]["id"], newest.to_string());
    assert_eq!(page["data"][1]["id"], middle.to_string());
    let cursor = page["nextCursor"]
        .as_str()
        .expect("a cursor for the next page")
        .to_owned();

    // The advertised cursor must be usable as the client would use it — round-tripped
    // through the query string, not handed back as a struct.
    let (status, second) = send(
        router_as(state, caller(ada())),
        Method::GET,
        &format!("/notifications?limit=2&cursor={cursor}"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{second}");
    let ids: Vec<&str> = second["data"]["data"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        vec![oldest.to_string().as_str()],
        "only the oldest row remains, and no row is repeated: {second}"
    );
    assert_eq!(
        second["data"]["hasNextPage"],
        serde_json::json!(false),
        "the cursor must not advertise a page that does not exist"
    );
}

#[tokio::test]
async fn the_unread_filter_is_read_from_the_query_string() {
    let (state, repo, _counter) = fixture();
    let unread = seed(ada(), &repo, 0);
    repo.seed_notification(ada(), codes::USER_SUBSCRIBED, None, None, instant(), true);

    let (status, response) = send(
        router_as(state, caller(ada())),
        Method::GET,
        "/notifications?unreadOnly=true",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    let items = response["data"]["data"].as_array().expect("a list");
    assert_eq!(items.len(), 1, "{response}");
    assert_eq!(items[0]["id"], unread.to_string());
}

#[tokio::test]
async fn the_unread_count_endpoint_answers_the_badge() {
    let (state, repo, _counter) = fixture();
    seed(ada(), &repo, 0);
    seed(ada(), &repo, 60);
    seed(grace(), &repo, 120);

    let (status, response) = send(
        router_as(state, caller(ada())),
        Method::GET,
        "/notifications/unread-count",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["message"], "Unread count retrieved");
    assert_eq!(response["data"]["count"], serde_json::json!(2));
}

#[tokio::test]
async fn marking_one_read_answers_the_postcondition() {
    let (state, repo, counter) = fixture();
    let id = seed(ada(), &repo, 0);
    counter.increment(ada()).await;

    let (status, response) = send(
        router_as(state, caller(ada())),
        Method::PATCH,
        &format!("/notifications/{id}/read"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["message"], "Notification marked as read");
    assert_eq!(response["data"]["id"], id.to_string());
    assert_eq!(response["data"]["read"], serde_json::json!(true));
    assert!(repo.is_read(id, ada()));
    assert_eq!(counter.value(ada()), Some(0));
}

#[tokio::test]
async fn marking_the_whole_inbox_read_answers_how_many_moved() {
    let (state, repo, _counter) = fixture();
    seed(ada(), &repo, 0);
    seed(ada(), &repo, 60);
    seed(grace(), &repo, 120);

    let (status, response) = send(
        router_as(state, caller(ada())),
        Method::PATCH,
        "/notifications/read-all",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["message"], "All notifications marked as read");
    assert_eq!(
        response["data"]["updated"],
        serde_json::json!(2),
        "only the caller's unread rows move: {response}"
    );
}

#[tokio::test]
async fn deleting_a_notification_is_a_200_with_no_data_and_is_idempotent() {
    let (state, repo, _counter) = fixture();
    let id = seed(ada(), &repo, 0);

    let (status, response) = send(
        router_as(state.clone(), caller(ada())),
        Method::DELETE,
        &format!("/notifications/{id}"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["message"], "Notification deleted");
    assert!(
        response.get("data").is_none(),
        "a no-content success omits `data` rather than sending null: {response}"
    );
    assert!(!repo.has_notification(id, ada()));

    // The same DELETE again: the postcondition already holds.
    let (status, response) = send(
        router_as(state, caller(ada())),
        Method::DELETE,
        &format!("/notifications/{id}"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
}

// ── the auth gate ──────────────────────────────────────────────────────────

#[tokio::test]
async fn an_unauthenticated_read_is_401() {
    let (state, _repo, _counter) = fixture();

    let (status, _) = send(router_with_auth_gate(state), Method::GET, "/notifications").await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ── §4.2 error taxonomy on the wire ────────────────────────────────────────

#[tokio::test]
async fn marking_a_notification_that_is_not_there_is_404_with_a_code_to_branch_on() {
    let (state, _repo, _counter) = fixture();
    let missing = Uuid::from_u128(0xdead);

    let (status, response) = send(
        router_as(state, caller(ada())),
        Method::PATCH,
        &format!("/notifications/{missing}/read"),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{response}");
    assert_eq!(response["status"], serde_json::json!(false));
    assert_eq!(response["code"], "NOT_FOUND");
}

#[tokio::test]
async fn an_undecodable_cursor_is_400_rather_than_a_dropped_request() {
    let (state, repo, _counter) = fixture();
    seed(ada(), &repo, 0);

    let (status, response) = send(
        router_as(state, caller(ada())),
        Method::GET,
        "/notifications?cursor=not-a-cursor",
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["code"], "INVALID_CURSOR");
}

#[tokio::test]
async fn an_unknown_route_under_the_same_prefix_is_a_404() {
    // The negative control for the route table: a near-miss path must not be swallowed by
    // a notifications handler.
    let (state, _repo, _counter) = fixture();

    let (status, _) = send(
        router_as(state, caller(ada())),
        Method::GET,
        "/notification",
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn every_documented_path_is_served_by_this_router() {
    // The table and this file are written by hand, so they can drift. A path in
    // `NOTIFICATIONS_PATHS` that this router does not mount means the suite is testing a
    // table the application does not serve.
    //
    // "Served" cannot be "not a 404": `PATCH /notifications/{id}/read` legitimately
    // answers 404 for an id that does not exist. The two are distinguishable by the body —
    // axum's fallback 404 is empty, the module's carries the §4.2 envelope — so a 404
    // with a body means the handler ran.
    let (state, _repo, _counter) = fixture();

    for path in NOTIFICATIONS_PATHS {
        let mut served = false;
        for method in [Method::GET, Method::PATCH, Method::DELETE] {
            let (status, body) = send(router_as(state.clone(), caller(ada())), method, path).await;
            if status != StatusCode::METHOD_NOT_ALLOWED
                && (status != StatusCode::NOT_FOUND || !body.is_null())
            {
                served = true;
                break;
            }
        }
        assert!(served, "{path} is documented but not routed here");
    }
}
