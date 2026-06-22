use crate::state::MenoState;
use apalis::prelude::{BoxDynError, Data};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::OffsetDateTime;

const RETENTION_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgeStaleNotesJob;

/// Hard-deletes notes and folders whose tombstone (`deleted_at`) is older
/// than 30 days. Soft-deletes exist purely so other devices can sync the
/// deletion; once every device has had a generous window to do that, the
/// tombstone has served its purpose and can be reclaimed.
pub async fn purge_stale_notes(
    _job: PurgeStaleNotesJob,
    state: Data<Arc<MenoState>>,
) -> Result<(), BoxDynError> {
    let cutoff = OffsetDateTime::now_utc() - time::Duration::days(RETENTION_DAYS);
    let (notes_deleted, folders_deleted) = state.notes.service.purge_stale(cutoff).await?;
    if notes_deleted > 0 || folders_deleted > 0 {
        tracing::info!(
            notes_deleted,
            folders_deleted,
            "Purged stale soft-deleted notes/folders"
        );
    }
    Ok(())
}
