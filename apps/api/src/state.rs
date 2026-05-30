use crate::config::MenoConfig;
use crate::jobs::{JobQueue, monitor};
use crate::modules::auth::jwt::Jwt;
use crate::modules::auth::services::AuthService;
use crate::modules::broadcast::repository::BroadcastRepository;
use crate::modules::broadcast::service::BroadcastService;
use crate::modules::profile::service::ProfileService;
use crate::routes::build_meno_routes;
use crate::shared::integrations::google::GoogleAuthService;
use crate::shared::middleware::timing::timing_middleware;
use crate::shared::services::livekit::LivekitService;
use crate::shared::services::redis::RedisService;
use crate::shared::services::storage::StorageService;
use crate::shared::services::ws::WsService;
use axum::{
    Router,
    http::{HeaderName, Request},
    http::{StatusCode, header},
    middleware::from_fn,
    routing::get,
};
use axum_prometheus::PrometheusMetricLayer;
use livekit_api::services::room::RoomClient;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower_http::{
    cors::{AllowHeaders, AllowOrigin, Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct MenoState {
    pub config: MenoConfig,
    pub db: PgPool,
    pub redis: RedisService,
    pub jwt: Jwt,
    pub google: GoogleAuthService,
    pub storage: StorageService,
    pub ws: WsService,
    pub livekit: LivekitService,
    pub auth: AuthService,
    pub profile: ProfileService,
    pub broadcast: BroadcastService,
    pub jobs: JobQueue,
}

pub async fn build_app_router(config: MenoConfig, db: PgPool, redis: RedisService) -> Router {
    let allowed_origins: Vec<_> = config.origins.iter().map(|o| o.parse().unwrap()).collect();

    let jwt = Jwt::new(
        &config.jwt_secret,
        &config.jwt_refresh_secret,
        config.access_token_expiration,
        config.refresh_token_expiration,
    );

    let ws = WsService::new(redis.clone());

    let storage = StorageService::new(&config);

    let jobs = JobQueue::new(&db);

    let livekit = LivekitService::new(
        &config,
        Arc::new(RoomClient::with_api_key(
            &config.livekit_host,
            &config.livekit_api_key,
            &config.livekit_api_secret,
        )),
    );

    let broadcast = BroadcastService::new(
        BroadcastRepository::new(db.clone()),
        livekit.clone(),
        redis.clone(),
        ws.clone(),
    );

    let state = Arc::new(MenoState {
        auth: AuthService::new(db.clone(), redis.clone()),
        profile: ProfileService::new(db.clone(), redis.clone(), storage.clone()),
        broadcast,
        livekit,
        ws,
        google: GoogleAuthService::new(&config),
        storage,
        jwt,
        jobs,
        config,
        db: db.clone(),
        redis,
    });

    let monitor_pool = db.clone();
    let monitor_state = Arc::clone(&state);
    tokio::spawn(async move {
        if let Err(e) = monitor::run_monitor(monitor_pool.clone(), monitor_state).await {
            tracing::error!(error = %e, "Apalis monitor exited with error");
        }
    });

    let cleanup_pool = db.clone();
    tokio::spawn(monitor::schedule_cleanup_job(cleanup_pool));

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

    let x_request_id = HeaderName::from_static("x-request-id");

    let middleware_stack = ServiceBuilder::new()
        .layer(SetRequestIdLayer::new(
            x_request_id.clone(),
            MakeRequestUuid,
        ))
        .layer(PropagateRequestIdLayer::new(x_request_id))
        .layer(TraceLayer::new_for_http().make_span_with(|r: &Request<_>| {
            let request_id = r
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("unknown");
            tracing::info_span!(
                "http_request",
                request_id = %request_id,
                method = %r.method(),
                uri = %r.uri(),
            )
        }))
        .layer(TimeoutLayer::with_status_code(status_code, timeout))
        .layer(cors_layer);

    // let middleware_stack = ServiceBuilder::new()
    //     .layer(TraceLayer::new_for_http())
    //     .layer(TimeoutLayer::with_status_code(status_code, timeout))
    //     .layer(cors_layer);

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
