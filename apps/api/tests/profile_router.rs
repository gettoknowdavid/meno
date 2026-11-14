//! §10 **contract** layer: the `/users` and `/settings` routers, driven end to end
//! through axum.
//!
//! # Why this suite exists
//!
//! `profile_service.rs` and `settings_service.rs` assert on the *rules*; these assert
//! on the *wire*. A handler can be correct in isolation and still answer 200 where
//! the contract says 401, return `full_name` where the client expects a
//! `FieldErrors` entry, or emit `time`'s nine-number date tuple instead of an RFC
//! 3339 string. Nothing below the handler would catch any of that.
//!
//! # Built the way `build_routes` builds it
//!
//! Unlike `broadcast_router.rs` — which registers absolute paths on the root — this
//! suite mounts the two nests exactly as [`meno_api::routes::build_routes`] does:
//! relative paths inside, `nest("/users", …)` and `nest("/settings", …)` outside.
//! That is deliberate. The mount is where axum's prefix semantics live, and prefix
//! semantics are exactly what once made `GET /broadcasts` answer 404 while every
//! test in the broadcast suite stayed green — so these tests exercise the nest too,
//! not just the handlers behind it.
//!
//! # What this cannot prove
//!
//! No Postgres, no Redis, no rate limiting: the doubles are the in-memory
//! repositories and cache, and the guard is a layer that checks for the identity
//! extension rather than verifying a JWT. The real verifier, the limiter, and the
//! SQL are covered by `auth_router`'s live-wiring notes, `middleware`'s own tests,
//! and `profile_repository_pg.rs` respectively.

// Panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml. The allow-*-in-tests keys cover #[test] functions; the helpers below
// are not #[test] functions, so the allowance does not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
// Each test uses a different subset of the fixture's doubles; the fields exist so a
// test can reach the one it needs without the others warning as dead.
#![allow(dead_code)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{Request as HttpRequest, StatusCode, header};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use http_body_util::BodyExt;
use meno_api::infrastructure::storage::InMemoryObjectStore;
use meno_api::middleware::auth::AuthUser;
use meno_api::modules::profile::cache::{InMemoryProfileCache, ProfileCache};
use meno_api::modules::profile::handlers as profile_handlers;
use meno_api::modules::profile::model::Profile;
use meno_api::modules::profile::repository::{InMemoryProfileRepo, ProfileRepo};
use meno_api::modules::profile::state::ProfileState;
use meno_api::modules::profile::storage::ProfileStorage;
use meno_api::modules::settings::handlers as settings_handlers;
use meno_api::modules::settings::repository::{InMemorySettingsRepo, SettingsRepo};
use meno_api::modules::settings::state::SettingsState;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tower::ServiceExt;
use uuid::Uuid;

// ── building the router ────────────────────────────────────────────────────

/// Both mounts, wired the way `build_routes` wires them: relative paths inside a
/// nest that carries the prefix.
fn router(fixture: &Fixture) -> Router {
    let profile = Router::new()
        .route("/", get(profile_handlers::search))
        .route(
            "/me",
            get(profile_handlers::get_me).patch(profile_handlers::update_me),
        )
        .route("/me/avatar", post(profile_handlers::upload_avatar))
        .route("/{id}", get(profile_handlers::get_profile))
        .with_state(fixture.profile.clone());

    let settings = Router::new()
        .route(
            "/",
            get(settings_handlers::get_settings).patch(settings_handlers::update_settings),
        )
        .route("/push-token", delete(settings_handlers::clear_push_token))
        .with_state(fixture.settings.clone());

    Router::new()
        .nest("/users", profile)
        .nest("/settings", settings)
}

/// The same router with the identity the auth middleware would have inserted.
fn router_as(fixture: &Fixture, user: AuthUser) -> Router {
    router(fixture).layer(axum::middleware::from_fn(
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
/// returning 401 when it is missing — the real guard's behaviour for an anonymous
/// request, without needing a token verifier.
fn router_with_auth_gate(fixture: &Fixture) -> Router {
    router(fixture).layer(axum::middleware::from_fn(|request: Request, next: Next| {
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

// ── sending requests ───────────────────────────────────────────────────────

async fn send(
    router: Router,
    method: &str,
    uri: &str,
    body: Option<String>,
) -> (StatusCode, serde_json::Value) {
    let request = match (method, &body) {
        ("GET", None) | ("POST", None) | ("DELETE", None) => HttpRequest::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .expect("a valid request"),
        ("POST", Some(payload)) | ("PATCH", Some(payload)) => HttpRequest::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(payload.clone()))
            .expect("a valid request"),
        _ => panic!("unexpected (method, body) pair: ({method}, {body:?})"),
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

/// Send one multipart upload — one `file` part, the shape `handlers::upload_avatar`
/// reads — and parse the answer.
async fn send_upload(
    router: Router,
    uri: &str,
    filename: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> (StatusCode, serde_json::Value) {
    let boundary = "meno-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let request = HttpRequest::builder()
        .method("POST")
        .uri(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .expect("a valid request");

    let response = router.oneshot(request).await.expect("a response");
    let status = response.status();
    let parsed = response
        .into_body()
        .collect()
        .await
        .expect("the body buffers")
        .to_bytes();

    (
        status,
        serde_json::from_slice(&parsed).unwrap_or(serde_json::Value::Null),
    )
}

// ── date guards, copied from the shape `broadcast_router.rs` established ────

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
    let parsed = OffsetDateTime::parse(text, &Rfc3339)
        .unwrap_or_else(|e| panic!("`{path}` = {text:?} is not RFC 3339: {e}"));
    assert!(
        parsed > OffsetDateTime::UNIX_EPOCH,
        "`{path}` = {text:?} parsed, but is not a real instant"
    );
}

// ── fixtures ───────────────────────────────────────────────────────────────

/// Both module states, with the doubles still concrete underneath.
struct Fixture {
    profile: ProfileState,
    settings: SettingsState,
    repo: Arc<InMemoryProfileRepo>,
    settings_rows: Arc<InMemorySettingsRepo>,
    cache: Arc<InMemoryProfileCache>,
}

fn fixture() -> Fixture {
    let repo = Arc::new(InMemoryProfileRepo::new());
    let settings_rows = Arc::new(InMemorySettingsRepo::new());
    let cache = Arc::new(InMemoryProfileCache::new());

    let profile = ProfileState::from_parts(
        Arc::clone(&repo) as Arc<dyn ProfileRepo>,
        Arc::clone(&settings_rows) as Arc<dyn SettingsRepo>,
        Arc::clone(&cache) as Arc<dyn ProfileCache>,
        ProfileStorage::new(Arc::new(InMemoryObjectStore::new("https://cdn.test"))),
    );
    let settings = SettingsState::from_parts(
        Arc::clone(&settings_rows) as Arc<dyn SettingsRepo>,
        Arc::clone(&cache) as Arc<dyn ProfileCache>,
    );

    Fixture {
        profile,
        settings,
        repo,
        settings_rows,
        cache,
    }
}

/// The fixture identity, standing in for a verified access token.
fn caller(full_name: &str, id: u128) -> AuthUser {
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

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(seconds).expect("a plausible fixture instant")
}

fn seed(f: &Fixture, id: u128, full_name: &str, created_at: OffsetDateTime) {
    f.repo.insert_user(Profile {
        id: Uuid::from_u128(id),
        full_name: full_name.to_owned(),
        email: format!("{full_name}@example.com"),
        verified: true,
        bio: Some("Notes on the analytical engine".to_owned()),
        avatar_id: None,
        avatar_url: None,
        followers: 3,
        following: 1,
        broadcasts: 2,
        created_at,
    });
}

fn png() -> Vec<u8> {
    [
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01, 0x42,
    ]
    .to_vec()
}

// ── the auth gate ──────────────────────────────────────────────────────────

#[tokio::test]
async fn every_endpoint_is_401_without_an_identity() {
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_000));

    for (method, uri, body) in [
        ("GET", "/users", None),
        ("GET", "/users/me", None),
        (
            "PATCH",
            "/users/me",
            Some(r#"{"full_name":"Ada"}"#.to_owned()),
        ),
        ("POST", "/users/me/avatar", None),
        ("GET", "/users/00000000-0000-0000-0000-000000000001", None),
        ("GET", "/settings", None),
        (
            "PATCH",
            "/settings",
            Some(r#"{"language":"fr"}"#.to_owned()),
        ),
        ("DELETE", "/settings/push-token", None),
    ] {
        let (status, _) = send(router_with_auth_gate(&f), method, uri, body).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must not be reachable anonymously"
        );
    }
}

// ── GET /users/me ──────────────────────────────────────────────────────────

#[tokio::test]
async fn me_returns_the_success_envelope_with_rfc3339_dates() {
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_000));

    let (status, response) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "GET",
        "/users/me",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["code"], "OK");
    assert_eq!(response["status"], true);
    assert!(response["data"]["full_name"].is_string(), "{response}");

    let data = &response["data"];
    assert_eq!(data["full_name"], "Ada Lovelace");
    assert_eq!(data["email_verified"], true);
    assert_eq!(data["followers"], 3, "counters come from the columns");
    assert_eq!(data["settings"]["language"], "en");
    assert_eq!(data["settings"]["push_notifications"], true);
    assert!(data["providers"].is_array(), "{response}");

    assert_no_tuple_dates(&response, "me");
    assert_rfc3339(&data["created_at"], "me.data.created_at");
}

// ── PATCH /users/me: §4.2 on the wire ──────────────────────────────────────

#[tokio::test]
async fn a_patch_with_a_short_name_is_422_and_names_the_field() {
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_000));

    let (status, response) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "PATCH",
        "/users/me",
        Some(r#"{"full_name":"A"}"#.to_owned()),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["code"], "VALIDATION_FAILED");
    assert!(
        response["data"]
            .as_object()
            .and_then(|o| o["full_name"].as_array())
            .is_some(),
        "the failing field must be named: {response}"
    );
}

#[tokio::test]
async fn a_bio_longer_than_the_column_is_422_not_a_500() {
    // `master` accepted any length and let `VARCHAR(244)` refuse it with a 500.
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_000));

    let body = serde_json::json!({ "bio": "x".repeat(245) }).to_string();
    let (status, response) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "PATCH",
        "/users/me",
        Some(body),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["code"], "VALIDATION_FAILED");
    assert!(response["data"]["bio"].is_array(), "{response}");
}

#[tokio::test]
async fn an_empty_patch_is_422_rather_than_a_round_trip_that_changes_nothing() {
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_000));

    let (status, response) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "PATCH",
        "/users/me",
        Some("{}".to_owned()),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["code"], "VALIDATION_FAILED");
}

#[tokio::test]
async fn a_valid_patch_is_persisted_and_answered_with_the_fresh_record() {
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_000));

    let (status, response) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "PATCH",
        "/users/me",
        Some(r#"{"full_name":"Ada Byron","bio":"Mathematician"}"#.to_owned()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["data"]["full_name"], "Ada Byron");
    assert_eq!(response["data"]["bio"], "Mathematician");

    // And the *next* read sees it — through the cache the write repaired.
    let (status, response) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "GET",
        "/users/me",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["data"]["full_name"], "Ada Byron");
}

// ── GET /users/{id} ────────────────────────────────────────────────────────

#[tokio::test]
async fn a_strangers_profile_is_public_and_a_missing_one_is_404() {
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_000));

    let (status, response) = send(
        router_as(&f, caller("Grace Hopper", 2)),
        "GET",
        "/users/00000000-0000-0000-0000-000000000001",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["data"]["full_name"], "Ada Lovelace");
    assert_eq!(response["data"]["is_following"], false);
    assert!(
        response["data"].get("email").is_none(),
        "a public profile never exposes an address: {response}"
    );
    assert_no_tuple_dates(&response, "profile");
    assert_rfc3339(&response["data"]["created_at"], "profile.data.created_at");

    let (status, response) = send(
        router_as(&f, caller("Grace Hopper", 2)),
        "GET",
        "/users/00000000-0000-0000-0000-000000000099",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{response}");
    assert_eq!(response["code"], "NOT_FOUND");
}

// ── GET /users?q= ──────────────────────────────────────────────────────────

#[tokio::test]
async fn search_returns_a_cursor_page_and_pages_forward() {
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_100));
    seed(&f, 2, "Adam Smith", at(1_700_000_000));
    seed(&f, 3, "Grace Hopper", at(1_700_000_200));

    let identity = caller("Ada Lovelace", 9);

    let (status, page_one) = send(
        router_as(&f, identity.clone()),
        "GET",
        "/users?q=ada&limit=1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page_one}");
    assert_eq!(page_one["code"], "OK");

    // The page sits *inside* the envelope: `data` holds the cursor page.
    let items = page_one["data"]["data"].as_array().expect("a page of rows");
    assert_eq!(items.len(), 1, "{page_one}");
    assert_eq!(items[0]["full_name"], "Ada Lovelace");
    assert_eq!(page_one["data"]["hasNextPage"], true);
    let cursor = page_one["data"]["nextCursor"]
        .as_str()
        .expect("a cursor to the next page")
        .to_owned();
    assert_no_tuple_dates(&page_one, "search");
    assert_rfc3339(&items[0]["created_at"], "search.data[0].created_at");

    let (status, page_two) = send(
        router_as(&f, identity),
        "GET",
        &format!("/users?q=ada&limit=1&cursor={cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page_two}");
    let items = page_two["data"]["data"].as_array().expect("a page of rows");
    assert_eq!(items.len(), 1, "{page_two}");
    assert_eq!(
        items[0]["full_name"], "Adam Smith",
        "the cursor advanced the page"
    );
    assert_eq!(page_two["data"]["hasNextPage"], false);
}

#[tokio::test]
async fn a_search_term_shorter_than_the_minimum_is_422_and_names_q() {
    let f = fixture();

    let (status, response) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "GET",
        "/users?q=a",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["code"], "VALIDATION_FAILED");
    assert!(response["data"]["q"].is_array(), "{response}");

    // An *absent* term is the same failure in the same envelope — without the
    // `#[serde(default)]` on `q`, this second request was axum's transport-level
    // 400 with a plain-text body, i.e. two shapes for one mistake.
    let (status, response) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "GET",
        "/users",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["code"], "VALIDATION_FAILED");
    assert!(response["data"]["q"].is_array(), "{response}");
}

#[tokio::test]
async fn a_garbage_cursor_is_a_400_not_a_silent_first_page() {
    // `master`'s cursor was dead code: an unusable value came back as page one
    // forever. Here it must fail loudly instead.
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_100));

    let (status, _) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "GET",
        "/users?q=ada&cursor=not-a-cursor",
        None,
    )
    .await;

    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "a malformed cursor must be refused, got {status}"
    );
}

// ── POST /users/me/avatar ──────────────────────────────────────────────────

#[tokio::test]
async fn an_avatar_upload_round_trips_and_then_replaces() {
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_000));

    let (status, response) = send_upload(
        router_as(&f, caller("Ada Lovelace", 1)),
        "/users/me/avatar",
        "avatar.png",
        "image/png",
        png(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["code"], "OK");
    assert_eq!(
        response["data"]["avatar_id"],
        "avatars/00000000-0000-0000-0000-000000000001.png"
    );
    assert_eq!(
        response["data"]["avatar_url"],
        "https://cdn.test/avatars/00000000-0000-0000-0000-000000000001.png"
    );

    // The profile screen now shows it — through the cache the upload dropped.
    let (status, me) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "GET",
        "/users/me",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(
        me["data"]["avatar_url"],
        "https://cdn.test/avatars/00000000-0000-0000-0000-000000000001.png"
    );

    // A replacement with another extension produces another key.
    let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    jpg.extend_from_slice(&[0x01; 4]);
    let (status, response) = send_upload(
        router_as(&f, caller("Ada Lovelace", 1)),
        "/users/me/avatar",
        "avatar.jpg",
        "image/jpeg",
        jpg,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(
        response["data"]["avatar_id"],
        "avatars/00000000-0000-0000-0000-000000000001.jpg"
    );
}

#[tokio::test]
async fn an_upload_that_is_not_an_image_is_422_with_the_file_field() {
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_000));

    let (status, response) = send_upload(
        router_as(&f, caller("Ada Lovelace", 1)),
        "/users/me/avatar",
        "avatar.png",
        "image/png",
        b"<html>not an image</html>".to_vec(),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["code"], "VALIDATION_FAILED");
    assert!(response["data"]["file"].is_array(), "{response}");
}

// ── /settings, and the contract across the two mounts ──────────────────────

#[tokio::test]
async fn settings_default_patch_and_the_embedded_block_stay_in_step() {
    let f = fixture();
    seed(&f, 1, "Ada Lovelace", at(1_700_000_000));
    let identity = caller("Ada Lovelace", 1);

    // Fresh account: defaults, not 404 (`master` answered SETTINGS_NOT_FOUND here).
    let (status, response) = send(router_as(&f, identity.clone()), "GET", "/settings", None).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["code"], "OK");
    assert_eq!(response["data"]["push_notifications"], true);
    assert_eq!(response["data"]["language"], "en");
    assert_eq!(response["data"]["has_push_token"], false);
    assert!(
        response["data"].get("push_notification_token").is_none(),
        "the raw device token is never echoed: {response}"
    );

    // Save a language and a token.
    let (status, response) = send(
        router_as(&f, identity.clone()),
        "PATCH",
        "/settings",
        Some(
            r#"{"language":"fr","push_notification_token":"fcm-token","push_notifications":false}"#
                .to_owned(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["data"]["language"], "fr");
    assert_eq!(response["data"]["has_push_token"], true);
    assert_eq!(response["data"]["push_notifications"], false);

    // The other mount sees it *immediately* — `/users/me` embeds settings behind
    // the cache the PATCH invalidated. Two modules, one contract.
    let (status, me) = send(router_as(&f, identity.clone()), "GET", "/users/me", None).await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["data"]["settings"]["language"], "fr");
    assert_eq!(me["data"]["settings"]["has_push_token"], true);
    assert_eq!(me["data"]["settings"]["push_notifications"], false);

    // Clearing the token lands in both responses too.
    let (status, response) = send(
        router_as(&f, identity.clone()),
        "DELETE",
        "/settings/push-token",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["code"], "OK");
    assert!(
        response.get("data").is_none(),
        "no_content omits the payload rather than sending null: {response}"
    );

    let (status, me) = send(router_as(&f, identity), "GET", "/users/me", None).await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["data"]["settings"]["has_push_token"], false);
}

#[tokio::test]
async fn an_unsupported_language_is_422_and_an_empty_settings_patch_too() {
    let f = fixture();

    let (status, response) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "PATCH",
        "/settings",
        Some(r#"{"language":"english"}"#.to_owned()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["code"], "VALIDATION_FAILED");
    assert!(response["data"]["language"].is_array(), "{response}");

    let (status, response) = send(
        router_as(&f, caller("Ada Lovelace", 1)),
        "PATCH",
        "/settings",
        Some("{}".to_owned()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["code"], "VALIDATION_FAILED");
}

// ── the mount itself ───────────────────────────────────────────────────────

#[tokio::test]
async fn an_unknown_route_under_both_mounts_is_404() {
    // The nest's own contract: paths the table does not list must not answer, and
    // paths it does list must not be double-prefixed into invisibility.
    let f = fixture();
    let identity = caller("Ada Lovelace", 1);

    for uri in [
        "/users/nope/extra",
        "/users/me/avatar/extra",
        "/settings/nope",
    ] {
        let (status, _) = send(router_as(&f, identity.clone()), "GET", uri, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} is not in the table");
    }
}
