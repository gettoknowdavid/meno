use crate::modules::subscribers::handlers as h;
use crate::shared::middleware::auth::auth_middleware;
use crate::shared::middleware::idempotency::idempotency_middleware;
use crate::state::MenoState;
use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{delete, get, post};

pub fn router(app: std::sync::Arc<MenoState>) -> Router<std::sync::Arc<MenoState>> {
    let normal = Router::new()
        .route("/me/subscribers", get(h::get_my_subscribers))
        .route("/me/subscriptions", get(h::get_my_subscriptions))
        .route("/{id}/subscribers", get(h::get_subscribers))
        .route("/{id}/subscriptions", get(h::get_subscriptions))
        .layer(from_fn_with_state(app.clone(), auth_middleware));

    let idempotent = Router::new()
        .route("/{id}", post(h::subscribe))
        .route("/{id}", delete(h::unsubscribe))
        .layer(from_fn(idempotency_middleware))
        .layer(from_fn_with_state(app.clone(), auth_middleware));

    normal.merge(idempotent)
}
