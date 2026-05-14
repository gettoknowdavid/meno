use crate::state::MenoState;
use axum::Router;
use axum::routing::get;
use std::sync::Arc;

pub mod health;

pub fn build_meno_routes(state: Arc<MenoState>) -> Router<Arc<MenoState>> {
    // let auth_routes = Router::new()
    //     .layer(with_rate_limit(10, 60))
    //     .with_state(state.clone());
    //
    // let broadcast_routes = Router::new()
    //     .layer(with_rate_limit(25, 60))
    //     .with_state(state.clone());

    Router::new()
        .route("/health", get(health::health_handler))
        .with_state(state)
}
