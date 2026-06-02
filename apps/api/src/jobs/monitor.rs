use super::{cleanup_jobs, email_jobs, notification_jobs};
use crate::jobs::cleanup_jobs::CleanupExpiredTokensJob;
use apalis::{layers::retry::RetryPolicy, prelude::*};
use apalis_postgres::PostgresStorage;
use tower::retry::RetryLayer;

/// Runs the Apalis monitor.
/// This function never returns while the server is running.
/// Call it from `tokio::spawn` in `main.rs`.
///
/// Design: each worker gets its own PostgresStorage (separate polling loop),
/// its own concurrency limit, and its own retry policy.
pub async fn run_monitor(pool: sqlx::PgPool) -> anyhow::Result<()> {
    PostgresStorage::setup(&pool).await?;
    tracing::info!("Apalis Postgres migrations applied");

    let email = PostgresStorage::new(&pool);
    let broadcast_started = PostgresStorage::new(&pool);
    let broadcast_scheduled = PostgresStorage::new(&pool);
    let cleanup_storage = PostgresStorage::new(&pool);

    Monitor::new()
        .register(move |_| {
            WorkerBuilder::new("meno-email")
                .backend(email.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(email_jobs::send_email)
        })
        .register(move |_| {
            WorkerBuilder::new("meno-broadcast-started-fanout")
                .backend(broadcast_started.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(notification_jobs::broadcast_started_fanout)
        })
        .register(move |_| {
            WorkerBuilder::new("meno-broadcast-scheduled-fanout")
                .backend(broadcast_scheduled.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(notification_jobs::broadcast_scheduled_fanout)
        })
        .register(move |_| {
            WorkerBuilder::new("meno-broadcast-cleanup-tokens")
                .backend(cleanup_storage.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(cleanup_jobs::cleanup_expired_tokens)
        })
        .on_event(|_, e| tracing::info!(event = ?e, "Apalis monitor event"))
        .shutdown_timeout(std::time::Duration::from_secs(30))
        .run_with_signal(shutdown_signal())
        .await?;

    Ok(())
}

// Utility: push the cleanup job on a schedule
pub async fn schedule_cleanup_job(pool: sqlx::PgPool) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
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
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    Ok(())
}
