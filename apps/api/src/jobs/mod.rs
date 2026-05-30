use crate::jobs::email_jobs::{SendResetPasswordEmailJob, SendVerificationEmailJob};

use anyhow::Result;
use apalis::prelude::TaskSink;
use apalis_postgres::PostgresStorage;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod email_jobs;
pub mod monitor;

#[derive(Clone)]
pub struct JobQueue {
    pub verify_emails: Arc<Mutex<PostgresStorage<SendVerificationEmailJob>>>,
    pub reset_emails: Arc<Mutex<PostgresStorage<SendResetPasswordEmailJob>>>,
}
impl JobQueue {
    /// Build all storage instances.
    /// Call once at startup, after `PostgresStorage::setup()` has run migrations.
    pub fn new(pool: &sqlx::PgPool) -> Self {
        Self {
            verify_emails: Arc::new(Mutex::new(PostgresStorage::new(pool))),
            reset_emails: Arc::new(Mutex::new(PostgresStorage::new(pool))),
        }
    }

    pub async fn push_verify_email(&self, job: SendVerificationEmailJob) -> Result<()> {
        self.verify_emails.lock().await.push(job).await?;
        Ok(())
    }

    pub async fn push_reset_email(&self, job: SendResetPasswordEmailJob) -> Result<()> {
        self.reset_emails.lock().await.push(job).await?;
        Ok(())
    }
}
