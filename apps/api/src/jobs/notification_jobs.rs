use crate::modules::broadcast::repository::{BroadcastRepo, BroadcastRepository};
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

/// Asynchronously handles the fan-out operation for notifying subscribers about a started broadcast.
///
/// # Parameters
/// - `job`: A `BroadcastStartedFanOutJob` struct containing details about the broadcast (e.g., broadcast ID, title, image URL, creator ID).
/// - `app`: Shared application state wrapped in `Data<Arc<MenoState>>` which provides access to the database and pub-sub system.
///
/// # Returns
/// - `Ok(())` on successful completion of the fan-out.
/// - `Err(BoxDynError)` if any error occurs during the database query or while publishing notifications.
///
/// # Workflow
/// 1. Creates a `BroadcastRepository` instance using the application's database connection.
/// 2. Fetches the list of subscriber IDs for the broadcast creator via `get_subscriber_ids`.
/// 3. If no subscribers are found, it returns early with `Ok(())`.
/// 4. Constructs a `WsPayload` with event details, including broadcast ID, title, image URL, and creator ID.
/// 5. Publishes the payload to the list of subscribers using the pub-sub system.
/// 6. Logs the event details, such as the broadcast ID and the count of notified subscribers, using `tracing`.
///
/// # Errors
/// - Returns an error if fetching subscriber IDs (`get_subscriber_ids`) or publishing the notification (`publish_to_users`) fails.
///
/// # Example Usage
/// ```rust
/// let job = BroadcastStartedFanOutJob {
///     broadcast_id: "1234".to_string(),
///     title: "Live Session on Rust".to_string(),
///     image_url: "https://example.com/image.png".to_string(),
///     creator_id: "5678".to_string(),
/// };
///
/// broadcast_started_fanout(job, app_state).await?;
/// ```
pub async fn broadcast_started_fanout(
    job: BroadcastStartedFanOutJob,
    app: Data<Arc<MenoState>>,
) -> Result<(), BoxDynError> {
    let repo = BroadcastRepository::new(app.db.clone());
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
    app.pubsub.publish_to_users(&ids, payload).await;

    tracing::info!(
        broadcast_id = %job.broadcast_id,
        notified     = ids.len(),
        "Broadcast started fan-out complete"
    );
    Ok(())
}

/// Asynchronously handles a scheduled broadcast fan-out job by fetching subscriber IDs
/// and publishing a WebSocket payload to all subscribers.
///
/// # Arguments
///
/// * `job` - An instance of `BroadcastScheduledFanOutJob` containing the details of
///   the scheduled broadcast (e.g., creator ID, broadcast ID, title, image URL, start time).
/// * `app` - Application state wrapped in `Data<Arc<MenoState>>`, providing access to
///   shared resources such as the database and pub/sub system.
///
/// # Returns
///
/// This function returns a `Result<(), BoxDynError>`:
/// * `Ok(())` if the broadcast fan-out is successfully handled.
/// * `Err(BoxDynError)` if an error occurs while retrieving subscriber IDs or publishing
///   the payload to users.
///
/// # Workflow
///
/// 1. A new `BroadcastRepository` instance is created using the database connection in `app`.
/// 2. Subscriber IDs for the given broadcast creator are fetched using
///    `repo.get_subscriber_ids(job.creator_id).await?`.
/// 3. If there are no subscribers (IDs are empty), the function exits early with `Ok(())`.
/// 4. Constructs a WebSocket payload (`WsPayload`) with event details, including:
///     - Event type `WsEvent::ScheduledBroadcast`
///     - Broadcast ID, title, image URL, creator ID, and start time.
/// 5. Publishes the WebSocket payload to the retrieved subscriber IDs using
///    `app.pubsub.publish_to_users(...)`.
/// 6. Logs the successful completion of the fan-out process, including the broadcast ID,
///    creator ID, start time, and the number of subscribers notified.
///
/// # Logging
///
/// The function uses `tracing::info` to log the following details after a successful fan-out:
/// * `broadcast_id`: The ID of the broadcast.
/// * `creator_id`: The ID of the broadcast's creator.
/// * `start_time`: The start time of the broadcast.
/// * `notified`: The count of notified subscribers.
///
/// # Errors
///
/// The function may return an error under the following circumstances:
/// * Failure to retrieve subscriber IDs from the database.
/// * Issues encountered while publishing the WebSocket payload.
///
/// # Example
///
/// ```rust
/// let job = BroadcastScheduledFanOutJob {
///     creator_id: 123,
///     broadcast_id: "abc123".to_string(),
///     title: "Exciting Event".to_string(),
///     image_url: "https://example.com/image.png".to_string(),
///     start_time: Utc::now(),
/// };
/// let app_state = Data::new(Arc::new(MenoState::new(/* config */)));
///
/// if let Err(e) = broadcast_scheduled_fanout(job, app_state).await {
///     eprintln!("Fan-out failed: {}", e);
/// }
/// ```
pub async fn broadcast_scheduled_fanout(
    job: BroadcastScheduledFanOutJob,
    app: Data<Arc<MenoState>>,
) -> Result<(), BoxDynError> {
    let repo = BroadcastRepository::new(app.db.clone());
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
    app.pubsub.publish_to_users(&ids, payload).await;

    tracing::info!(
        broadcast_id = %job.broadcast_id,
        creator_id = %job.creator_id,
        start_time = %job.start_time,
        notified     = ids.len(),
        "Broadcast scheduled fan-out complete"
    );
    Ok(())
}
