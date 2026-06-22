use crate::modules::notes::handlers as h;
use crate::shared::middleware::auth::auth_middleware;
use crate::shared::middleware::idempotency::idempotency_middleware;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{delete, get, patch, post};
use std::sync::Arc;

pub fn router(app: Arc<MenoState>) -> Router<Arc<MenoState>> {
    let normal = Router::new()
        .route("/", get(h::get_folders))
        .layer(from_fn_with_state(app.clone(), auth_middleware));

    let idempotent = Router::new()
        .route("/", post(h::create_folder))
        .route("/{id}", patch(h::update_folder))
        .route("/{id}", delete(h::delete_folder))
        .layer(from_fn(idempotency_middleware))
        .layer(from_fn_with_state(app.clone(), auth_middleware));

    normal.merge(idempotent)
}
