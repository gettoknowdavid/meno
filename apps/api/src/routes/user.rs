use crate::modules::user::handlers;
use crate::state::MenoState;
use axum::Router;
use axum::routing::get;

pub fn router() -> Router<std::sync::Arc<MenoState>> {
    Router::new().route("/me", get(handlers::get_me))
}
