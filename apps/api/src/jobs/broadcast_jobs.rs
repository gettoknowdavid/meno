use crate::shared::services::redis::keys::RedisKey;
use crate::state::MenoState;
use apalis::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Scheduled at go-live time; fires when the host's grace period expires.
/// If the host reconnected, the grace key was deleted and this job is a no-op.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndBroadcastJob {
    pub broadcast_id: Uuid,
    pub host_id: Uuid,
    pub reason: String,
}

/// Ends a broadcast session that has exceeded its grace period without the host reconnecting.
///
/// # Arguments
///
/// * `job` - An instance of `EndBroadcastJob` which contains the broadcast session details,
///   including `broadcast_id`, `host_id`, and the `reason` for ending the broadcast.
/// * `state` - Shared application state of type `Data<Arc<MenoState>>` which includes access to
///   Redis for caching and the broadcast service for managing sessions.
///
/// # Behavior
///
/// 1. Checks whether the broadcast's grace period is still active by verifying the existence of
///    a Redis key associated with the grace period.
/// 2. If the grace period has expired and the host has not reconnected, the broadcast session is
///    terminated by invoking the broadcast service's `end` method.
/// 3. If the grace key is missing (indicating the host has reconnected), no action is taken, and
///    the function exits early.
///
/// # Redis Key
/// The function relies on a Redis key associated with the broadcast's grace period. The key is
/// obtained using `RedisKey::host_grace(job.broadcast_id)`.
///
/// # Logging
/// - Logs an informational message if the host has reconnected during the grace period, skipping
///   the broadcast termination.
/// - Logs an informational message upon successful termination of the broadcast, including the
///   broadcast ID and the reason for termination.
///
/// # Errors
///
/// Returns:
/// * `Ok(())` if the operation succeeds without errors.
/// * `Err(BoxDynError)` if any asynchronous operation (such as Redis commands or broadcast service
///   calls) fails.
///
/// # Example
///
/// ```rust
/// let job = EndBroadcastJob {
///     broadcast_id: "broadcast123".to_string(),
///     host_id: "host456".to_string(),
///     reason: "Grace period expired".to_string(),
/// };
/// let result = end_broadcast(job, state.clone()).await;
/// if let Err(e) = result {
///     eprintln!("Failed to end broadcast: {:?}", e);
/// }
/// ```
pub async fn end_broadcast(
    job: EndBroadcastJob,
    state: Data<Arc<MenoState>>,
) -> Result<(), BoxDynError> {
    // Check grace key — if missing, host reconnected: nothing to do
    let grace_key = RedisKey::host_grace(job.broadcast_id);
    let still_in_grace = state.redis.exists(&grace_key).await.unwrap_or(false);

    if !still_in_grace {
        tracing::info!(
            broadcast_id = %job.broadcast_id,
            "EndBroadcastJob: host already reconnected, skipping"
        );
        return Ok(());
    }

    // Grace period still active → host never reconnected → end the broadcast
    let _ = state.redis.del(&grace_key).await;

    let broadcast_id = job.broadcast_id;
    let host_id = job.host_id;
    state.broadcast.service.end(broadcast_id, host_id).await?;

    tracing::info!(
        broadcast_id = %broadcast_id,
        reason       = %job.reason,
        "Broadcast ended via background job"
    );
    Ok(())
}
