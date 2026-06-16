use crate::modules::auth::state::AuthState;
use crate::modules::broadcast::state::BroadcastState;
use crate::modules::chat::state::ChatState;
use crate::modules::notifications::state::NotificationState;
use crate::modules::profile::state::ProfileState;
use crate::modules::subscribers::state::SubscribersState;
use crate::routes::health;
use crate::shared::services::push::PushNotificationService;
use crate::shared::services::ws;
use crate::shared::services::ws::pubsub::WsPubSubBridge;
use crate::{
    config::MenoConfig,
    jobs::{JobQueue, monitor},
    routes::build_meno_routes,
    shared::middleware::timing::timing_middleware,
    shared::services::livekit::LivekitService,
    shared::services::redis::RedisService,
    shared::services::storage::StorageService,
    shared::services::ws::WsService,
};
use axum::{
    Router,
    http::{HeaderName, Request},
    http::{StatusCode, header},
    middleware::from_fn,
    routing::get,
};
use axum_prometheus::PrometheusMetricLayer;
use lettre::{
    AsyncSmtpTransport, Tokio1Executor, transport::smtp::authentication::Credentials,
    transport::smtp::client::Tls,
};
use livekit_api::services::room::RoomClient;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tower::ServiceBuilder;
use tower_http::{
    cors::{AllowHeaders, AllowOrigin, Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct MenoState {
    pub config: Arc<MenoConfig>,
    pub db: PgPool,
    pub redis: RedisService,
    pub auth: AuthState,
    pub profile: ProfileState,
    pub broadcast: BroadcastState,
    pub subscribers: SubscribersState,
    pub notifications: NotificationState,
    pub chat: ChatState,
    pub livekit: LivekitService,
    pub ws: WsService,
    pub pubsub: Arc<WsPubSubBridge>,
    pub jobs: JobQueue,
    pub smtp: AsyncSmtpTransport<Tokio1Executor>,
}

pub async fn build_meno_router(config: MenoConfig, db: PgPool, redis: RedisService) -> Router {
    let config = Arc::new(config);

    let storage = StorageService::new(&config);
    let jobs = JobQueue::new(&db);
    let livekit = build_livekit_service(&config);
    let smtp = build_smtp_transport(&config);
    let push = PushNotificationService::new(&config);
    let ws = WsService::new(redis.clone());
    let bridge = WsPubSubBridge::build(&config, ws.clone(), redis.clone())
        .await
        .expect("Failed to build WS pub/sub bridge");

    // Spawn the receive loop before any request can arrive.
    bridge.spawn_subscriber_loop();

    let state = Arc::new(MenoState {
        auth: AuthState::new(db.clone(), redis.clone(), &config),
        profile: ProfileState::new(db.clone(), redis.clone(), storage.clone()),
        broadcast: BroadcastState::new(db.clone(), redis.clone(), livekit.clone(), ws.clone()),
        subscribers: SubscribersState::new(db.clone(), ws.clone()),
        notifications: NotificationState::new(db.clone(), redis.clone(), ws.clone(), push.clone()),
        chat: ChatState::new(db.clone(), redis.clone()),
        pubsub: Arc::new(bridge),
        livekit,
        ws,
        jobs,
        smtp,
        redis,
        config,
        db: db.clone(),
    });

    start_background_workers(&db, Arc::clone(&state));
    build_middleware_stack(state)
}

fn build_livekit_service(config: &MenoConfig) -> LivekitService {
    LivekitService::new(
        config,
        Arc::new(RoomClient::with_api_key(
            &config.livekit_host,
            &config.livekit_api_key,
            &config.livekit_api_secret,
        )),
    )
}

fn build_smtp_transport(config: &MenoConfig) -> AsyncSmtpTransport<Tokio1Executor> {
    let creds = Credentials::new(config.smtp_user.clone(), config.smtp_password.clone());
    AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)
        .unwrap()
        .port(config.smtp_port)
        .credentials(creds)
        .tls(Tls::None)
        .build()
}

fn build_cors(config: &MenoConfig) -> CorsLayer {
    let allowed_origins: Vec<_> = config.origins.iter().map(|o| o.parse().unwrap()).collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods(Any)
        .allow_headers(AllowHeaders::list([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::ORIGIN,
            "X-Requested-With".parse().unwrap(),
            "X-User-Id".parse().unwrap(),
        ]))
}

fn build_middleware_stack(state: Arc<MenoState>) -> Router {
    let cors_layer = build_cors(&state.config);
    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let x_req_id = HeaderName::from_static("x-request-id");
    let status_code = StatusCode::REQUEST_TIMEOUT;
    let timeout = Duration::from_secs(30);

    let middleware_stack = ServiceBuilder::new()
        .layer(SetRequestIdLayer::new(x_req_id.clone(), MakeRequestUuid))
        .layer(PropagateRequestIdLayer::new(x_req_id))
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

    Router::new()
        .route(
            "/metrics",
            get(move || async move { metric_handle.render() }),
        )
        .route("/health", get(health::health_handler))
        .route("/ws", get(ws::handlers::ws_upgrade))
        .layer(prometheus_layer)
        .merge(build_meno_routes(state.clone()))
        .layer(from_fn(timing_middleware))
        .layer(middleware_stack)
        .with_state(state)
}

fn start_background_workers(db: &PgPool, state: Arc<MenoState>) {
    let monitor_pool = db.clone();
    let monitor_state = Arc::clone(&state);
    tokio::spawn(async move {
        if let Err(e) = monitor::run_monitor(monitor_pool, monitor_state).await {
            tracing::error!(error = %e, "Apalis monitor exited");
        }
    });

    let cleanup_pool = db.clone();
    tokio::spawn(monitor::schedule_cleanup_job(cleanup_pool));
}
