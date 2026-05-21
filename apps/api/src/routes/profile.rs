use axum::middleware::from_fn_with_state;
use crate::modules::profile::handlers;
use crate::state::MenoState;
use axum::Router;
use axum::routing::{get, patch};
use crate::shared::middleware::auth::auth_middleware;

pub fn router(state: std::sync::Arc<MenoState>) -> Router<std::sync::Arc<MenoState>> {
    Router::new()
        .route("/me", get(handlers::get_me))
        .route("/me", patch(handlers::update_me))
        .route("/me/avatar-upload-url", get(handlers::get_avatar_upload_url))
        .route("/{id}", get(handlers::get_profile))
        .route("/", get(handlers::search_profiles))
        .route_layer(from_fn_with_state(state.clone(), auth_middleware))
}
