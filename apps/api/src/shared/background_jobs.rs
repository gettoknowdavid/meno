use crate::modules::auth::errors::AuthError;
use crate::modules::auth::repository::AuthRepository;
use crate::shared::services::redis::RedisService;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct BackgroundJobs {
    auth_repo: AuthRepository,
}
impl BackgroundJobs {
    pub fn new(database: PgPool, redis: RedisService) -> Self {
        let auth_repo = AuthRepository::new(database, redis);
        Self { auth_repo }
    }

    pub fn start(self: Arc<Self>, cancel_token: CancellationToken) {
        let self_clone = Arc::clone(&self);
        let token_clone = cancel_token.clone();

        tokio::spawn(async move {
            self_clone.cleanup_expired_refresh_tokens(token_clone).await;
        });
    }
    async fn cleanup_expired_refresh_tokens(self: Arc<Self>, cancel_token: CancellationToken) {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        // Skip the immediate initial tick so it waits 1 hour before first run
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.run_refresh_token_cleanup().await {
                        Ok(count) => {
                            if count > 0 {
                                tracing::info!(count, "Cleaned up expired refresh tokens");
                            } else {
                                tracing::debug!("No expired refresh tokens to clean");
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to cleanup expired refresh tokens");
                        }
                    }
                }
                _ = cancel_token.cancelled() => {
                    tracing::info!("Stopping expired refresh token cleanup background job gracefully.");
                    break;
                }
            }
        }
    }

    async fn run_refresh_token_cleanup(&self) -> Result<u64, AuthError> {
        self.auth_repo.cleanup_expired_refresh_tokens().await
    }
}
