//! The Meno HTTP API.
//!
//! Step 3.6 of the implementation guide declares two binaries from this one crate:
//! `meno-api` (this file) and `meno-worker` (`src/worker.rs`). Same image, same
//! dependencies, different entry point — so a Render deploy runs the web service and
//! the job worker from one build with one set of secrets.
//!
//! # Current state
//!
//! The refactor is landing module by module, so this binary currently boots the pieces
//! that exist and serves the health endpoints. `bootstrap`, `state` and `routes` arrive
//! with the remaining steps and will replace [`build_router`]. Everything here is the
//! part that has to be right first: configuration is validated, telemetry is
//! installed, Redis is connected, and the process shuts down gracefully on the signals
//! a container platform actually sends.

use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Context;

use axum::Router;
use axum::routing::get;
use tokio::net::TcpListener;

use meno_api::config::Config;
use meno_api::infrastructure::redis::{Redis, RedisConfig};
use meno_api::infrastructure::signals::shutdown_signal;
use meno_api::infrastructure::telemetry;

/// The HTTP server entry point.
///
/// # Errors
///
/// Returns an error if the config is invalid, Redis cannot be reached, or the port
/// cannot be bound. Each is returned with context rather than panicking — plan §4.3
/// requires the process to log clearly and exit, never to abort mid-write.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Config first: the log filter comes from it, so there is nothing useful to print
    // until it has been read.
    let config = Config::load()?;
    telemetry::init(&config);

    tracing::info!(config = %config.summary(), "starting meno-api");

    // Connecting here rather than lazily means a misconfigured Redis is a startup
    // failure with a clear message, not a 500 on the first request that needs it.
    // The context matters: fred's own error is a bare `IO Error` with no indication of
    // which URL it was trying, and `REDIS_URL` is the single most common thing to get
    // wrong on a fresh deployment.
    let redis_url = config.redis_url.expose().to_owned();
    let redis = Redis::new(RedisConfig::from_url(redis_url.clone()))
        .await
        .with_context(|| format!("failed to connect to Redis at {redis_url}"))?;

    let app = build_router(&config, redis);

    let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, config.port));
    let listener = TcpListener::bind(address)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bind {address}: {e}"))?;

    tracing::info!(port = config.port, env = %config.env, "meno-api listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("meno-api shut down cleanly");
    Ok(())
}

/// The application router.
///
/// Serves the health endpoints until `bootstrap`/`state`/`routes` land; those replace
/// this body. The signature already takes the pieces they will need, so the swap is a
/// change to this function only.
fn build_router(_config: &Config, _redis: Redis) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/health/ready", get(|| async { "ready" }))
}
