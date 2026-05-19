use crate::shared::middleware::auth::auth_middleware;
use crate::shared::middleware::rate_limit::with_rate_limit;
use crate::shared::middleware::require_verified::require_verified;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use std::sync::Arc;

pub mod auth;
pub mod health;
pub mod user;

pub fn build_meno_routes(state: Arc<MenoState>) -> Router<Arc<MenoState>> {
    let public_routes = Router::new()
        .route("/health", get(health::health_handler))
        .nest("/api/v1/auth", auth::router())
        .layer(with_rate_limit(10, 60));

    let protected_routes = Router::new()
        .nest("/api/v1/users", user::router())
        .layer(with_rate_limit(25, 60))
        .layer(from_fn_with_state(state.clone(), auth_middleware))
        .layer(from_fn_with_state(state.clone(), require_verified));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state)
}
