use super::email_jobs::{
    SendResetPasswordEmailJob, SendVerificationEmailJob, handle_send_reset_password_email,
    handle_send_verification_email,
};
use crate::shared::signals::shutdown_signal;
use crate::state::MenoState;
use apalis::{layers::retry::RetryPolicy, prelude::*};
use apalis_postgres::PostgresStorage;
use tower::retry::RetryLayer;

/// Runs the Apalis monitor.
/// This function never returns while the server is running.
/// Call it from `tokio::spawn` in `main.rs`.
///
/// Design: each worker gets its own PostgresStorage (separate polling loop),
/// its own concurrency limit, and its own retry policy.
pub async fn run_monitor(
    pool: sqlx::PgPool,
    state: std::sync::Arc<MenoState>,
) -> anyhow::Result<()> {
    // Run the migrations once on startup.
    // This creates the `apalis_jobs` table with the supporting indexes, if absent.
    // Since it is idempotent, it is okay to call it on every startup.
    PostgresStorage::<()>::new(&pool).await?;
    tracing::info!("Apalis Postgres migrations applied");

    let verify_emails_storage: PostgresStorage<SendVerificationEmailJob> =
        PostgresStorage::new(&pool);

    let reset_emails_storage: PostgresStorage<SendResetPasswordEmailJob> =
        PostgresStorage::new(&pool);

    Monitor::new()
        .register(
            WorkerBuilder::new("meno-email-verification")
                .backend(verify_emails_storage)
                .data(state.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(handle_send_verification_email),
        )
        .register(
            WorkerBuilder::new("meno-email-reset")
                .backend(reset_emails_storage)
                .data(state.clone())
                .concurrency(20)
                .layer(RetryLayer::new(RetryPolicy::retries(3)))
                .build(handle_send_reset_password_email),
        )
        .on_event(|e| tracing::info!(event = ?e, "Apalis monitor event"))
        .shutdown_timeout(std::time::Duration::from_secs(30))
        .run_with_signal(shutdown_signal())
        .await?;

    Ok(())
}

pub async fn schedule_cleanup_job(pool: sqlx::PgPool) {}
