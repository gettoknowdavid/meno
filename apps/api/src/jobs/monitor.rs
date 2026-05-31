use crate::jobs::email_job;
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

    Monitor::new()
        .register(move |_| {
            WorkerBuilder::new("meno-email")
                .backend(email.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(email_job::send_email)
        })
        .on_event(|_, e| tracing::info!(event = ?e, "Apalis monitor event"))
        .shutdown_timeout(std::time::Duration::from_secs(30))
        .run_with_signal(shutdown_signal())
        .await?;

    Ok(())
}

pub async fn schedule_cleanup_job(pool: sqlx::PgPool) {}

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
