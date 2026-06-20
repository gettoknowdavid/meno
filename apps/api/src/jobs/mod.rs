use apalis::prelude::TaskSink;
use apalis_postgres::PostgresStorage;

pub mod broadcast_jobs;
pub mod cleanup_jobs;
pub mod email_jobs;
pub mod monitor;
pub mod notification_jobs;

#[derive(Clone)]
pub struct Jobs {
    pub email: PostgresStorage<email_jobs::SendEmailJob>,
    pub broadcast_started: PostgresStorage<notification_jobs::BroadcastStartedFanOutJob>,
    pub broadcast_scheduled: PostgresStorage<notification_jobs::BroadcastScheduledFanOutJob>,
    pub cleanup: PostgresStorage<cleanup_jobs::CleanupExpiredTokensJob>,
    pub broadcast_end: PostgresStorage<broadcast_jobs::EndBroadcastJob>,
}
impl Jobs {
    /// Build all storage instances.
    /// Call once at startup, after `PostgresStorage::setup()` has run migrations.
    #[must_use]
    pub fn new(pool: &sqlx::PgPool) -> Self {
        Self {
            email: PostgresStorage::new(pool),
            broadcast_started: PostgresStorage::new(pool),
            broadcast_scheduled: PostgresStorage::new(pool),
            cleanup: PostgresStorage::new(pool),
            broadcast_end: PostgresStorage::new(pool),
        }
    }

    pub async fn push_email(&self, job: email_jobs::SendEmailJob) -> anyhow::Result<()> {
        self.email.clone().push(job).await?;
        Ok(())
    }
    pub async fn push_broadcast_started_fanout(
        &self,
        job: notification_jobs::BroadcastStartedFanOutJob,
    ) -> anyhow::Result<()> {
        self.broadcast_started.clone().push(job).await?;
        Ok(())
    }
    pub async fn push_broadcast_scheduled_fanout(
        &self,
        job: notification_jobs::BroadcastScheduledFanOutJob,
    ) -> anyhow::Result<()> {
        self.broadcast_scheduled.clone().push(job).await?;
        Ok(())
    }
    pub async fn push_broadcast_end(
        &self,
        job: broadcast_jobs::EndBroadcastJob,
    ) -> anyhow::Result<()> {
        self.broadcast_end.clone().push(job).await?;
        Ok(())
    }
    pub async fn push_cleanup(
        &self,
        job: cleanup_jobs::CleanupExpiredTokensJob,
    ) -> anyhow::Result<()> {
        self.cleanup.clone().push(job).await?;
        Ok(())
    }
}
