use crate::modules::auth::repository::{AuthRepo, AuthRepository};
use crate::state::MenoState;
use apalis::prelude::{BoxDynError, Data};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Periodic job: delete expired refresh tokens in batches.
/// Scheduled via apalis-cron (see monitor.rs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupExpiredTokensJob;

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
