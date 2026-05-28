use crate::modules::broadcast::handlers as h;
use crate::shared::middleware::auth::auth_middleware;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, patch, post, put};
use std::sync::Arc;

pub fn router(state: Arc<MenoState>) -> Router<Arc<MenoState>> {
    Router::new()
        .route("/", post(h::create_broadcast))
        .route("/{id}", patch(h::update_broadcast))
        .route("/{id}", delete(h::delete_broadcast))
        .route("/{id}/go-live", put(h::go_live))
        .route("/{id}/end", delete(h::end_broadcast))
        .route_layer(from_fn_with_state(state.clone(), auth_middleware))
}
