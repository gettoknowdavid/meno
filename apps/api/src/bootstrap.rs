//! Startup wiring shared by both binaries (plan §6).
//!
//! # Why this file exists
//!
//! `meno-api` and `meno-worker` are two entry points in one crate, built into one image
//! and deployed as two Render services. Before this module they each assembled their own
//! config, pool and Redis handle, which is a duplication with a cost: the two could drift,
//! and the failure mode of that drift is a worker whose schema is a version behind the web
//! service that enqueued its jobs.
//!
//! §6 says both "call the same `bootstrap::build_context()` so config/DB/Redis wiring is
//! identical". That is what [`build_context`] is.
//!
//! # What it does, in order, and why that order
//!
//! 1. **Config** — everything else reads it, and the log filter comes from it, so there
//!    is nothing useful to print before it is read.
//! 2. **Telemetry** — so the failure of step 3 is logged rather than lost.
//! 3. **Postgres, then migrations** — §4.3: "Refuse to serve traffic until migrations
//!    succeed — but log clearly and exit, never `panic!`". A `Result` returned to
//!    `main` *is* that exit.
//!
//! 4. **Redis** — after the schema exists, so a Redis failure cannot be mistaken for a
//!    missing-table failure.
//!
//! # What it deliberately does not do
//!
//! It does not start the Apalis monitor. §6 is explicit that `main.rs` must not, and
//! that the guard should make it impossible to re-enable by accident; the [`AppRole`]
//! enum carries which binary is running so the job loop can be attached in exactly one
//! place when `jobs/` lands.

use std::sync::Arc;

use anyhow::Context as _;

use crate::config::Config;
use crate::infrastructure::database::{create_postgres_pool, log_pool_capacity, run_migrations};
use crate::infrastructure::push::sender_from_config;
use crate::infrastructure::redis::{Redis, RedisConfig};
use crate::infrastructure::storage::store_from_config;
use crate::infrastructure::telemetry;
use crate::modules::auth::state::AuthState;
use crate::state::{Assembly, MenoState};

/// Which binary is starting.
///
/// §6 asks for this so the web service cannot accidentally run the job monitor, and the
/// worker cannot accidentally serve traffic. It is an enum rather than a boolean because
/// a boolean is exactly the kind of thing that gets passed as `true` in the wrong call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppRole {
    /// `meno-api`: serves HTTP.
    Web,
    /// `meno-worker`: runs background jobs.
    Worker,
}

impl AppRole {
    /// The name used in log lines, so the two processes are distinguishable in a log
    /// aggregator without guessing from the absence of traffic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Worker => "worker",
        }
    }
}

impl std::fmt::Display for AppRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Everything a binary needs after startup.
///
/// Returned rather than logged-and-dropped so neither `main` can forget to use it.
pub struct AppContext {
    /// Validated configuration, shared with the state.
    pub config: Arc<Config>,
    /// The wired application: modules plus the infrastructure they hold.
    pub state: MenoState,
}

/// Build the shared startup context.
///
/// # Errors
///
/// Returns an error, with context naming the variable at fault, when:
///
/// - the configuration is invalid or a required variable is missing (§4.6 — all missing
///   variables are listed at once, not one per restart);
/// - Postgres cannot be reached, or the migrations fail (§4.3 — this is a startup
///   failure by design, not a warning);
/// - Redis cannot be reached. `REDIS_URL` is the single most common misconfiguration on
///   a fresh deployment, and fred's own error is a bare `IO Error` with no indication of
///   which URL it tried, so the URL is named here;
/// - push or storage is *enabled but unusable*. Optional integrations must not fail when
///   switched off (§4.6) — that is what the no-op adapters are for — but an enabled
///   adapter that cannot build would fail one request at a time, and the first symptom
///   would be a user reporting their avatar is broken.
///
/// This function deliberately does not start the job monitor; see the module docs.
pub async fn build_context(role: AppRole) -> anyhow::Result<AppContext> {
    let config = Arc::new(Config::load()?);
    telemetry::init(&config);

    tracing::info!(config = %config.summary(), role = %role, "starting");

    let pool = create_postgres_pool(&config).await?;
    run_migrations(&pool, &config).await?;
    log_pool_capacity(&pool);

    let redis_url = config.redis_url.expose().to_owned();
    let redis = Redis::new(RedisConfig::from_url(redis_url.clone()))
        .await
        .with_context(|| format!("failed to connect to Redis at {redis_url}"))?;

    let push = sender_from_config(&config)
        .map_err(|e| anyhow::anyhow!("push is enabled but unusable: {e}"))
        .context("check PUSH_ENABLED and FIREBASE_SERVICE_ACCOUNT_JSON")?;

    let storage = store_from_config(&config)
        .map_err(|e| anyhow::anyhow!("storage is enabled but unusable: {e}"))
        .context("check STORAGE_ENABLED, STORAGE_ENDPOINT and STORAGE_BUCKET")?;

    let state = crate::state::build(Assembly {
        config: Arc::clone(&config),
        db: pool,
        redis: redis.clone(),
        // §4.6: an unset SMTP_HOST yields the no-op mailer, which logs and drops.
        // Refusing to start would make `SMTP_HOST` load-bearing for a deployment that
        // does not need email.
        mailer: AuthState::default_mailer(&config),
        push,
        storage,
    })
    .map_err(|e| anyhow::anyhow!("auth wiring failed: {e}"))
    .context("check JWT_SECRET, JWT_REFRESH_SECRET and the GOOGLE_* settings")?;

    Ok(AppContext { config, state })
}

#[cfg(test)]
mod tests {
    //! The only thing testable without infrastructure is the role enum — and it is worth
    //! testing, because §6's whole point is that the two binaries cannot be confused.

    use super::AppRole;

    #[test]
    fn the_two_roles_are_distinguishable_in_a_log_line() {
        assert_eq!(AppRole::Web.as_str(), "web");
        assert_eq!(AppRole::Worker.as_str(), "worker");
        // Without this the log field is a constant and tells an operator nothing.
        assert_ne!(AppRole::Web, AppRole::Worker);
    }

    #[test]
    fn a_role_renders_into_a_structured_field() {
        assert_eq!(AppRole::Worker.to_string(), "worker");
    }
}
