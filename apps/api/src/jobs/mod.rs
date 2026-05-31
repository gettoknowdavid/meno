use crate::jobs::email_job::SendEmailJob;

use anyhow::Result;
use apalis::prelude::TaskSink;
use apalis_postgres::PostgresStorage;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod email_job;
pub mod monitor;

#[derive(Clone)]
pub struct JobQueue {
    pub email: Arc<Mutex<PostgresStorage<SendEmailJob>>>,
}
impl JobQueue {
    /// Build all storage instances.
    /// Call once at startup, after `PostgresStorage::setup()` has run migrations.
    pub fn new(pool: &sqlx::PgPool) -> Self {
        Self {
            email: Arc::new(Mutex::new(PostgresStorage::new(pool))),
        }
    }

    pub async fn push_email(&self, job: SendEmailJob) -> Result<()> {
        self.email.lock().await.push(job).await?;
        Ok(())
    }
}
