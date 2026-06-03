use meno_api::{
    shared::signals::shutdown_signal,
    shared::services::redis::{RedisConfig, RedisService},
    database::create_postgres_pool,
    config::MenoConfig,
    shared::telemetry::init_telemetry,
    state::build_meno_router
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let provider = &jsonwebtoken::crypto::aws_lc::DEFAULT_PROVIDER;
    let _ = jsonwebtoken::crypto::CryptoProvider::install_default(provider);

    init_telemetry();

    let config = MenoConfig::from_env()?;

    let env = config.env.clone();
    let port = config.port;
    let db = create_postgres_pool(config.database_url.as_str()).await;
    let redis = RedisService::new(RedisConfig::from_url(config.redis_url.clone())).await?;

    let router = build_meno_router(config, db, redis).await;

    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!(port, env = %env, "Meno API starting");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server shutdown cleanly");
    Ok(())
}
