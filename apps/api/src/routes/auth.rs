use crate::modules::auth::handlers;
use crate::state::MenoState;
use axum::{Router, routing::post};

pub fn router() -> Router<std::sync::Arc<MenoState>> {
    Router::new().route("/register", post(handlers::register))
}
