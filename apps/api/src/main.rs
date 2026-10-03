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
use std::sync::Arc;

use anyhow::Context;

use axum::Router;
use axum::routing::get;
use sqlx::PgPool;
use tokio::net::TcpListener;

use meno_api::config::Config;
use meno_api::infrastructure::database::{create_postgres_pool, log_pool_capacity, run_migrations};
use meno_api::infrastructure::oauth::{IdentityProvider, provider_from_config};
use meno_api::infrastructure::push::{PushSender, sender_from_config};
use meno_api::infrastructure::redis::{Redis, RedisConfig};
use meno_api::infrastructure::signals::shutdown_signal;
use meno_api::infrastructure::storage::{ObjectStore, store_from_config};
use meno_api::infrastructure::telemetry;

/// The HTTP server entry point.
///
/// # Errors
///
/// Returns an error if the config is invalid, Postgres cannot be reached or migrated,
/// Redis cannot be reached, the push, storage or Google credentials are unusable, or the
/// port cannot be bound. Each is returned with context rather than panicking — plan §4.3
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
    // Postgres before Redis, and both before anything that needs them. §4.3 is explicit:
    // "Refuse to serve traffic until migrations succeed - but log clearly and exit, never
    // panic!". A `?` here is that exit: `main` returns a non-zero code with a readable
    // message, which is what §7.11 asks for in place of the old `.expect`.
    //
    // §7.2 is the reason this is not optional: the 15 migrations were never applied on
    // `master`, so a fresh database has no schema and every request fails in a way that
    // looks like an application bug.
    let pool = create_postgres_pool(&config).await?;
    run_migrations(&pool, &config).await?;
    log_pool_capacity(&pool);

    let redis_url = config.redis_url.expose().to_owned();
    let redis = Redis::new(RedisConfig::from_url(redis_url.clone()))
        .await
        .with_context(|| format!("failed to connect to Redis at {redis_url}"))?;

    // Push is optional (§4.6), so this cannot fail when `PUSH_ENABLED` is unset — it
    // returns the no-op sender and every notification job logs `Skipped`. When push *is*
    // enabled it can still fail, on an unparseable or non-RSA
    // `FIREBASE_SERVICE_ACCOUNT_JSON`, and that is deliberately fatal: an enabled
    // adapter that cannot sign a JWT would otherwise boot and silently drop every
    // broadcast notification. The context names the variable, because the raw error is
    // about a key rather than about a URL.
    let push = sender_from_config(&config)
        .map_err(|e| anyhow::anyhow!("push is enabled but unusable: {e}"))
        .context("check PUSH_ENABLED and FIREBASE_SERVICE_ACCOUNT_JSON")?;

    // Storage follows the same optional-integration rule as push (§4.6). With
    // `STORAGE_ENABLED` unset it yields the no-op store, and an upload through it is
    // refused with `StorageError::Disabled` rather than silently dropped. With it set,
    // a broken `STORAGE_ENDPOINT` is fatal here — an enabled adapter that cannot build
    // would fail one upload at a time, and the first symptom would be a user reporting
    // their avatar is broken.
    let storage = store_from_config(&config)
        .map_err(|e| anyhow::anyhow!("storage is enabled but unusable: {e}"))
        .context("check STORAGE_ENABLED, STORAGE_ENDPOINT and STORAGE_BUCKET")?;

    // Google sign-in is optional for the same reason (§4.6). Unset yields the no-op
    // provider, whose `is_enabled()` is false so the sign-in screen hides the button. Set
    // but malformed is fatal here: a redirect URI that will not parse means every
    // callback 500s, and the first report is "the Google button just spins".
    let identity = provider_from_config(&config)
        .map_err(|e| anyhow::anyhow!("Google sign-in is enabled but unusable: {e}"))
        .context("check GOOGLE_ENABLED, GOOGLE_CLIENT_ID and GOOGLE_REDIRECT_URI")?;

    let app = build_router(&config, redis, pool, push, storage, identity);

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
/// this body. The signature already takes the pieces they will need — the config, the
/// Postgres pool, the Redis handle, the [`PushSender`] the notification jobs will be
/// handed, the
/// [`ObjectStore`] the upload routes write through and the [`IdentityProvider`] the
/// sign-in routes verify against — so the swap is a change to this function only.
fn build_router(
    _config: &Config,
    _redis: Redis,
    _pool: PgPool,
    _push: Arc<dyn PushSender>,
    _storage: Arc<dyn ObjectStore>,
    _identity: Arc<dyn IdentityProvider>,
) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/health/ready", get(|| async { "ready" }))
}
