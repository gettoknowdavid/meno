//! Postgres: the connection pool and the migrations.
//!
//! Ported from `apps/api/src/database.rs` on `master` (`903c3ba`), which lived outside
//! `infrastructure/` because it predates the layout split.
//!
//! # What changed, and why
//!
//! - **It compiled at all.** `master` called `Duration::from_mins(..)`, which is not a
//!   method on `std::time::Duration` — `from_mins` is on `humantime`, not the standard
//!   library. The file could never have been built; it was undeclared dead code.
//!
//! - **No panics** (§9.1, §7.11). `master` ended in
//!   `.expect("Failed to connect to PostgreSQL DB")`. §7.11 names that line specifically:
//!   "A bad config should exit with a readable message and non-zero code, not a panic
//!   trace." [`create_postgres_pool`] now returns [`DatabaseError`].
//!
//! - **Migrations actually run** (§4.3, §7.2). §7.2 is a P0: the 15 migrations in
//!   `crates/db/migrations` were *never applied*, so a fresh Neon branch has no schema at
//!   all. [`run_migrations`] embeds them with `sqlx::migrate!` and refuses to serve traffic
//!   until they succeed.
//!
//! - **Pool sized for Neon** (§3.5). `master` used 20 connections with a 30-minute
//!   lifetime, which §3.5 calls out by name as wrong behind PgBouncer. See [`PoolSettings`].
//!
//! - **One boundary for driver errors** (§5.5). [`db_err`] is the single place a
//!   `sqlx::Error` becomes a `meno_core::Error`; see the `infrastructure` module docs,
//!   which already pointed here.
//!
//! # Module map
//!
//! - [`pool`] — [`PoolSettings`], the §3.5 sizing.
//! - [`error`] — [`DatabaseError`] and [`db_err`].
//!
//! # Testing
//!
//! Pool sizing and the boundary translation are pure and tested inline. Connecting and
//! migrating need a real Postgres, and live in `mod tests::live` behind `#[ignore]`.

use sqlx::PgPool;

use crate::config::Config;
pub mod error;
pub mod pool;

pub use error::{DatabaseError, db_err};
pub use pool::PoolSettings;

/// The migrations, embedded at compile time.
///
/// `sqlx::migrate!` reads the directory at build time and bakes the SQL into the binary,
/// so a deploy cannot ship code that expects a migration the image does not contain —
/// which is the usual way "it worked in staging" turns into a 500 on the first boot
/// against production.
///
/// The path is relative to the crate root (`apps/api/`), so it must keep pointing at
/// `crates/db/migrations` two levels up.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../crates/db/migrations");

/// How many migrations are embedded.
///
/// Asserted in the tests against the directory on disk. The failure mode this catches is
/// a path typo silently embedding *zero* migrations — `sqlx::migrate!` accepts an empty
/// directory without complaint, and a fresh database would then have no schema and no
/// error, which is §7.2's exact failure recurring in a new form.
pub const EMBEDDED_MIGRATION_COUNT: usize = 15;

/// Open a connection pool.
///
/// # Errors
///
/// Returns [`DatabaseError::Connect`] when the pool cannot be created or the database is
/// unreachable. A bad `DATABASE_URL` is the usual cause; the message says so without
/// printing the URL, which carries a password (§9.5).
///
/// Note this is a *connection* check only. [`run_migrations`] is what guarantees the
/// schema exists, and `bootstrap` must call both before serving.
pub async fn create_postgres_pool(config: &Config) -> Result<PgPool, DatabaseError> {
    create_pool_with(config, PoolSettings::for_neon()).await
}

/// Open a connection pool with explicit sizing.
///
/// Split out so the sizing can be varied in a test without duplicating the connect call.
async fn create_pool_with(
    config: &Config,
    settings: PoolSettings,
) -> Result<PgPool, DatabaseError> {
    settings
        .apply(sqlx::postgres::PgPoolOptions::new())
        .connect(config.database_url.expose())
        .await
        .map_err(|e| {
            // The URL is deliberately absent from this message: it is a `Secret` in
            // `Config` and carries a password. `db_err` is for the *caller* to attach a
            // context; here we know exactly what failed.
            DatabaseError::Connect(format!(
                "check DATABASE_URL (host, port, credentials) — {}",
                redact(&e)
            ))
        })
}

/// Strip anything that looks like a password out of a driver's message.
///
/// A driver error can echo the connection string, and `PgPoolOptions::connect` is handed
/// the secret directly. This is a belt to §9.5's braces: the URL is already a `Secret`
/// and never logged, but the driver's own text is not under our control.
fn redact(error: &sqlx::Error) -> String {
    let text = error.to_string();

    let Some(scheme_end) = text.find("://") else {
        return text;
    };

    // `head` keeps whatever the driver said *before* the URL - which is usually the useful
    // part ("error with configuration: ") - and re-adds the `://`.
    let head = &text[..scheme_end + 3];
    let rest = &text[scheme_end + 3..];

    let authority_end = rest.find(['/', '?', ' ']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];

    // The userinfo is `user:password@`. Dropping it is the whole point: the password is
    // what must never reach a log line, and keeping the host is what lets an operator tell
    // "wrong host" from "wrong password".
    let host = authority.rsplit('@').next().unwrap_or(authority);

    format!("{head}{host}...")
}

/// Apply the embedded migrations, unless `SKIP_MIGRATIONS` says otherwise.
///
/// # Errors
///
/// Returns [`DatabaseError::Migration`] when a migration fails. The caller **must not**
/// serve traffic on this path: §4.3 is explicit — "Refuse to serve traffic until
/// migrations succeed — but _log clearly and exit_, never `panic!`." A process that boots
/// with a stale schema fails every request in a way that looks like a bug in the code
/// rather than a deployment that skipped a step.
///
/// # Idempotency
///
/// `sqlx` tracks applied migrations in `_sqlx_migrations`, so calling this on every boot
/// is safe and applies only what is missing. That is what lets both binaries call it.
pub async fn run_migrations(
    pool: &PgPool,
    config: &Config,
) -> Result<MigrationOutcome, DatabaseError> {
    if should_skip(config) {
        tracing::warn!(
            "SKIP_MIGRATIONS is set; the schema was not verified at startup. This is only \
             safe when another instance owns migration ordering."
        );
        return Ok(MigrationOutcome::Skipped);
    }

    MIGRATOR
        .run(pool)
        .await
        .map_err(|e| DatabaseError::Migration(redact_migration(&e)))?;

    tracing::info!(
        migrations = EMBEDDED_MIGRATION_COUNT,
        "migrations applied; the schema is current"
    );
    Ok(MigrationOutcome::Applied)
}

/// What [`run_migrations`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// Migrations ran. Anything missing was applied.
    Applied,
    /// `SKIP_MIGRATIONS` was set, so nothing ran.
    ///
    /// A distinct outcome rather than a silent `Ok(())` so the startup log line can say
    /// which happened — "migrations applied" and "migrations skipped" are very different
    /// facts to read in an incident.
    Skipped,
}

/// Whether this process should skip migrations.
///
/// The `SKIP_MIGRATIONS` escape hatch §4.3 asks for, exposed separately so the rule is
/// testable without a database.
#[must_use]
pub fn should_skip(config: &Config) -> bool {
    config.skip_migrations
}

/// Reduce a migration error to something safe to log.
fn redact_migration(error: &sqlx::migrate::MigrateError) -> String {
    let text = error.to_string();
    // A migration error quotes the failing statement, which is schema. It belongs in a log
    // and not a response, and `DatabaseError` is only ever logged — but trimming keeps the
    // line readable.
    let first_line = text.lines().next().unwrap_or("unknown migration failure");
    first_line.to_owned()
}

/// Log the pool's capacity and idle count at startup.
///
/// §4.8 lists "DB pool saturation" as an SLO metric to expose. There is no `/metrics`
/// endpoint yet — `routes/` does not exist — so there is nowhere to scrape a gauge from,
/// and this is the interim answer: a single structured line at boot.
///
/// It is worth having *because* §3.5 deliberately made the pool small. A pool capped at 10
/// is the right setting for Neon and also the setting most likely to saturate, so
/// "connections=10 idle=8" at boot and a rising wait time afterwards is the pair of
/// numbers an operator needs to correlate.
///
/// ```text
/// db_connections = 10   // the cap
/// db_idle = 8// open now
/// ```
pub fn log_pool_capacity(pool: &PgPool) {
    let settings = PoolSettings::for_neon();
    tracing::info!(
        db_connections = settings.max_connections,
        db_idle = pool.num_idle(),
        db_open = pool.size(),
        db_min_connections = settings.min_connections,
        "postgres pool ready"
    );
}

/// Log a database fault and return the application error.
///
/// The shape every repository should use so §9.1's "logged with full context" is the
/// default rather than something each repository remembers to do.
///
/// # Errors
///
/// Always. Returning `Result` from a function that only fails this way lets a repository
/// write `repository_error("load_broadcast", e)?` and get both the log line and the
/// boundary translation in one expression.
pub fn repository_error<T>(
    context: &'static str,
    result: Result<T, sqlx::Error>,
) -> Result<T, meno_core::Error> {
    result.map_err(|e| {
        let error = db_err(context, &e);
        tracing::error!(
            context,
            service = error::SERVICE_NAME,
            detail = %e,
            "database operation failed"
        );
        error
    })
}

#[cfg(test)]
mod tests {
    //! Tests for the migration gate, the redaction, and the embedded set.
    //!
    //! Connecting and migrating need a real Postgres and live in `mod live` below.

    use super::*;

    fn config_with_skip(skip: bool) -> Config {
        Config::from_source(
            &crate::config::MapSource::new()
                .with("ENV", "dev")
                .with("DATABASE_URL", "postgres://u:p@localhost/meno")
                .with("REDIS_URL", "redis://localhost:6379")
                .with("JWT_SECRET", "a-real-secret-value")
                .with("JWT_REFRESH_SECRET", "another-real-secret")
                .with("CORS_ORIGINS", "https://app.example.com")
                .with("SKIP_MIGRATIONS", if skip { "true" } else { "false" }),
        )
        .expect("valid config")
    }

    // ── the SKIP_MIGRATIONS escape hatch (§4.3) ───────────────────────────

    #[test]
    fn migrations_run_by_default() {
        // §4.3 wants them to run at startup, so the default has to be "run".
        assert!(!should_skip(&config_with_skip(false)));
    }

    #[test]
    fn the_escape_hatch_skips_migrations_when_set() {
        assert!(should_skip(&config_with_skip(true)));
    }

    #[test]
    fn skip_migrations_is_off_unless_the_variable_says_otherwise() {
        // Not set at all.
        let config = Config::from_source(
            &crate::config::MapSource::new()
                .with("ENV", "dev")
                .with("DATABASE_URL", "postgres://u:p@localhost/meno")
                .with("REDIS_URL", "redis://localhost:6379")
                .with("JWT_SECRET", "a-real-secret-value")
                .with("JWT_REFRESH_SECRET", "another-real-secret")
                .with("CORS_ORIGINS", "https://app.example.com"),
        )
        .expect("valid config");

        assert!(!config.skip_migrations);
        assert!(!should_skip(&config));
    }

    #[tokio::test]
    async fn skipping_is_reported_distinctly_from_applying() {
        // An incident reader has to be able to tell "the schema was checked" from "nobody
        // checked the schema", and `Ok(())` for both makes that impossible.
        // `connect_lazy` opens no connection, so this exercises the gate, not the network.
        let pool = PgPool::connect_lazy("postgres://u:p@127.0.0.1:1/meno")
            .expect("a lazy pool needs no reachable database");

        let outcome = run_migrations(&pool, &config_with_skip(true))
            .await
            .expect("skipping cannot fail");

        assert_eq!(outcome, MigrationOutcome::Skipped);
    }

    // ── the embedded migration set (§7.2) ─────────────────────────────────

    #[test]
    fn the_expected_migrations_are_embedded() {
        // A path typo would embed zero migrations and `sqlx::migrate!` would not complain,
        // so §7.2 would recur in a new form: no schema, no error.
        assert_eq!(
            MIGRATOR.migrations.len(),
            EMBEDDED_MIGRATION_COUNT,
            "the embedded set must match the directory on disk"
        );
    }

    #[test]
    fn the_migrations_are_numbered_and_ordered() {
        // `_sqlx_migrations` applies these in version order, so a duplicate or
        // out-of-sequence version is a silent hazard rather than a compile error.
        let versions: Vec<i64> = MIGRATOR.migrations.iter().map(|m| m.version).collect();

        assert!(
            versions.windows(2).all(|pair| pair[0] < pair[1]),
            "versions must be strictly increasing: {versions:?}"
        );
        assert_eq!(
            versions.len(),
            versions
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        );
    }

    #[test]
    fn the_first_and_last_migrations_are_the_ones_on_disk() {
        let versions: Vec<i64> = MIGRATOR.migrations.iter().map(|m| m.version).collect();

        assert_eq!(
            versions.first(),
            Some(&1),
            "0001_extensions must be embedded"
        );
        assert_eq!(
            versions.last(),
            Some(&(EMBEDDED_MIGRATION_COUNT as i64)),
            "the newest migration must be embedded"
        );
    }

    // ── redaction (§9.5) ──────────────────────────────────────────────────

    #[test]
    fn a_url_in_an_error_is_stripped_of_its_credentials() {
        // `create_pool_with` hands a `Secret` to sqlx, and a driver error can echo it
        // back. The password must not reach a log line.
        let error = sqlx::Error::Configuration(
            "postgres://meno:hunter2@db.internal:5432/meno"
                .to_owned()
                .into(),
        );

        let redacted = redact(&error);

        assert!(
            !redacted.contains("hunter2"),
            "the password must be stripped: {redacted}"
        );
        assert!(
            !redacted.contains("meno:"),
            "the user must be stripped: {redacted}"
        );
        assert!(
            redacted.contains("db.internal"),
            "the host must survive — it is what tells you the host is wrong: {redacted}"
        );
    }

    #[test]
    fn an_error_without_a_url_is_passed_through_unchanged() {
        let error = sqlx::Error::PoolTimedOut;

        assert_eq!(redact(&error), error.to_string());
    }

    // ── the repository boundary (§5.5) ────────────────────────────────────

    #[test]
    fn a_repository_failure_is_logged_and_translated() {
        let result: Result<(), sqlx::Error> = Err(sqlx::Error::PoolTimedOut);

        let error = repository_error("load_broadcast", result).expect_err("must fail");

        assert_eq!(error.code(), meno_core::ErrorCode::Internal);
        assert!(!error.is_client_safe());
        match &error {
            meno_core::Error::Internal { context, .. } => assert_eq!(*context, "load_broadcast"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn a_successful_repository_call_passes_its_value_through() {
        let result: Result<u64, sqlx::Error> = Ok(42);

        assert_eq!(
            repository_error("count_broadcasts", result).expect("must pass through"),
            42
        );
    }

    /// Tests that need a real Postgres.
    ///
    /// These are the tests §4.3 and §7.2 actually call for: "Verify by creating a
    /// brand-new Neon branch and confirming the schema builds from zero." No Postgres is
    /// available here, so everything is `#[ignore]`d.
    ///
    /// Run with `cargo test -p meno-api -- --ignored database::tests::live` against a
    /// `DATABASE_URL` pointing at an empty database.
    mod live {
        use super::*;

        #[tokio::test]
        #[ignore = "needs a real, empty Postgres"]
        async fn a_fresh_database_gets_every_migration() {
            let config = Config::load().expect("a real environment");
            let pool = create_postgres_pool(&config).await.expect("a pool");

            let outcome = run_migrations(&pool, &config)
                .await
                .expect("migrations must apply to an empty database");

            assert_eq!(outcome, MigrationOutcome::Applied);

            // The schema the app needs must exist, which is the whole of §7.2.
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM information_schema.tables \
                 WHERE table_name = 'users')",
            )
            .fetch_one(&pool)
            .await
            .expect("a query");

            assert!(exists, "the users table must exist after migrating");
        }

        #[tokio::test]
        #[ignore = "needs a real, empty Postgres"]
        async fn migrations_are_idempotent_across_repeated_boots() {
            // Both binaries call `run_migrations` on every start, so running twice must be
            // a no-op rather than an error.
            let config = Config::load().expect("a real environment");
            let pool = create_postgres_pool(&config).await.expect("a pool");

            run_migrations(&pool, &config).await.expect("first run");
            let second = run_migrations(&pool, &config).await.expect("second run");

            assert_eq!(second, MigrationOutcome::Applied);
        }

        #[tokio::test]
        #[ignore = "needs a real Postgres with an unreachable host"]
        async fn a_bad_database_url_is_an_error_not_a_panic() {
            // §7.11: the specific regression. `master` panicked here.
            let config = Config::from_source(
                &crate::config::MapSource::new()
                    .with("ENV", "dev")
                    .with("DATABASE_URL", "postgres://u:p@127.0.0.1:1/meno")
                    .with("REDIS_URL", "redis://localhost:6379")
                    .with("JWT_SECRET", "a-real-secret-value")
                    .with("JWT_REFRESH_SECRET", "another-real-secret")
                    .with("CORS_ORIGINS", "https://app.example.com"),
            )
            .expect("valid config");

            let Err(error) = create_postgres_pool(&config).await else {
                panic!("an unreachable database must not produce a pool");
            };

            assert!(matches!(error, DatabaseError::Connect(_)), "got {error:?}");
        }
    }
}
