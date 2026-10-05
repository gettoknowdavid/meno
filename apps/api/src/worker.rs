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
//! # What this binary does now
//!
//! It calls the same [`bootstrap::build_context`] as the web binary, under
//! [`AppRole::Worker`]. That is the point of the module: both processes apply the same
//! migrations and connect to the same pool, so a job can never be enqueued against a
//! schema version the worker has not migrated to.
//!
//! `jobs/` arrives with a later step. Until then this waits for a shutdown signal,
//! which is what keeps the graceful-shutdown path exercised today.

use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Context as _;
use tokio::net::TcpListener;

use meno_api::bootstrap::{self, AppRole};
use meno_api::infrastructure::signals::shutdown_signal;

/// The worker entry point.
///
/// # Errors
///
/// Returns an error if the config is invalid, Postgres cannot be reached or migrated,
/// Redis cannot be reached, or the health socket cannot be bound.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let context = bootstrap::build_context(AppRole::Worker).await?;

    tracing::info!(env = %context.config.env, "meno-worker ready");

    run().await
}

/// The worker loop.
///
/// Stands in for the job runner until `jobs/` lands. Binds a socket only so the process
/// looks alive to a platform health check; the loop itself just waits for a shutdown
/// signal.
async fn run() -> anyhow::Result<()> {
    let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0));
    let _probe = TcpListener::bind(address)
        .await
        .map_err(|e| anyhow::anyhow!("worker failed to bind a health socket: {e}"))
        .context("the worker health probe could not bind")?;

    shutdown_signal().await;

    tracing::info!("meno-worker shut down cleanly");
    Ok(())
}
