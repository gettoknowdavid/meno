//! The notes and folders endpoints, with their layers attached.
//!
//! Two collections (`/notes`, `/folders`) in one file because they are one module: a note
//! and its folder share a repository, a service and a state, and splitting the table would
//! split a boundary that does not exist.
//!
//! # Why this is merged rather than nested twice
//!
//! The paths are written out in full and mounted at the root with `.merge`, like
//! [`super::broadcasts`]. Nesting `/notes` and `/folders` separately would work but would
//! mean two `with_state` lifts and two path lists for one module — and `nest(\"/\")` is
//! refused by axum 0.8, so the root mount is `.merge` either way.
//!
//! # Layers
//!
//! The same shape as broadcast: `LimitPolicy::read()` on the reads,
//! `LimitPolicy::write()` plus §4.5's idempotency middleware on the writes. Idempotency
//! matters most on `POST /notes/sync` — a retried push must be answered from the stored
//! response rather than re-entering the batch — and on every create, since the client
//! chooses the id and a retry would otherwise be a duplicate-key 500.

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, patch, post, put};

use crate::infrastructure::redis::Redis;
use crate::middleware::auth::{AuthState, auth_middleware};
use crate::middleware::idempotency::idempotency_middleware;
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::notes::handlers;
use crate::modules::notes::state::NotesState;

/// Every path this table serves, written out.
///
/// Written out rather than derived from the router, because a list derived from the thing
/// it is meant to check cannot fail. The route-table test in `crate::routes` asserts
/// against it.
pub const NOTES_PATHS: &[&str] = &[
    "/notes",
    "/notes/00000000-0000-0000-0000-000000000000",
    "/notes/00000000-0000-0000-0000-000000000000/folders/00000000-0000-0000-0000-000000000000",
    "/notes/sync",
    "/folders",
    "/folders/00000000-0000-0000-0000-000000000000",
];

/// The notes/folders sub-router, before [`super::build_routes`] lifts it to
/// [`NotesState`].
pub fn routes(guard: AuthState, redis: Arc<Redis>) -> Router<NotesState> {
    let reads = Router::new()
        .route("/notes", get(handlers::get_notes))
        .route("/notes/sync", get(handlers::sync_pull))
        .route("/folders", get(handlers::get_folders))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::read()),
            rate_limit_middleware,
        ));

    // Idempotency innermost, so a retried write is answered from the stored response
    // rather than re-entering the service.
    let writes = Router::new()
        .route("/notes", post(handlers::create_note))
        .route("/notes", put(handlers::move_notes_to_folder))
        .route("/notes/sync", post(handlers::sync_push))
        .route("/notes/{id}", patch(handlers::update_note))
        .route("/notes/{id}", delete(handlers::delete_note))
        .route(
            "/notes/{noteId}/folders/{folderId}",
            put(handlers::add_note_to_folder),
        )
        .route(
            "/notes/{noteId}/folders/{folderId}",
            delete(handlers::remove_note_from_folder),
        )
        .route("/folders", post(handlers::create_folder))
        .route("/folders/{id}", patch(handlers::update_folder))
        .route("/folders/{id}", delete(handlers::delete_folder))
        .layer(from_fn_with_state(
            Arc::clone(&redis),
            idempotency_middleware,
        ))
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::write()),
            rate_limit_middleware,
        ));

    reads
        .merge(writes)
        .layer(from_fn_with_state(guard, auth_middleware))
}
