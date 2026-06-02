use crate::modules::broadcast::handlers as h;
use crate::shared::middleware::auth::auth_middleware;
use crate::shared::middleware::idempotency::idempotency_middleware;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{delete, get, patch, post, put};

pub fn router(state: std::sync::Arc<MenoState>) -> Router<std::sync::Arc<MenoState>> {
    let normal = Router::new()
        .route("/", get(h::get_broadcasts))
        .route("/{id}", get(h::get_broadcast))
        .route("/{id}/participants", get(h::get_participants))
        .route("/{id}/live-participants", get(h::get_live_participants))
        .layer(from_fn_with_state(state.clone(), auth_middleware));

    let idempotent = Router::new()
        .route("/", post(h::create_broadcast))
        .route("/{id}", patch(h::update_broadcast))
        .route("/{id}", delete(h::delete_broadcast))
        .route("/{id}/start", put(h::go_live))
        .route("/{id}/end", delete(h::end_broadcast))
        .route("/{id}/join", post(h::join_broadcast))
        .route("/{id}/leave", post(h::leave_broadcast))
        .route("/{id}/cohosts", post(h::add_cohost))
        .route("/{id}/cohosts/{user_id}", delete(h::remove_cohost))
        .route("/{id}/token", post(h::refresh_token))
        .layer(from_fn(idempotency_middleware))
        .layer(from_fn_with_state(state.clone(), auth_middleware));

    normal.merge(idempotent)
}
