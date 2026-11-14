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
//! # What this binary does
//!
//! It calls the same [`bootstrap::build_context`] as the web binary, under
//! [`AppRole::Worker`] — so both processes apply the same migrations and connect to the
//! same pool — and then runs [`JobRunner`]: auth retention, notes retention and the
//! broadcast abandonment sweep.
//!
//! The HTTP surface is a health probe and nothing else. It binds the same `PORT` the web
//! service reads, because the two are separate services with their own port assignment;
//! readiness here means "the process is up and its loops started", which is all a
//! platform can check without reading the tables.

use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Context as _;
use axum::Router;
use axum::routing::get;
use tokio::net::TcpListener;

use meno_api::bootstrap::{self, AppRole};
use meno_api::infrastructure::signals::shutdown_signal;
use meno_api::jobs::JobRunner;
use meno_api::state::MenoState;

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

    run(context.config.port, context.state).await
}

/// Serve the health probe and own the job runner's lifetime.
///
/// The runner is a task rather than a loop awaited here, because axum's graceful
/// shutdown owns the process's stop: when `shutdown_signal` resolves the server finishes,
/// this function aborts the runner, and its loops drop with it. Awaiting the runner
/// directly would leave the health socket unserved, and the platform could never tell
/// the difference between a working worker and a wedged one.
async fn run(port: u16, state: MenoState) -> anyhow::Result<()> {
    let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    let listener = TcpListener::bind(address)
        .await
        .map_err(|e| anyhow::anyhow!("worker failed to bind {address}: {e}"))
        .context("the worker health probe could not bind")?;

    let jobs = tokio::spawn(JobRunner::new(state).run());

    tracing::info!(port, "meno-worker listening; background jobs started");

    let served = axum::serve(listener, health_router())
        .with_graceful_shutdown(shutdown_signal())
        .await;

    jobs.abort();

    served.context("the worker health server exited with an error")?;

    tracing::info!("meno-worker shut down cleanly");
    Ok(())
}

/// The worker's only route.
///
/// A bare 200 rather than a readiness check that queries Postgres: the worker's useful
/// signal is that its process is alive, and a probe that opened a pool connection every
/// ten seconds would be a load source of its own on the free tier.
fn health_router() -> Router {
    Router::new().route("/health", get(|| async { "ok" }))
}
