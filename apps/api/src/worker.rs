//! The Meno background job worker.
//!
//! Step 3.6's second binary. Same crate, same build, same secrets as `meno-api`, but a
//! different entry point so Render can run the web service and the job queue as two
//! services from one image.
//!
//! # Why a separate binary rather than a flag
//!
//! A `--worker` flag on the web binary would mean one process doing both, so a job
//! backlog could starve request handling of the replica's CPU — and on Render's free
//! tier there is one shared pool for both. Splitting them lets each service scale on
//! its own axis.
//!
//! # Current state
//!
//! `jobs/` arrives with a later step. This binary boots config, telemetry and Redis —
//! so the wiring is exercised and the process has correct shutdown behaviour — and
//! then waits. The job loop replaces the wait.

use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Context;

use tokio::net::TcpListener;

use meno_api::config::Config;
use meno_api::infrastructure::redis::{Redis, RedisConfig};
use meno_api::infrastructure::signals::shutdown_signal;
use meno_api::infrastructure::telemetry;

/// The worker entry point.
///
/// # Errors
///
/// Returns an error if the config is invalid or Redis cannot be reached.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    telemetry::init(&config);

    tracing::info!(config = %config.summary(), "starting meno-worker");

    let redis_url = config.redis_url.expose().to_owned();
    let redis = Redis::new(RedisConfig::from_url(redis_url.clone()))
        .await
        .with_context(|| format!("failed to connect to Redis at {redis_url}"))?;

    tracing::info!(env = %config.env, "meno-worker ready");

    run(redis).await
}

/// The worker loop.
///
/// Stands in for the job runner until `jobs/` lands. Binds a socket only so the process
/// looks alive to a platform health check; the loop itself just waits for a shutdown
/// signal, which is what keeps the graceful-shutdown path tested today.
async fn run(_redis: Redis) -> anyhow::Result<()> {
    let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0));
    let _probe = TcpListener::bind(address)
        .await
        .map_err(|e| anyhow::anyhow!("worker failed to bind a health socket: {e}"))?;

    shutdown_signal().await;

    tracing::info!("meno-worker shut down cleanly");
    Ok(())
}
