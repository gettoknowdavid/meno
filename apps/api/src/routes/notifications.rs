use crate::modules::notifications::handlers as h;
use crate::shared::middleware::auth::auth_middleware;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, patch};
use std::sync::Arc;

pub fn router(app: Arc<MenoState>) -> Router<Arc<MenoState>> {
    Router::new()
        .route("/", get(h::get_notifications))
        .route("/unread-count", get(h::get_unread_count))
        .route("/read-all", patch(h::mark_all_read))
        .route("/:id/read", patch(h::mark_read))
        .route("/:id", delete(h::delete_notification))
        .layer(from_fn_with_state(app, auth_middleware))
}
