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

    state
        .broadcast
        .end(&state, job.broadcast_id, job.host_id)
        .await?;

    tracing::info!(
        broadcast_id = %job.broadcast_id,
        reason       = %job.reason,
        "Broadcast ended via background job"
    );
    Ok(())
}
