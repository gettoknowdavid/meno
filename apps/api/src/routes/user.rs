use crate::modules::user::handlers;
use crate::state::MenoState;
use axum::Router;
use axum::routing::{get, patch};

pub fn router() -> Router<std::sync::Arc<MenoState>> {
    Router::new()
        .route("/me", get(handlers::get_me))
        .route("/me", patch(handlers::update_me))
        .route("/me/avatar-upload-url", get(handlers::get_avatar_upload_url))
}
