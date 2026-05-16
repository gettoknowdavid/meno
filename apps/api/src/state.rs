use crate::config::MenoConfig;
use crate::modules::auth::jwt_service::JwtService;
use crate::modules::auth::services::AuthService;
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
use moka::future::Cache;
use sqlx::PgPool;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    cors::{AllowHeaders, AllowOrigin, Any, CorsLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct MenoState {
    pub config: MenoConfig,
    pub db: PgPool,
    pub redis: Pool,
    pub jwt: JwtService,
    pub local_rate_cache: Cache<String, u64>,
    pub auth_service: AuthService,
}

pub async fn build_app_router(config: MenoConfig, db: PgPool, redis: Pool) -> Router {
    let allowed_origins: Vec<_> = config.origins.iter().map(|o| o.parse().unwrap()).collect();

    let jwt = JwtService::new(
        &config.jwt_secret,
        &config.jwt_refresh_secret,
        config.access_token_expiration,
        config.refresh_token_expiration,
    );

    let local_rate_cache = Cache::builder()
        .max_capacity(100_000)
        .time_to_live(Duration::from_secs(60))
        .build();

    let state = std::sync::Arc::new(MenoState {
        auth_service: AuthService::new(db.clone(), redis.clone()),
        jwt,
        local_rate_cache,
        config,
        db,
        redis,
    });

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
