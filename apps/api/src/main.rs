use meno_api::config::MenoConfig;
use meno_api::database::create_postgres_pool;
use meno_api::shared::services::redis::{RedisService};
use meno_api::shared::signals::shutdown_signal;
use meno_api::shared::telemetry::init_telemetry;
use meno_api::state::build_app_router;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_telemetry();

    let config = MenoConfig::from_env()?;

    let env = config.env.clone();
    let port = config.port;

    let db_pool = create_postgres_pool(config.database_url.as_str()).await;
    let redis = RedisService::new(&config.redis_url).await?;
    let router = build_app_router(config, db_pool, redis).await;

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!(port = port, env = %env, "Meno API is starting");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server shutdown cleanly");

    Ok(())
}
