use crate::shared::middleware::rate_limit::with_rate_limit;
use crate::state::MenoState;
use axum::Router;
use axum::routing::get;
use std::sync::Arc;

pub mod auth;
pub mod health;
pub mod user;

pub fn build_meno_routes(state: Arc<MenoState>) -> Router<Arc<MenoState>> {
    let public_routes = Router::new()
        .route("/health", get(health::health_handler))
        .nest("/api/v1/auth", auth::router());

    // let protected_routes = Router::new()
    //     .nest("/api/v1/users", user::router())
    //     .layer(from_fn_with_state(state.clone(), auth_middleware))
    //     .layer(with_rate_limit(25, 60));

    Router::new()
        .merge(public_routes)
        .nest("/api/v1/users", user::router(state.clone()))
        .layer(with_rate_limit(25, 60))
}
