use crate::modules::broadcast::handlers::{
    create_broadcast, end_broadcast, go_live, update_broadcast,
};
use crate::shared::middleware::auth::auth_middleware;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, patch, post, put};
use std::sync::Arc;

pub fn router(state: Arc<MenoState>) -> Router<Arc<MenoState>> {
    Router::new()
        .route("/", post(create_broadcast))
        .route("/{id}", patch(update_broadcast))
        .route("/{id}/go-live", put(go_live))
        .route("/{id}/end", delete(end_broadcast))
        .route_layer(from_fn_with_state(state.clone(), auth_middleware))
}
