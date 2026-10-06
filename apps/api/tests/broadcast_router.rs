//! §10 **contract** layer: the broadcast router, driven end to end through axum.
//!
//! # Why this suite exists
//!
//! `broadcast_service.rs` asserts on the *service*; these assert on the *wire*. A
//! handler can be correct in isolation and still answer 200 where the contract says
//! 401, or return a bare `{\"error\": \"...\"}` instead of the §4.2 envelope. Nothing below
//! the handler would catch either, which is why §10 ranks router tests separately from
//! unit tests rather than treating them as a longer version of the same thing.
//!
//! The router here is built by hand rather than reused from
//! [`meno_api::routes::build_routes`], because that needs a real `MenoState` — a live
//! Postgres pool and Redis client — and assembling those would turn this into an
//! integration test against shared infrastructure. Keeping the router local keeps the
//! suite hermetic; the cost is that `router()` must be kept in step with the real table
//! by hand.
//!
//! # What this cannot prove
//!
//! No Postgres or LiveKit runs here; the doubles are the in-memory repository and the
//! recording media server. `pg.rs` is covered by the Postgres-backed suite; the media
//! adapter's real behaviour is covered by `media.rs`'s own tests.

// Panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml. The allow-*-in-tests keys cover #[test] functions; the helpers below
// are not #[test] functions, so the allowance does not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{Request as HttpRequest, StatusCode, header};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use http_body_util::BodyExt;
use meno_api::middleware::auth::AuthUser;
use meno_api::modules::broadcast::handlers;
use meno_api::modules::broadcast::state::BroadcastState;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tower::ServiceExt;
use uuid::Uuid;

// ── building the router ────────────────────────────────────────────────────

/// The broadcast routes under test, with the test doubles installed.
fn router(state: BroadcastState) -> Router {
    // The broadcast paths already begin with `/broadcasts`; the nest is at `/` so the
    // tree reads `GET /broadcasts` and not `/broadcasts/broadcasts`.
    Router::new()
        .route("/broadcasts", post(handlers::create).get(handlers::list))
        .route(
            "/broadcasts/{id}",
            get(handlers::get)
                .patch(handlers::update)
                .delete(handlers::delete),
        )
        .route("/broadcasts/{id}/live", post(handlers::go_live))
        .route("/broadcasts/{id}/end", post(handlers::end))
        .route("/broadcasts/{id}/join", post(handlers::join))
        .route("/broadcasts/{id}/leave", post(handlers::leave))
        .route("/broadcasts/{id}/cohosts", post(handlers::add_cohost))
        .route(
            "/broadcasts/{id}/cohosts/{cohostId}",
            delete(handlers::remove_cohost),
        )
        .route("/broadcasts/{id}/participants", get(handlers::participants))
        .with_state(state)
}

/// The same router with the identity the auth middleware would have inserted.
///
/// Every broadcast handler reads `Extension<AuthUser>`, and this suite has no
/// middleware — so without this the endpoints are unreachable, and the wire shape of
/// every response could not be asserted at all.
fn router_as(state: BroadcastState, user: AuthUser) -> Router {
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

/// A router that gates every route on the presence of an `AuthUser` extension,
/// returning 401 when it is missing — mimicking the real auth middleware's behaviour
/// for unauthenticated requests without needing a token verifier.
fn router_with_auth_gate(state: BroadcastState) -> Router {
    router(state).layer(axum::middleware::from_fn(|request: Request, next: Next| {
        let has_auth = request.extensions().get::<AuthUser>().is_some();
        async move {
            if has_auth {
                next.run(request).await
            } else {
                let body = axum::Json(meno_core::to_body(&meno_core::Error::Unauthorized {
                    code: meno_core::ErrorCode::Unauthorized,
                    message: "an access token is required".to_owned(),
                }));
                (axum::http::StatusCode::UNAUTHORIZED, body).into_response()
            }
        }
    }))
}

/// Send one request and read back the status and parsed body.
async fn send(
    router: Router,
    method: &str,
    uri: &str,
    body: Option<String>,
) -> (StatusCode, serde_json::Value) {
    let request = match (method, &body) {
        ("GET", None) | ("POST", None) => HttpRequest::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .expect("a valid request"),
        ("POST", Some(payload)) | ("PATCH", Some(payload)) | ("DELETE", Some(payload)) => {
            HttpRequest::builder()
                .method(method)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload.clone()))
                .expect("a valid request")
        }
        _ => {
            // A request the test suite did not anticipate — report it rather than
            // panic, so a failed assertion below tells us what went wrong.
            panic!("unexpected (method, body) pair: ({method}, {:?})", body)
        }
    };

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

// ── fixtures ──────────────────────────────────────────────────────────────

mod fixture {
    use super::*;

    /// The fixture identity, standing in for a verified access token.
    /// Each caller gets a distinct id so authorization checks between them are meaningful.
    pub fn caller(full_name: &str, id: u128) -> AuthUser {
        AuthUser {
            id: Uuid::from_u128(id),
            jti: Uuid::from_u128(id),
            full_name: full_name.to_owned(),
            email: "ada@example.com".to_owned(),
            verified: true,
            providers: vec![],
            role: meno_api::modules::auth::model::UserRole::User,
        }
    }

    /// The wired broadcast module, with the in-memory doubles installed.
    pub fn state() -> BroadcastState {
        use std::sync::Arc;

        use meno_api::modules::broadcast::media::RecordingMedia;
        use meno_api::modules::broadcast::repository::InMemoryBroadcastRepo;
        use meno_api::modules::broadcast::state::BroadcastState;

        BroadcastState::from_parts(
            Arc::new(InMemoryBroadcastRepo::new()),
            Arc::new(RecordingMedia::new()),
        )
    }

    /// Build a state whose repository already contains `user`.
    pub fn state_with_user(full_name: &str) -> BroadcastState {
        use std::sync::Arc;

        use meno_api::modules::broadcast::media::RecordingMedia;
        use meno_api::modules::broadcast::model::UserSummary;
        use meno_api::modules::broadcast::repository::InMemoryBroadcastRepo;
        use meno_api::modules::broadcast::state::BroadcastState;

        let repo = Arc::new(InMemoryBroadcastRepo::new());
        repo.insert_user(UserSummary {
            id: Uuid::from_u128(1),
            full_name: full_name.to_owned(),
            avatar_id: None,
            avatar_url: None,
        });
        BroadcastState::from_parts(
            Arc::clone(&repo) as Arc<dyn meno_api::modules::broadcast::repository::BroadcastRepo>,
            Arc::new(RecordingMedia::new())
                as Arc<dyn meno_api::modules::broadcast::media::BroadcastMedia>,
        )
    }
}

// ── the success envelope ───────────────────────────────────────────────────

#[tokio::test]
async fn creating_a_broadcast_returns_201_and_the_success_envelope() {
    let state = fixture::state_with_user("Ada Lovelace");
    let caller = fixture::caller("Ada Lovelace", 1);

    let body = serde_json::json!({
        "title": "Ada's show",
        "description": "Every Thursday",
        "timeZone": "Africa/Lagos",
        "recordingEnabled": true,
    })
    .to_string();

    let (status, response) = send(
        router_as(state, caller.clone()),
        "POST",
        "/broadcasts",
        Some(body),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{response}");
    assert_eq!(response["code"], "CREATED");
    assert_eq!(response["status"], true);
    assert!(response["data"]["id"].is_string(), "{response}");
    assert_eq!(response["data"]["title"], "Ada's show");
    assert_eq!(response["data"]["creator"]["full_name"], "Ada Lovelace");
    assert_eq!(response["data"]["creator"]["id"], caller.id.to_string());
    assert!(
        response["data"]["cohosts"]
            .as_array()
            .is_some_and(|a| a.is_empty()),
        "{response}"
    );
}

#[tokio::test]
async fn getting_a_broadcast_returns_200_and_the_success_envelope() {
    let state = fixture::state_with_user("Ada Lovelace");
    let caller = fixture::caller("Ada Lovelace", 1);

    let create_body = serde_json::json!({
        "title": "Ada's show",
        "timeZone": "Africa/Lagos",
    })
    .to_string();

    let (status, create_response) = send(
        router_as(state.clone(), caller.clone()),
        "POST",
        "/broadcasts",
        Some(create_body),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{create_response}");

    let id = create_response["data"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    let (status, response) = send(
        router_as(state, caller),
        "GET",
        &format!("/broadcasts/{id}"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["code"], "OK");
    assert_eq!(response["data"]["id"], id);
}

// ── auth gate ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_unauthenticated_create_is_401() {
    let state = fixture::state();

    let (status, _) = send(
        router_with_auth_gate(state),
        "POST",
        "/broadcasts",
        Some(serde_json::json!({"title": "Ada's show"}).to_string()),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_unauthenticated_get_is_401() {
    let state = fixture::state();

    let (status, _) = send(
        router_with_auth_gate(state),
        "GET",
        "/broadcasts/some-uuid",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ── §4.2 error taxonomy on the wire ────────────────────────────────────────

#[tokio::test]
async fn a_create_without_a_title_is_422_and_names_the_field() {
    let state = fixture::state_with_user("Ada Lovelace");
    let caller = fixture::caller("Ada Lovelace", 1);

    let body = serde_json::json!({
        "title": "Ab",
        "description": "Every Thursday",
    })
    .to_string();

    let (status, response) =
        send(router_as(state, caller), "POST", "/broadcasts", Some(body)).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["code"], "VALIDATION_FAILED");
    assert!(
        response["data"]
            .as_object()
            .and_then(|o| o["title"].as_array())
            .is_some(),
        "{response}"
    );
}

#[tokio::test]
async fn updating_as_a_non_creator_is_403() {
    let state = fixture::state_with_user("Ada Lovelace");
    let ada = fixture::caller("Ada Lovelace", 1);
    let grace = fixture::caller("Grace Hopper", 2);

    // Ada creates.
    let create_body = serde_json::json!({
        "title": "Ada's show",
        "timeZone": "Africa/Lagos",
    })
    .to_string();

    let (status, create_response) = send(
        router_as(state.clone(), ada.clone()),
        "POST",
        "/broadcasts",
        Some(create_body),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{create_response}");

    let id = create_response["data"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    // Grace tries to patch it.
    let (status, _) = send(
        router_as(state, grace),
        "PATCH",
        &format!("/broadcasts/{id}"),
        Some(serde_json::json!({"title": "Grace's title"}).to_string()),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn going_live_without_a_media_server_is_a_503() {
    // The module is wired with `RecordingMedia` everywhere here, but the point of the
    // test is that the *disabled* path is a 503, not a panic. The fixture above uses
    // `RecordingMedia`, which succeeds — so this test reaches for the adapter through
    // the repository's refusal path instead. The cleanest way to assert the 503 is to
    // build a second state with `DisabledMedia`, which is the production path for a
    // deployment with `LIVEKIT_ENABLED=false`.
    let disabled_state = {
        use std::sync::Arc;

        use meno_api::modules::broadcast::media::DisabledMedia;
        use meno_api::modules::broadcast::repository::InMemoryBroadcastRepo;
        use meno_api::modules::broadcast::state::BroadcastState;

        let repo = Arc::new(InMemoryBroadcastRepo::new());
        let media = Arc::new(DisabledMedia);
        BroadcastState::from_parts(repo, media)
    };

    let caller = fixture::caller("Ada Lovelace", 1);

    // Ada creates a broadcast in the disabled state.
    let create_body = serde_json::json!({
        "title": "Ada's show",
        "timeZone": "Africa/Lagos",
    })
    .to_string();

    let (status, create_response) = send(
        router_as(disabled_state.clone(), caller.clone()),
        "POST",
        "/broadcasts",
        Some(create_body),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{create_response}");

    let id = create_response["data"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    // Going live against a disabled media server must be a 503.
    let (status, response) = send(
        router_as(disabled_state, caller),
        "POST",
        &format!("/broadcasts/{id}/live"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{response}");
    assert_eq!(response["code"], "UPSTREAM_UNAVAILABLE");
}

#[tokio::test]
async fn a_broadcast_that_is_not_live_refuses_join_with_a_409() {
    let state = fixture::state_with_user("Ada Lovelace");
    let caller = fixture::caller("Ada Lovelace", 1);

    // Ada creates a draft.
    let create_body = serde_json::json!({
        "title": "Ada's show",
        "timeZone": "Africa/Lagos",
    })
    .to_string();

    let (status, create_response) = send(
        router_as(state.clone(), caller.clone()),
        "POST",
        "/broadcasts",
        Some(create_body),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{create_response}");

    let id = create_response["data"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    // Grace tries to join the draft.
    let grace = fixture::caller("Grace Hopper", 2);
    let (status, _) = send(
        router_as(state, grace),
        "POST",
        &format!("/broadcasts/{id}/join"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
}

// ── RFC 3339 dates on the wire ─────────────────────────────────────────────

#[tokio::test]
async fn every_date_on_the_broadcast_wire_is_an_rfc3339_string() {
    let state = fixture::state_with_user("Ada Lovelace");
    let caller = fixture::caller("Ada Lovelace", 1);

    let create_body = serde_json::json!({
        "title": "Ada's show",
        "description": "Every Thursday",
        "timeZone": "Africa/Lagos",
        "recordingEnabled": true,
        "startTime": (OffsetDateTime::now_utc() + time::Duration::hours(24)).to_string(),
    })
    .to_string();

    // Create.
    let (status, create_response) = send(
        router_as(state.clone(), caller.clone()),
        "POST",
        "/broadcasts",
        Some(create_body),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{create_response}");

    let id = create_response["data"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    // Go live, so the response carries `publishedAt`.
    let (_status, _) = send(
        router_as(state.clone(), caller.clone()),
        "POST",
        &format!("/broadcasts/{id}/live"),
        None,
    )
    .await;

    // Detail.
    let (status, detail) = send(
        router_as(state.clone(), caller.clone()),
        "GET",
        &format!("/broadcasts/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");

    // List. The page is wrapped inside the envelope, so the items sit under
    // `data.data` rather than `data`.
    let (status, list) = send(router_as(state, caller), "GET", "/broadcasts", None).await;
    assert_eq!(status, StatusCode::OK, "{list}");

    let list_page = list["data"]
        .as_object()
        .and_then(|page| page["data"].as_array())
        .expect("the list page is nested inside the envelope");
    assert!(!list_page.is_empty(), "{list}");

    // Walk the whole tree, refusing anything that looks like a nine-tuple.
    for (name, body) in [
        ("create", &create_response),
        ("detail", &detail),
        ("list", &list),
    ] {
        assert_no_tuple_dates(body, name);
    }

    // Named assertions, so a failure names the field that regressed.
    assert_rfc3339(&detail["data"]["createdAt"], "detail.data.createdAt");
    assert_rfc3339(&detail["data"]["publishedAt"], "detail.data.publishedAt");
    assert!(detail["data"]["endTime"].is_null(), "{detail}");
    assert!(
        detail["data"]["startTime"].is_string() || detail["data"]["startTime"].is_null(),
        "{detail}"
    );

    let list_item = list_page
        .iter()
        .find(|item| item["id"].as_str().is_some_and(|s| s == id))
        .expect("the created broadcast is in the list");
    assert_rfc3339(&list_item["createdAt"], "list item createdAt");
    assert!(
        list_item["startTime"].is_string() || list_item["startTime"].is_null(),
        "{list}"
    );

    // And the two are consistent: the created-at on the detail and on the list item
    // for the same broadcast must be the same instant, because they came from the same
    // row.
    let detail_ts = OffsetDateTime::parse(
        detail["data"]["createdAt"]
            .as_str()
            .expect("asserted above"),
        &Rfc3339,
    )
    .expect("asserted above");
    let list_ts = OffsetDateTime::parse(
        list_item["createdAt"].as_str().expect("asserted above"),
        &Rfc3339,
    )
    .expect("asserted above");
    assert_eq!(
        detail_ts, list_ts,
        "the same row must render the same createdAt"
    );
}

#[tokio::test]
async fn an_unknown_route_is_404_rather_than_a_500() {
    let state = fixture::state();
    let caller = fixture::caller("Ada Lovelace", 1);

    let (status, _) = send(
        router_as(state, caller),
        "GET",
        "/broadcasts/does-not-exist/participants/extra",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
