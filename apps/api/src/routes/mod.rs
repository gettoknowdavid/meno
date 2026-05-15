use crate::shared::middleware::auth::auth_middleware;
use crate::shared::middleware::rate_limit::with_rate_limit;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use std::sync::Arc;

pub mod health;

pub fn build_meno_routes(state: Arc<MenoState>) -> Router<Arc<MenoState>> {
    let protected_routes = Router::new()
        .layer(with_rate_limit(25, 60))
        .layer(from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .route("/health", get(health::health_handler))
        .merge(protected_routes)
        .with_state(state)
}
