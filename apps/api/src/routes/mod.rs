use crate::shared::middleware::rate_limit::with_rate_limit;
use crate::state::MenoState;
use axum::Router;
use std::sync::Arc;

pub mod auth;
pub mod broadcast;
pub mod chat;
pub mod health;
pub mod notes;
pub mod notifications;
pub mod profile;
pub mod subscribers;

pub fn build_meno_routes(app: Arc<MenoState>) -> Router<Arc<MenoState>> {
    Router::new()
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/users", profile::router(app.clone()))
        .nest("/api/v1/broadcasts", broadcast::router(app.clone()))
        .nest("/api/v1/chat", chat::router(app.clone()))
        .nest("/api/v1/subscribers", subscribers::router(app.clone()))
        .nest("/api/v1/notifications", notifications::router(app.clone()))
        .nest("/api/v1/notes", notes::router(app.clone()))
        .layer(with_rate_limit(25, 60))
}
