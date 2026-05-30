use crate::modules::broadcast::handlers as h;
use crate::shared::middleware::auth::auth_middleware;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, patch, post, put};
use std::sync::Arc;

pub fn router(state: Arc<MenoState>) -> Router<Arc<MenoState>> {
    Router::new()
        .route("/", post(h::create_broadcast))
        .route("/", get(h::get_broadcasts))
        .route("/{id}", get(h::get_broadcast))
        .route("/{id}", patch(h::update_broadcast))
        .route("/{id}", delete(h::delete_broadcast))
        .route("/{id}/start", put(h::go_live))
        .route("/{id}/end", delete(h::end_broadcast))
        .route("/{id}/join", post(h::join_broadcast))
        .route("/{id}/leave", post(h::leave_broadcast))
        .route("/{id}/cohosts", post(h::add_cohost))
        .route("/{id}/cohosts/{user_id}", delete(h::add_cohost))
        .route("/{id}/participants", get(h::get_participants))
        .route("/{id}/live-participants", get(h::get_live_participants))
        .route("/{id}/token", post(h::refresh_token))
        .route_layer(from_fn_with_state(state.clone(), auth_middleware))
}
