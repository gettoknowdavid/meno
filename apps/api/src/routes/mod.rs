use crate::shared::middleware::rate_limit::with_rate_limit;
use crate::state::MenoState;
use axum::Router;
use std::sync::Arc;

pub mod auth;
pub mod broadcast;
pub mod health;
pub mod profile;

pub mod subscribers;

pub fn build_meno_routes(state: Arc<MenoState>) -> Router<Arc<MenoState>> {
    Router::new()
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/users", profile::router(state.clone()))
        .nest("/api/v1/broadcasts", broadcast::router(state.clone()))
        .nest("/api/v1/subscribers", subscribers::router(state.clone()))
        .layer(with_rate_limit(25, 60))
}
