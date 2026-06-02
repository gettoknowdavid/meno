use crate::modules::broadcast::repository::BroadcastRepository;
use crate::shared::services::ws::dto::WsPayload;
use crate::shared::services::ws::model::WsEvent;
use crate::state::MenoState;
use apalis::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::{OffsetDateTime, serde::rfc3339};
use uuid::Uuid;

/// This job notifies all those subscribed to the creator, and for now, every online user via
/// WebSocket, that a new broadcast has just gone live or has started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastStartedFanOutJob {
    pub broadcast_id: Uuid,
    pub creator_id: Uuid,
    pub title: String,
    pub image_url: Option<String>,
}

/// This job is emitted when a broadcast is scheduled.
/// The notification is sent to all those subscribed to the creator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastScheduledFanOutJob {
    pub broadcast_id: Uuid,
    pub creator_id: Uuid,
    pub title: String,
    pub image_url: Option<String>,
    #[serde(with = "rfc3339")]
    pub start_time: OffsetDateTime,
}

pub async fn broadcast_started_fanout(
    job: BroadcastStartedFanOutJob,
    state: Data<Arc<MenoState>>,
) -> Result<(), BoxDynError> {
    let repo = BroadcastRepository::new(state.db.clone());
    let ids = repo.get_subscriber_ids(job.creator_id).await?;
    if ids.is_empty() {
        return Ok(());
    }

    let payload = WsPayload::new(
        WsEvent::NewBroadcast,
        serde_json::json!({
            "broadcast_id": job.broadcast_id,
            "title": job.title,
            "imageUrl": job.image_url,
            "creatorId": job.creator_id,
        }),
    );
    state.ws.send_to_users(&ids, payload).await;

    tracing::info!(
        broadcast_id = %job.broadcast_id,
        notified     = ids.len(),
        "Broadcast started fan-out complete"
    );
    Ok(())
}

pub async fn broadcast_scheduled_fanout(
    job: BroadcastScheduledFanOutJob,
    state: Data<Arc<MenoState>>,
) -> Result<(), BoxDynError> {
    let repo = BroadcastRepository::new(state.db.clone());
    let ids = repo.get_subscriber_ids(job.creator_id).await?;
    if ids.is_empty() {
        return Ok(());
    }

    let payload = WsPayload::new(
        WsEvent::ScheduledBroadcast,
        serde_json::json!({
            "broadcastId": job.broadcast_id,
            "title": job.title,
            "imageUrl": job.image_url,
            "creatorId": job.creator_id,
            "startTime": job.start_time,
        }),
    );
    state.ws.send_to_users(&ids, payload).await;

    tracing::info!(
        broadcast_id = %job.broadcast_id,
        creator_id = %job.creator_id,
        start_time = %job.start_time,
        notified     = ids.len(),
        "Broadcast scheduled fan-out complete"
    );
    Ok(())
}
