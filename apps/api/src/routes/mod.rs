use crate::shared::middleware::rate_limit::with_rate_limit;
use crate::state::MenoState;
use axum::Router;
use axum::routing::get;
use std::sync::Arc;

pub mod auth;
pub mod health;
pub mod profile;

pub fn build_meno_routes(state: Arc<MenoState>) -> Router<Arc<MenoState>> {
    Router::new()
        .route("/health", get(health::health_handler))
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/users", profile::router(state.clone()))
        .layer(with_rate_limit(25, 60))
}
