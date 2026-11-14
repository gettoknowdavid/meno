//! The Meno HTTP API.
//!
//! Step 3.6 of the implementation guide declares two binaries from this one crate:
//! `meno-api` (this file) and `meno-worker` (`src/worker.rs`). Same image, same
//! dependencies, different entry point — so a Render deploy runs the web service and
//! the job worker from one build with one set of secrets.
//!
//! # What this binary does now
//!
//! [`bootstrap::build_context`] assembles configuration, telemetry, the Postgres pool
//! with its migrations, Redis and the wired module state; this file binds a socket,
//! mounts [`routes::build_routes`] and serves until a shutdown signal. The wiring that
//! used to live here — twenty-odd lines of pool creation, optional-adapter selection and
//! `?`-on-everything — is now [`bootstrap`](meno_api::bootstrap), shared with the worker
//! so the two cannot drift.
//!
//! It does **not** start the Apalis monitor. §6 is explicit that it must not, and
//! [`AppRole::Web`](meno_api::bootstrap::AppRole::Web) is the guard that makes the
//! omission deliberate rather than an oversight.

use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Context as _;
use tokio::net::TcpListener;

use meno_api::bootstrap::{self, AppRole};
use meno_api::infrastructure::signals::shutdown_signal;
use meno_api::routes;

/// The HTTP server entry point.
///
/// # Errors
///
/// Returns an error if the config is invalid, Postgres cannot be reached or migrated,
/// Redis cannot be reached, the push, storage or Google credentials are unusable, the
/// auth module cannot be wired, or the port cannot be bound. Each is returned with
/// context rather than panicking — plan §4.3 requires the process to log clearly and
/// exit, never to abort mid-write.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let context = bootstrap::build_context(AppRole::Web).await?;

    let port = context.config.port;
    let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    let listener = TcpListener::bind(address)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bind {address}: {e}"))?;

    tracing::info!(port, env = %context.config.env, "meno-api listening");

    axum::serve(listener, routes::build_routes(context.state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("the HTTP server exited with an error")?;

    tracing::info!("meno-api shut down cleanly");
    Ok(())
}
