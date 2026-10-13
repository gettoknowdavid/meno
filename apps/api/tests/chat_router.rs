//! §10 **contract** layer: the chat router, driven end to end through axum.
//!
//! # Why this suite exists
//!
//! `chat_service.rs` asserts on the *service*; these assert on the *wire*. A handler can
//! be correct in isolation and still answer 200 where the contract says 401, or emit a
//! bare `{"error": "..."}` instead of the §4.2 envelope. Nothing below the handler would
//! catch either.
//!
//! The router here is built by hand rather than reused from
//! [`meno_api::routes::build_routes`], because that needs a real `MenoState` — a live
//! Postgres pool and Redis client. Keeping the router local keeps the suite hermetic; the
//! cost is that `router()` must be kept in step with the real table by hand, which is why
//! `routes::chat::CHAT_PATHS` exists and `routes/mod.rs` asserts against it.
//!
//! # What this cannot prove
//!
//! No Postgres runs here; the storage is the in-memory double. `pg.rs` is covered by
//! `chat_repository_pg.rs`, which needs a live database.

// Panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{Request as HttpRequest, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::routing::get;
use http_body_util::BodyExt;
use meno_api::middleware::auth::AuthUser;
use meno_api::modules::chat::handlers;
use meno_api::modules::chat::model::{ChatMessage, SenderSummary};
use meno_api::modules::chat::publisher::ChatPublisher;
use meno_api::modules::chat::publisher::recording::RecordingPublisher;
use meno_api::modules::chat::repository::{ChatRepo, InMemoryChatRepo};
use meno_api::modules::chat::state::ChatState;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tower::ServiceExt;
use uuid::Uuid;

// ── building the router ────────────────────────────────────────────────────

/// The chat route under test, with the test doubles installed.
fn router(state: ChatState) -> Router {
    // The path is written out completely, exactly as `routes::chat` writes it, because
    // the real table is merged at the root rather than nested under `/broadcasts`.
    Router::new()
        .route(
            "/broadcasts/{id}/chat/messages",
            get(handlers::get_messages),
        )
        .with_state(state)
}

/// The same router with the identity the auth middleware would have inserted.
fn router_as(state: ChatState, user: AuthUser) -> Router {
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

/// A router that gates the route on the presence of an `AuthUser` extension, returning
/// 401 when it is missing — the real middleware's behaviour, without a token verifier.
fn router_with_auth_gate(state: ChatState) -> Router {
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
async fn send(router: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let request = HttpRequest::builder()
        .method("GET")
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

mod fixture {
    use super::*;

    /// The broadcast every request in this suite reads.
    pub fn room() -> Uuid {
        Uuid::from_u128(0x100)
    }

    pub fn ada() -> Uuid {
        Uuid::from_u128(0x1)
    }

    pub fn grace() -> Uuid {
        Uuid::from_u128(0x2)
    }

    /// A registered account who is in no room.
    pub fn stranger() -> Uuid {
        Uuid::from_u128(0x3)
    }

    /// The fixture identity, standing in for a verified access token.
    pub fn caller(id: Uuid) -> AuthUser {
        AuthUser {
            id,
            jti: id,
            full_name: "Ada Lovelace".to_owned(),
            email: "ada@example.com".to_owned(),
            verified: true,
            providers: vec![],
            role: meno_api::modules::auth::model::UserRole::User,
        }
    }

    /// A live room with Ada and Grace in it, one stored message, and the wired module.
    pub async fn state() -> (ChatState, Arc<InMemoryChatRepo>) {
        let repo = Arc::new(InMemoryChatRepo::new());
        for (id, name) in [(ada(), "Ada Lovelace"), (grace(), "Grace Hopper")] {
            repo.add_user(SenderSummary {
                id,
                full_name: name.to_owned(),
                bio: Some("mathematician".to_owned()),
                avatar_id: None,
                avatar_url: None,
            });
        }
        repo.start_broadcast(room());
        repo.add_participant(room(), ada());
        repo.add_participant(room(), grace());
        repo.seed_message(ChatMessage {
            id: Uuid::from_u128(0x11),
            broadcast_id: room(),
            sender: SenderSummary {
                id: ada(),
                full_name: "Ada Lovelace".to_owned(),
                bio: None,
                avatar_id: None,
                avatar_url: None,
            },
            content: "hello everyone".to_owned(),
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("a fixture"),
            updated_at: None,
        });

        let state = ChatState::from_parts(
            Arc::clone(&repo) as Arc<dyn ChatRepo>,
            Arc::new(RecordingPublisher::new()) as Arc<dyn ChatPublisher>,
        );

        (state, repo)
    }

    /// The URL every request in this suite reads, with an optional query string.
    pub fn url(query: &str) -> String {
        format!("/broadcasts/{}/chat/messages{query}", room())
    }
}

// ── the success envelope ───────────────────────────────────────────────────

#[tokio::test]
async fn reading_the_history_returns_200_and_the_success_envelope() {
    let (state, _repo) = fixture::state().await;

    let (status, response) = send(
        router_as(state, fixture::caller(fixture::ada())),
        &fixture::url(""),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["code"], "OK");
    assert_eq!(response["status"], true);
    assert_eq!(response["message"], "Messages retrieved");

    // The page is nested inside the envelope: `data.data` is the list.
    let page = response["data"].as_object().expect("the page is an object");
    assert_eq!(page["hasNextPage"], serde_json::json!(false));
    assert!(page["nextCursor"].is_null(), "one message is one page");

    let items = page["data"].as_array().expect("the list");
    assert_eq!(items.len(), 1, "{response}");
    assert_eq!(items[0]["content"], "hello everyone");
    assert_eq!(items[0]["broadcastId"], fixture::room().to_string());
    assert_eq!(items[0]["sender"]["id"], fixture::ada().to_string());
    assert_eq!(items[0]["sender"]["fullName"], "Ada Lovelace");
}

#[tokio::test]
async fn every_date_on_the_chat_wire_is_an_rfc3339_string() {
    let (state, _repo) = fixture::state().await;

    let (status, response) = send(
        router_as(state, fixture::caller(fixture::ada())),
        &fixture::url(""),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");

    assert_no_tuple_dates(&response, "history");
    assert_rfc3339(&response["data"]["data"][0]["createdAt"], "createdAt");
    assert!(
        response["data"]["data"][0].get("updatedAt").is_none(),
        "an unedited message omits `updatedAt` rather than sending null: {response}"
    );
    assert_eq!(
        response["data"]["data"][0]["isEdited"],
        serde_json::json!(false)
    );
}

#[tokio::test]
async fn the_page_is_cursor_paginated_when_asked_for_a_smaller_page() {
    let (state, repo) = fixture::state().await;
    // Three messages an hour apart, so the order is unambiguous.
    for (id, seconds) in [(0x12_u128, 3_600_i64), (0x13_u128, 7_200_i64)] {
        repo.seed_message(ChatMessage {
            id: Uuid::from_u128(id),
            broadcast_id: fixture::room(),
            sender: SenderSummary {
                id: fixture::ada(),
                full_name: "Ada Lovelace".to_owned(),
                bio: None,
                avatar_id: None,
                avatar_url: None,
            },
            content: format!("message {id}"),
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds)
                .expect("a fixture"),
            updated_at: None,
        });
    }

    let (status, first) = send(
        router_as(state.clone(), fixture::caller(fixture::ada())),
        &fixture::url("?limit=2"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["data"]["data"].as_array().expect("a list").len(), 2);
    assert_eq!(first["data"]["hasNextPage"], serde_json::json!(true));
    let cursor = first["data"]["nextCursor"]
        .as_str()
        .expect("a cursor for the next page")
        .to_owned();

    // The advertised cursor must be usable as the client would use it — round-tripped
    // through the query string, not handed back as a struct.
    let (status, second) = send(
        router_as(state, fixture::caller(fixture::ada())),
        &fixture::url(&format!("?limit=2&cursor={cursor}")),
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
        vec!["00000000-0000-0000-0000-000000000011"],
        "only the oldest row remains, and no row is repeated: {second}"
    );
}

// ── the auth gate ──────────────────────────────────────────────────────────

#[tokio::test]
async fn an_unauthenticated_read_is_401() {
    let (state, _repo) = fixture::state().await;

    let (status, _) = send(router_with_auth_gate(state), &fixture::url("")).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ── §4.2 error taxonomy on the wire ────────────────────────────────────────

#[tokio::test]
async fn somebody_outside_the_room_gets_403_with_a_code_to_branch_on() {
    let (state, _repo) = fixture::state().await;

    let (status, response) = send(
        router_as(state, fixture::caller(fixture::stranger())),
        &fixture::url(""),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{response}");
    assert_eq!(response["status"], false);
    assert_eq!(response["code"], "NOT_PARTICIPANT");
}

#[tokio::test]
async fn an_undecodable_cursor_is_400_rather_than_a_dropped_request() {
    let (state, _repo) = fixture::state().await;

    let (status, response) = send(
        router_as(state, fixture::caller(fixture::ada())),
        &fixture::url("?cursor=not-a-cursor"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["code"], "INVALID_CURSOR");
}

#[tokio::test]
async fn an_unknown_route_under_the_same_prefix_is_a_404() {
    // The negative control for the route table: a near-miss path must not be swallowed
    // by the chat handler.
    let (state, _repo) = fixture::state().await;

    let (status, _) = send(
        router_as(state, fixture::caller(fixture::ada())),
        &format!("/broadcasts/{}/chat/message", fixture::room()),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
