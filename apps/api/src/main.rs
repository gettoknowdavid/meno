use meno_api::config::MenoConfig;
use meno_api::database::create_postgres_pool;
use meno_api::shared::services::redis::RedisService;
use meno_api::shared::signals::shutdown_signal;
use meno_api::state::build_app_router;
use tracing_subscriber::{EnvFilter, fmt, prelude::*, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(fmt::layer().json())
        .init();

    let config = MenoConfig::from_env()?;

    let env = config.env.clone();
    let port = config.port;

    let db_pool = create_postgres_pool(config.database_url.as_str()).await;
    let redis_service = RedisService::new(config.redis_url.as_str()).await?;

    let router = build_app_router(config, db_pool, redis_service.client).await;

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!(port = port, env = %env, "Meno API is starting");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server shutdown cleanly");

    Ok(())
}
