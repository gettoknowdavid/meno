use crate::modules::broadcast::handlers::{create_broadcast, end_broadcast, go_live};
use crate::state::MenoState;
use axum::Router;
use axum::routing::{delete, post, put};
use std::sync::Arc;
use axum::middleware::from_fn_with_state;
use crate::shared::middleware::auth::auth_middleware;

pub fn router(state: Arc<MenoState>) -> Router<Arc<MenoState>> {
    Router::new()
        .route("/", post(create_broadcast))
        .route("/{id}/go-live", put(go_live))
        .route("/{id}/end", delete(end_broadcast))
        .route_layer(from_fn_with_state(state.clone(), auth_middleware))
}
