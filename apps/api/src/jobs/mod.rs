use cleanup_jobs::CleanupExpiredTokensJob;
use email_jobs::SendEmailJob;
use notification_jobs::{BroadcastScheduledFanOutJob, BroadcastStartedFanOutJob};

use crate::jobs::broadcast_jobs::EndBroadcastJob;
use anyhow::Result;
use apalis::prelude::TaskSink;
use apalis_postgres::PostgresStorage;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod broadcast_jobs;
pub mod cleanup_jobs;
pub mod email_jobs;
pub mod monitor;
pub mod notification_jobs;

#[derive(Clone)]
pub struct Jobs {
    pub email: Arc<Mutex<PostgresStorage<SendEmailJob>>>,
    pub broadcast_started: Arc<Mutex<PostgresStorage<BroadcastStartedFanOutJob>>>,
    pub broadcast_scheduled: Arc<Mutex<PostgresStorage<BroadcastScheduledFanOutJob>>>,
    pub cleanup: Arc<Mutex<PostgresStorage<CleanupExpiredTokensJob>>>,
    pub broadcast_end: Arc<Mutex<PostgresStorage<EndBroadcastJob>>>,
}
impl Jobs {
    /// Build all storage instances.
    /// Call once at startup, after `PostgresStorage::setup()` has run migrations.
    pub fn new(pool: &sqlx::PgPool) -> Self {
        Self {
            email: Arc::new(Mutex::new(PostgresStorage::new(pool))),
            broadcast_started: Arc::new(Mutex::new(PostgresStorage::new(pool))),
            broadcast_scheduled: Arc::new(Mutex::new(PostgresStorage::new(pool))),
            cleanup: Arc::new(Mutex::new(PostgresStorage::new(pool))),
            broadcast_end: Arc::new(Mutex::new(PostgresStorage::new(pool))),
        }
    }

    pub async fn push_email(&self, job: SendEmailJob) -> Result<()> {
        self.email.lock().await.push(job).await?;
        Ok(())
    }
    pub async fn push_broadcast_started_fanout(
        &self,
        job: BroadcastStartedFanOutJob,
    ) -> Result<()> {
        self.broadcast_started.lock().await.push(job).await?;
        Ok(())
    }
    pub async fn push_broadcast_scheduled_fanout(
        &self,
        job: BroadcastScheduledFanOutJob,
    ) -> Result<()> {
        self.broadcast_scheduled.lock().await.push(job).await?;
        Ok(())
    }
    pub async fn push_broadcast_end(&self, job: EndBroadcastJob) -> Result<()> {
        self.broadcast_end.lock().await.push(job).await?;
        Ok(())
    }
    pub async fn push_cleanup(&self, job: CleanupExpiredTokensJob) -> Result<()> {
        self.cleanup.lock().await.push(job).await?;
        Ok(())
    }
}
