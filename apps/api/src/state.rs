use crate::config::MenoConfig;
use crate::modules::auth::jwt_service::JwtService;
use crate::modules::auth::services::AuthService;
use crate::routes::build_meno_routes;
use std::sync::Arc;

use crate::modules::profile::service::ProfileService;
use crate::shared::background_jobs::BackgroundJobs;
use crate::shared::integrations::google::GoogleAuthService;
use crate::shared::middleware::timing::timing_middleware;
use crate::shared::services::redis::RedisService;
use crate::shared::services::storage::StorageService;
use axum::middleware::from_fn;
use axum::{
    Router,
    http::{StatusCode, header},
    routing::get,
};
use axum_prometheus::PrometheusMetricLayer;
use sqlx::PgPool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
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
    pub redis: RedisService,
    pub jwt: JwtService,
    pub google: GoogleAuthService,
    pub storage: StorageService,
    pub background_jobs: Arc<BackgroundJobs>,
    pub auth_service: AuthService,
    pub profile_service: ProfileService,
}

pub async fn build_app_router(config: MenoConfig, db: PgPool, redis: RedisService) -> Router {
    let allowed_origins: Vec<_> = config.origins.iter().map(|o| o.parse().unwrap()).collect();

    let jwt = JwtService::new(
        &config.jwt_secret,
        &config.jwt_refresh_secret,
        config.access_token_expiration,
        config.refresh_token_expiration,
    );

    let cancel_token = CancellationToken::new();
    let background_jobs = Arc::new(BackgroundJobs::new(db.clone(), redis.clone(), &config.env));

    let storage = StorageService::new(&config);

    let state = Arc::new(MenoState {
        auth_service: AuthService::new(db.clone(), redis.clone(), &config.env),
        profile_service: ProfileService::new(db.clone(), redis.clone(), storage.clone()),
        background_jobs: background_jobs.clone(),
        google: GoogleAuthService::new(&config),
        storage,
        jwt,
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

    BackgroundJobs::start(background_jobs.clone(), cancel_token.clone());
    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    Router::new()
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .layer(prometheus_layer)
        .merge(build_meno_routes(state.clone()))
        .layer(from_fn(timing_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(middleware_stack)
        .with_state(state)
}
