use crate::modules::settings::handlers as h;
use crate::shared::middleware::auth::auth_middleware;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, patch};
use std::sync::Arc;

pub fn router(app: Arc<MenoState>) -> Router<Arc<MenoState>> {
    Router::new()
        .route("/", get(h::get_settings))
        .route("/", patch(h::update_settings))
        .route("/push-token", delete(h::clear_push_token))
        .layer(from_fn_with_state(app, auth_middleware))
}
