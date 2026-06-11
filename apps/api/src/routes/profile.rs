use crate::modules::profile::handlers as h;
use crate::shared::middleware::auth::auth_middleware;
use crate::shared::middleware::idempotency::idempotency_middleware;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{get, patch};
use std::sync::Arc;

pub fn router(app: Arc<MenoState>) -> Router<Arc<MenoState>> {
    let normal = Router::new()
        .route("/me", get(h::get_me))
        .route("/{id}", get(h::get_profile))
        .route("/", get(h::search_profiles))
        .layer(from_fn_with_state(app.clone(), auth_middleware));

    let idempotent = Router::new()
        .route("/me", patch(h::update_me))
        .route("/me/avatar-upload-url", get(h::get_avatar_upload_url))
        .layer(from_fn_with_state(app.clone(), auth_middleware))
        .layer(from_fn(idempotency_middleware));

    normal.merge(idempotent)
}
