use super::{broadcast_jobs, cleanup_jobs, email_jobs, note_jobs, notification_jobs};
use crate::jobs::cleanup_jobs::CleanupExpiredTokensJob;
use apalis::layers::retry::RetryPolicy;
use apalis::prelude::*;
use apalis_postgres::PostgresStorage;
use tower::retry::RetryLayer;

/// Runs the job monitoring system using the Apalis job framework.
///
/// This function sets up the necessary environment for processing jobs, applies database migrations,
/// and initializes workers to handle different types of jobs. Each worker is registered to handle
/// specific job tasks with configurable concurrency, retry policies, and shared storage backends.
/// The workers and their respective jobs include:
///
/// - `meno-email`: Handles email-related jobs.
/// - `meno-broadcast-started-fanout`: Handles fanout notifications for started broadcasts.
/// - `meno-broadcast-scheduled-fanout`: Handles fanout notifications for scheduled broadcasts.
/// - `meno-broadcast-cleanup-tokens`: Cleans up expired tokens.
/// - `meno-broadcast-end`: Handles tasks related to the end of a broadcast.
///
/// The monitor also tracks job execution events and provides a graceful shutdown mechanism.
///
/// ### Parameters:
/// - `pool`: A `sqlx::PgPool` instance representing the connection pool for PostgreSQL.
/// - `state`: Shared application state wrapped in an `std::sync::Arc<MenoState>`. This state is passed
///   to the workers for processing jobs.
///
/// ### Returns:
/// Returns an `anyhow::Result<()>`, which will be `Ok(())` if successful or an error if
/// something goes wrong.
///
/// ### Behavior:
/// - Applies necessary Postgres migrations via the `PostgresStorage::setup` method.
/// - Sets up shared storage via `SharedPostgresStorage`, which provides backend storage
///   for job workers.
/// - Configures workers with the `WorkerBuilder`:
///   - Each worker uses specific layers, like `tower::retry::RetryLayer` for retry policies.
///   - Each worker has a concurrency limit of 20 jobs.
/// - Registers event logging for monitoring job execution.
/// - Handles shutdown signals with a 30-second timeout grace period.
///
/// ### Example:
/// ```rust
/// use sqlx::PgPool;
/// use std::sync::std::sync::Arc;
///
/// let pool = PgPool::connect("postgres://example_db_url").await.unwrap();
/// let state = std::sync::Arc::new(MenoState::new());
///
/// run_monitor(pool, state).await.unwrap();
/// ```
///
/// ### Errors:
/// This function can return errors in cases such as:
/// - Database connection or migration failure.
/// - Job setup or worker initialization issues.
/// - Errors during runtime signal handling or job processing.
pub async fn run_monitor(
    pool: sqlx::PgPool,
    state: std::sync::Arc<crate::state::MenoState>,
) -> anyhow::Result<()> {
    PostgresStorage::setup(&pool).await?;
    tracing::info!("Apalis Postgres migrations applied");

    let mut store = apalis_postgres::shared::SharedPostgresStorage::new(pool);

    let email = store.make_shared()?;
    let broadcast_started = store.make_shared()?;
    let broadcast_scheduled = store.make_shared()?;
    let cleanup_storage = store.make_shared()?;
    let broadcast_end = store.make_shared()?;
    let notes_cleanup = store.make_shared()?;

    let state_email = std::sync::Arc::clone(&state);
    let state_started = std::sync::Arc::clone(&state);
    let state_scheduled = std::sync::Arc::clone(&state);
    let state_cleanup = std::sync::Arc::clone(&state);
    let state_end = std::sync::Arc::clone(&state);
    let state_notes_cleanup = std::sync::Arc::clone(&state);

    Monitor::new()
        .register(move |_| {
            WorkerBuilder::new("meno-email")
                .backend(email.clone())
                .data(state_email.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(email_jobs::send_email)
        })
        .register(move |_| {
            WorkerBuilder::new("meno-broadcast-started-fanout")
                .backend(broadcast_started.clone())
                .data(state_started.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(notification_jobs::broadcast_started_fanout)
        })
        .register(move |_| {
            WorkerBuilder::new("meno-broadcast-scheduled-fanout")
                .backend(broadcast_scheduled.clone())
                .data(state_scheduled.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(notification_jobs::broadcast_scheduled_fanout)
        })
        .register(move |_| {
            WorkerBuilder::new("meno-broadcast-cleanup-tokens")
                .backend(cleanup_storage.clone())
                .data(state_cleanup.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(cleanup_jobs::cleanup_expired_tokens)
        })
        .register(move |_| {
            WorkerBuilder::new("meno-broadcast-end")
                .backend(broadcast_end.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .data(state_end.clone())
                .build(broadcast_jobs::end_broadcast)
        })
        .register(move |_| {
            WorkerBuilder::new("meno-notes-cleanup")
                .backend(notes_cleanup.clone())
                .data(state_notes_cleanup.clone())
                .concurrency(5) // cheap batched deletes, no need for 20 here
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(note_jobs::purge_stale_notes)
        })
        .on_event(|_, e| tracing::info!(event = ?e, "Apalis monitor event"))
        .shutdown_timeout(std::time::Duration::from_secs(30))
        .run_with_signal(shutdown_signal())
        .await?;

    Ok(())
}

// Utility: push the cleanup job on a schedule
pub async fn schedule_cleanup_job(pool: sqlx::PgPool) {
    let mut interval = tokio::time::interval(std::time::Duration::from_hours(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Skip the first immediate tick
    interval.tick().await;

    loop {
        interval.tick().await;
        let mut storage: PostgresStorage<CleanupExpiredTokensJob> = PostgresStorage::new(&pool);

        if let Err(e) = storage.push(CleanupExpiredTokensJob).await {
            tracing::warn!(error = %e, "Failed to schedule cleanup job");
        } else {
            tracing::debug!("CleanupExpiredTokensJob scheduled");
        }
    }
}

pub async fn schedule_notes_cleanup_job(pool: sqlx::PgPool) {
    let mut interval = tokio::time::interval(std::time::Duration::from_hours(24));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;

    loop {
        interval.tick().await;
        let mut storage: PostgresStorage<note_jobs::PurgeStaleNotesJob> =
            PostgresStorage::new(&pool);
        if let Err(e) = storage.push(note_jobs::PurgeStaleNotesJob).await {
            tracing::warn!(error = %e, "Failed to schedule notes cleanup job");
        } else {
            tracing::debug!("PurgeStaleNotesJob scheduled");
        }
    }
}

async fn shutdown_signal() -> Result<(), std::io::Error> {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install the Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }

    Ok(())
}
