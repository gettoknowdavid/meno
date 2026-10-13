//! §10 **contract** layer: the notes/folders router, driven end to end through axum.
//!
//! `notes_service.rs` asserts on the *service*; these assert on the *wire* — the §4.2
//! envelope, camelCase keys, RFC 3339 dates, and the status codes the contract promises.
//!
//! The router is built by hand with the in-memory double, so the suite is hermetic;
//! `routes::notes::NOTES_PATHS` exists so `routes/mod.rs` keeps it in step with the real
//! table by hand.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{Method, Request as HttpRequest, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::routing::{delete, get, patch, post, put};
use http_body_util::BodyExt;
use meno_api::middleware::auth::AuthUser;
use meno_api::modules::auth::model::UserRole;
use meno_api::modules::notes::handlers;
use meno_api::modules::notes::model::{Folder, Note};
use meno_api::modules::notes::repository::{InMemoryNotesRepo, NotesRepo};
use meno_api::modules::notes::state::NotesState;
use serde_json::json;
use time::OffsetDateTime;
use tower::ServiceExt;
use uuid::Uuid;

// ── fixtures ──────────────────────────────────────────────────────────────

fn ada() -> Uuid {
    Uuid::from_u128(0x1)
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

fn instant() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("a plausible instant")
}

fn note(id: u128, folder_id: Option<Uuid>) -> Note {
    Note {
        id: Uuid::from_u128(id),
        title: "Groceries".to_owned(),
        content: "milk".to_owned(),
        pinned: false,
        folder_id,
        creator_id: ada(),
        version: 1,
        created_at: instant(),
        updated_at: instant(),
        deleted_at: None,
    }
}

/// The wired state, with a repo the test can seed.
fn state(repo: Arc<InMemoryNotesRepo>) -> NotesState {
    NotesState::from_parts(Arc::clone(&repo) as Arc<dyn NotesRepo>)
}

/// The notes/folders routes under test, mirroring `routes::notes` for the paths used here.
fn router(state: NotesState) -> Router {
    Router::new()
        .route("/notes", get(handlers::get_notes))
        .route("/notes", post(handlers::create_note))
        .route("/notes", put(handlers::move_notes_to_folder))
        .route("/notes/sync", get(handlers::sync_pull))
        .route("/notes/sync", post(handlers::sync_push))
        .route("/notes/{id}", patch(handlers::update_note))
        .route("/notes/{id}", delete(handlers::delete_note))
        .route(
            "/notes/{noteId}/folders/{folderId}",
            put(handlers::add_note_to_folder).delete(handlers::remove_note_from_folder),
        )
        .route("/folders", get(handlers::get_folders))
        .route("/folders", post(handlers::create_folder))
        .route("/folders/{id}", patch(handlers::update_folder))
        .route("/folders/{id}", delete(handlers::delete_folder))
        .with_state(state)
}

fn router_as(state: NotesState, user: AuthUser) -> Router {
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

fn router_with_auth_gate(state: NotesState) -> Router {
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

// ── the success envelope ───────────────────────────────────────────────────

#[tokio::test]
async fn reading_notes_returns_200_and_the_success_envelope() {
    let repo = Arc::new(InMemoryNotesRepo::new());
    repo.seed_note(note(1, None));

    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::GET,
        "/notes",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["code"], "OK");
    assert_eq!(response["status"], true);
    assert_eq!(response["message"], "Notes retrieved");

    let page = response["data"].as_object().expect("a page object");
    assert_eq!(page["hasNextPage"], json!(false));
    let items = page["data"].as_array().expect("a list");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Groceries");
    assert!(
        items[0].get("folderId").is_some(),
        "camelCase key: {response}"
    );
    assert!(
        items[0]["createdAt"].is_string(),
        "an RFC 3339 string: {response}"
    );
    assert!(
        !response.to_string().contains("created_at"),
        "a snake_case key leaked: {response}"
    );
}

#[tokio::test]
async fn creating_a_note_is_201_and_created() {
    let repo = Arc::new(InMemoryNotesRepo::new());
    let id = Uuid::from_u128(2);

    let (status, response) = send(
        router_as(state(repo.clone()), caller(ada())),
        Method::POST,
        "/notes",
        Some(json!({ "id": id, "title": "Groceries", "content": "milk" })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{response}");
    assert_eq!(response["code"], "CREATED");
    assert_eq!(response["data"]["id"], id.to_string());
    assert_eq!(response["data"]["version"], 1);
    assert_eq!(repo.stored_note_count(), 1);
}

// ── §4.2 error taxonomy on the wire ────────────────────────────────────────

#[tokio::test]
async fn an_unauthenticated_read_is_401() {
    let repo = Arc::new(InMemoryNotesRepo::new());
    let (status, _) = send(
        router_with_auth_gate(state(repo)),
        Method::GET,
        "/notes",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_stale_update_is_409_version_conflict() {
    let repo = Arc::new(InMemoryNotesRepo::new());
    repo.seed_note(note(1, None));

    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::PATCH,
        "/notes/00000000-0000-0000-0000-000000000001",
        Some(json!({ "title": "New", "baseVersion": 99 })),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    assert_eq!(response["code"], "VERSION_CONFLICT");
    assert_eq!(response["status"], false);
}

#[tokio::test]
async fn an_unknown_note_is_404_not_found() {
    let repo = Arc::new(InMemoryNotesRepo::new());

    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::PATCH,
        "/notes/00000000-0000-0000-0000-0000000000ff",
        Some(json!({ "title": "ghost", "baseVersion": 1 })),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{response}");
    assert_eq!(response["code"], "NOT_FOUND");
}

#[tokio::test]
async fn an_undecodable_cursor_is_400_invalid_cursor() {
    let repo = Arc::new(InMemoryNotesRepo::new());
    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::GET,
        "/notes?cursor=not-a-cursor",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["code"], "INVALID_CURSOR");
}

#[tokio::test]
async fn an_over_long_title_is_422_with_a_field_map() {
    let repo = Arc::new(InMemoryNotesRepo::new());
    let (status, response) = send(
        router_as(state(repo), caller(ada())),
        Method::POST,
        "/notes",
        Some(json!({
            "id": Uuid::from_u128(2),
            "title": "t".repeat(101),
            "content": "milk"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["code"], "VALIDATION_FAILED");
    assert!(
        response["data"]["title"].is_array(),
        "per-field detail: {response}"
    );
}

// ── deleting a folder acts on its notes, over HTTP ─────────────────────────

#[tokio::test]
async fn deleting_a_folder_with_should_delete_notes_removes_them_from_the_feed() {
    let repo = Arc::new(InMemoryNotesRepo::new());
    let folder_id = Uuid::from_u128(0x10);
    repo.seed_folder(Folder {
        id: folder_id,
        title: "House".to_owned(),
        pinned: false,
        creator_id: ada(),
        version: 1,
        created_at: instant(),
        updated_at: instant(),
        deleted_at: None,
    });
    repo.seed_note(note(1, Some(folder_id)));

    let (status, response) = send(
        router_as(state(repo.clone()), caller(ada())),
        Method::DELETE,
        &format!("/folders/{folder_id}"),
        Some(json!({ "baseVersion": 1, "shouldDeleteNotes": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");

    // The note is gone from the feed because it was soft-deleted with the folder.
    let (_, feed) = send(
        router_as(state(Arc::clone(&repo)), caller(ada())),
        Method::GET,
        "/notes",
        None,
    )
    .await;
    assert_eq!(
        feed["data"]["data"].as_array().expect("a list").len(),
        0,
        "the note went with the folder: {feed}"
    );
}
