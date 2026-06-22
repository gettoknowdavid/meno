use crate::modules::notes::handlers as h;
use crate::shared::middleware::auth::auth_middleware;
use crate::shared::middleware::idempotency::idempotency_middleware;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{delete, get, patch, post, put};
use std::sync::Arc;

pub fn router(app: Arc<MenoState>) -> Router<Arc<MenoState>> {
    let normal = Router::new()
        .route("/", get(h::get_notes))
        .route("/sync", get(h::sync_pull))
        .layer(from_fn_with_state(app.clone(), auth_middleware));

    let idempotent = Router::new()
        .route("/", post(h::create_note))
        .route("/{id}", patch(h::update_note))
        .route("/{id}", delete(h::delete_note))
        .route("/{n_id}/folders/{f_id}", put(h::add_note_to_folder))
        .route("/{n_id}/folders/{f_id}", delete(h::remove_note_from_folder))
        .route("/", put(h::move_notes_to_folder))
        .route("/sync", post(h::sync_push)) // Idempotency-Key here matters most of all
        .layer(from_fn(idempotency_middleware))
        .layer(from_fn_with_state(app.clone(), auth_middleware));

    normal.merge(idempotent)
}
