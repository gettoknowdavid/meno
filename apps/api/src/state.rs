use crate::{
    config::Config,
    jobs::Jobs,
    modules::{
        auth::state::AuthState, broadcast::state::BroadcastState, chat::state::ChatState,
        notifications::state::NotificationState, profile::state::ProfileState,
        subscribers::state::SubscribersState,
    },
    routes::{build_meno_routes, health},
    shared::{
        middleware::timing::timing_middleware,
        services::{
            livekit::LivekitService,
            push::PushNotificationService,
            redis::Redis,
            storage::StorageService,
            ws::{WsService, pubsub::WsPubSubBridge},
        },
    },
};
use axum::{
    Router,
    http::{HeaderName, Request, StatusCode, header},
    middleware::from_fn,
    routing::get,
};
use axum_prometheus::PrometheusMetricLayer;
use lettre::{AsyncSmtpTransport, Tokio1Executor};
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

/// Top-level application state.
///
/// Every field is an `Arc`-wrapped module state or shared service.
/// No field is mutable after startup.
/// No handler or service takes `&MenoState` — they receive their own
/// module state via `app.auth`, `app.broadcast`, etc.
#[derive(Clone)]
pub struct MenoState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub redis: Redis,
    pub auth: AuthState,
    pub profile: ProfileState,
    pub broadcast: BroadcastState,
    pub subscribers: SubscribersState,
    pub notifications: NotificationState,
    pub chat: ChatState,
    pub ws: WsService,
    pub pubsub: Arc<WsPubSubBridge>,
    pub jobs: Jobs,
    pub smtp: AsyncSmtpTransport<Tokio1Executor>,
}

pub async fn build_meno_router(config: Config, db: PgPool, redis: Redis) -> Router {
    let config = Arc::new(config);

    let storage = StorageService::new(&config);
    let jobs = Jobs::new(&db);
    let livekit = build_livekit(&config);
    let smtp = build_smtp(&config);
    let push = PushNotificationService::new(&config);
    let ws = WsService::new(redis.clone());

    let bridge = build_ws_pubsub_bridge(&config, ws.clone(), redis.clone()).await;
    let pubsub = Arc::new(bridge);

    let auth = AuthState::new(db.clone(), redis.clone(), &config, jobs.clone());
    let profile = ProfileState::new(db.clone(), redis.clone(), storage.clone());
    let broadcast = BroadcastState::new(
        db.clone(),
        redis.clone(),
        livekit.clone(),
        pubsub.clone(),
        ws.clone(),
        jobs.clone(),
    );
    let subscribers = SubscribersState::new(db.clone());
    let notifications = NotificationState::new(db.clone(), redis.clone(), push.clone());
    let chat = ChatState::new(db.clone(), redis.clone());

    let state = Arc::new(MenoState {
        auth,
        profile,
        broadcast,
        subscribers,
        notifications,
        chat,
        ws,
        pubsub,
        jobs,
        smtp,
        redis,
        config,
        db: db.clone(),
    });

    start_background_workers(&state.db, Arc::clone(&state));
    build_middleware_stack(state)
}

async fn build_ws_pubsub_bridge(config: &Config, ws: WsService, redis: Redis) -> WsPubSubBridge {
    let bridge = WsPubSubBridge::build(&config, ws, redis)
        .await
        .expect("WsPubSubBridge failed to initialise");
    bridge.spawn_subscriber_loop();
    bridge
}

fn build_livekit(config: &Config) -> LivekitService {
    LivekitService::new(
        config,
        Arc::new(RoomClient::with_api_key(
            &config.livekit_host,
            &config.livekit_api_key,
            &config.livekit_api_secret,
        )),
    )
}

fn build_smtp(config: &Config) -> AsyncSmtpTransport<Tokio1Executor> {
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::transport::smtp::client::Tls;
    let creds = Credentials::new(config.smtp_user.clone(), config.smtp_password.clone());
    AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)
        .unwrap()
        .port(config.smtp_port)
        .credentials(creds)
        .tls(Tls::None)
        .build()
}

fn build_cors(config: &Config) -> CorsLayer {
    let origins: Vec<_> = config.origins.iter().map(|o| o.parse().unwrap()).collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(Any)
        .allow_headers(AllowHeaders::list([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::ORIGIN,
            "X-Requested-With".parse().unwrap(),
        ]))
}

fn build_middleware_stack(state: Arc<MenoState>) -> Router {
    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();
    let cors = build_cors(&state.config);
    let x_req_id = HeaderName::from_static("x-request-id");

    Router::<Arc<MenoState>>::new()
        .route(
            "/metrics",
            get(move || async move { metric_handle.render() }),
        )
        .route("/health", get(health::health_handler))
        .route(
            "/ws",
            get(crate::shared::services::ws::handlers::ws_upgrade),
        )
        .layer(prometheus_layer)
        .merge(build_meno_routes(state.clone()))
        .layer(from_fn(timing_middleware))
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::new(x_req_id.clone(), MakeRequestUuid))
                .layer(PropagateRequestIdLayer::new(x_req_id))
                .layer(TraceLayer::new_for_http().make_span_with(|r: &Request<_>| {
                    let rid = r
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("unknown");
                    tracing::info_span!(
                        "http_request",
                        request_id = %rid,
                        method     = %r.method(),
                        uri        = %r.uri(),
                        user_id    = tracing::field::Empty,
                    )
                }))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    Duration::from_secs(30),
                ))
                .layer(cors),
        )
        .with_state(state)
}

fn start_background_workers(db: &PgPool, state: Arc<MenoState>) {
    let pool = db.clone();
    let s = Arc::clone(&state);
    tokio::spawn(async move {
        if let Err(e) = crate::jobs::monitor::run_monitor(pool, s).await {
            tracing::error!(error = %e, "Apalis monitor exited unexpectedly");
        }
    });

    let pool = db.clone();
    tokio::spawn(crate::jobs::monitor::schedule_cleanup_job(pool));
}
