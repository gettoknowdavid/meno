//! Process signals: how the server learns it should stop.
//!
//! Ported from `apps/api/src/shared/signals.rs` on `master` (`903c3ba`).
//!
//! # Why SIGTERM matters more than Ctrl-C
//!
//! Render — and every other container orchestrator — stops a process by sending
//! `SIGTERM` and then killing it after a grace period. `master` only listened for
//! Ctrl-C, which never arrives in a container, so every deploy killed the process
//! mid-flight: open WebSockets dropped with no `serverShutdown` notice, and in-flight
//! requests severed. Both signals are handled here.

/// Resolves when the process has been asked to stop.
///
/// Awaited by `axum::serve(...).with_graceful_shutdown(..)`, which then stops
/// accepting new connections and lets in-flight ones finish.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        // Not `expect`: a failure to install the handler means we lose Ctrl-C
        // handling, and panicking there turns a degraded shutdown into a dead
        // process that ignores its own interrupt.
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "failed to install the Ctrl-C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to install the SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    // Windows has no SIGTERM; keep the future pending rather than completing it, which
    // would shut the server down immediately.
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("shutdown requested via Ctrl-C"),
        () = terminate => tracing::info!("shutdown requested via SIGTERM"),
    }
}
