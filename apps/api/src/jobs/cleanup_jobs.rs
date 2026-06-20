use crate::modules::auth::repository::{AuthRepo, AuthRepository};
use crate::state::MenoState;
use apalis::prelude::{BoxDynError, Data};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Periodic job: delete expired refresh tokens in batches.
/// Scheduled via apalis-cron (see monitor.rs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupExpiredTokensJob;

/// Asynchronous function to clean up expired refresh tokens from the database.
///
/// This function is triggered as part of a background job to remove expired
/// tokens from the system, reducing unnecessary storage usage and maintaining
/// data integrity. It uses the `AuthRepository` to interact with the database
/// and logs the count of deleted tokens if any.
///
/// # Arguments
///
/// * `_job` - A `CleanupExpiredTokensJob` instance representing the context for the cleanup job.
///   It is unused in this function but may contain metadata or context in broader usage scenarios.
///
/// * `state` - Shared application state of type `Data<Arc<MenoState>>` that provides access to
///   the necessary resources, including the database connection.
///
/// # Returns
///
/// This function returns a `Result`:
///
/// * `Ok(())` when the cleanup operation completes successfully.
/// * `Err(BoxDynError)` if any error occurs during the cleanup process, including database failures.
///
/// # Side Effects
///
/// * Logs the number of expired tokens successfully cleaned up, if greater than zero.
///
/// # Examples
///
/// ```rust
/// let job = CleanupExpiredTokensJob { /* job details */ };
/// let app_state = Data::new(Arc::new(MenoState::from_config(config)));
/// cleanup_expired_tokens(job, app_state).await?;
/// ```
pub async fn cleanup_expired_tokens(
    _job: CleanupExpiredTokensJob,
    state: Data<Arc<MenoState>>,
) -> Result<(), BoxDynError> {
    let repo = AuthRepository::new(state.db.clone());
    let deleted = repo.cleanup_expired_refresh_tokens().await?;
    if deleted > 0 {
        tracing::info!(deleted, "Cleaned up expired refresh tokens");
    }
    Ok(())
}
