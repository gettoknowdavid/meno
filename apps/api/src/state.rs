use crate::config::MenoConfig;
use crate::routes::build_meno_routes;
use crate::shared::middleware::rate_limit::rate_limit_middleware;
use axum::{
    Router,
    http::{StatusCode, header},
    middleware::from_fn_with_state,
    routing::get,
};
use axum_prometheus::PrometheusMetricLayer;
use fred::clients::Pool;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tower::ServiceBuilder;
use tower_http::{
    cors::{AllowHeaders, AllowOrigin, Any, CorsLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct MenoState {
    pub config: MenoConfig,
    pub db: Arc<PgPool>,
    pub redis: Arc<Pool>,
}

pub async fn build_app_router(config: MenoConfig, db_pool: PgPool, redis_pool: Pool) -> Router {
    let db = Arc::new(db_pool);
    let redis = Arc::new(redis_pool);

    let allowed_origins: Vec<_> = config.origins.iter().map(|o| o.parse().unwrap()).collect();

    let state = Arc::new(MenoState { config, db, redis });

    let status_code = StatusCode::REQUEST_TIMEOUT;
    let timeout = Duration::from_secs(30);

    let cors_layer = CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods(Any)
        .allow_headers(AllowHeaders::list([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::ORIGIN,
            "X-Requested-With".parse().unwrap(),
            "X-User-Id".parse().unwrap(),
        ]));

    let middleware_stack = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(status_code, timeout))
        .layer(cors_layer);

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    Router::new()
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .layer(prometheus_layer)
        .layer(TraceLayer::new_for_http())
        .merge(build_meno_routes(state.clone()))
        .layer(from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(middleware_stack)
        .with_state(state)
}
