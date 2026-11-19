//! The in-memory [`ModerationRepo`], for the §10 service suite.
//!
//! Not scaffolding: this is a real implementation of the trait, and it is what lets the
//! whole moderation rule set run with no Postgres. It reproduces the decisions the
//! migration and the SQL make — the self-block constraint, "an unknown subject inserts
//! nothing", "a repeated block is one row" — because a double that is more permissive
//! than the adapter makes every service test pass for the wrong reason.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

use super::{ModerationRepo, ReportFilter};
use crate::modules::moderation::model::{BlockedUser, NewReport, Report, ReportStatus};
use meno_core::Error as MenoError;

/// One stored block.
#[derive(Debug, Clone)]
struct Block {
    blocker_id: Uuid,
    blocked_id: Uuid,
    created_at: OffsetDateTime,
}

/// A live account the double knows about.
#[derive(Debug, Clone)]
struct Account {
    full_name: String,
}

#[derive(Debug, Default)]
struct State {
    /// Accounts the double can answer `user_exists` for, by id.
    users: HashMap<Uuid, Account>,
    /// Broadcast ids the double can answer `broadcast_exists` for.
    broadcasts: HashSet<Uuid>,
    /// Blocks, in insertion order — enough to answer the list in newest-first order.
    blocks: Vec<Block>,
    /// Reports, oldest first: the page is reversed on read.
    reports: Vec<Report>,
}

/// The double.
#[derive(Debug, Clone, Default)]
pub struct InMemoryModerationRepo {
    state: Arc<Mutex<State>>,
}

impl InMemoryModerationRepo {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a live account.
    pub fn add_user(&self, id: Uuid, full_name: &str) {
        self.lock().users.insert(
            id,
            Account {
                full_name: full_name.to_owned(),
            },
        );
    }

    /// Seed a broadcast.
    pub fn add_broadcast(&self, id: Uuid) {
        self.lock().broadcasts.insert(id);
    }

    /// Every stored block, as `(blocker, blocked)`.
    #[must_use]
    pub fn blocks(&self) -> Vec<(Uuid, Uuid)> {
        self.lock()
            .blocks
            .iter()
            .map(|block| (block.blocker_id, block.blocked_id))
            .collect()
    }

    /// Every stored report, oldest first.
    #[must_use]
    pub fn stored_reports(&self) -> Vec<Report> {
        self.lock().reports.clone()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A poisoned lock is another test's panic; the state it guards is still exactly
        // what was written, and every other module's double recovers the same way.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait]
impl ModerationRepo for InMemoryModerationRepo {
    async fn block(&self, blocker_id: Uuid, blocked_id: Uuid) -> Result<bool, MenoError> {
        let mut state = self.lock();

        // The migration's `CHECK` and the SQL's `WHERE EXISTS`, together.
        if blocker_id == blocked_id || !state.users.contains_key(&blocked_id) {
            return Ok(false);
        }
        if state
            .blocks
            .iter()
            .any(|b| b.blocker_id == blocker_id && b.blocked_id == blocked_id)
        {
            return Ok(false);
        }

        state.blocks.push(Block {
            blocker_id,
            blocked_id,
            created_at: OffsetDateTime::now_utc(),
        });
        Ok(true)
    }

    async fn unblock(&self, blocker_id: Uuid, blocked_id: Uuid) -> Result<bool, MenoError> {
        let mut state = self.lock();
        let before = state.blocks.len();
        state
            .blocks
            .retain(|b| !(b.blocker_id == blocker_id && b.blocked_id == blocked_id));
        Ok(state.blocks.len() != before)
    }

    async fn blocked(&self, blocker_id: Uuid) -> Result<Vec<BlockedUser>, MenoError> {
        let state = self.lock();

        let mut rows: Vec<BlockedUser> = state
            .blocks
            .iter()
            .filter(|b| b.blocker_id == blocker_id)
            .filter_map(|block| {
                state
                    .users
                    .get(&block.blocked_id)
                    .map(|account| BlockedUser {
                        id: block.blocked_id,
                        full_name: account.full_name.clone(),
                        blocked_at: block.created_at,
                    })
            })
            .collect();

        rows.sort_by(|a, b| {
            b.blocked_at
                .cmp(&a.blocked_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(rows)
    }

    async fn user_exists(&self, user_id: Uuid) -> Result<bool, MenoError> {
        Ok(self.lock().users.contains_key(&user_id))
    }

    async fn broadcast_exists(&self, broadcast_id: Uuid) -> Result<bool, MenoError> {
        Ok(self.lock().broadcasts.contains(&broadcast_id))
    }

    async fn create_report(&self, new: NewReport<'_>) -> Result<Report, MenoError> {
        let report = Report {
            id: Uuid::new_v4(),
            reporter_id: new.reporter_id,
            subject_type: new.subject_type,
            subject_id: new.subject_id,
            reason: new.reason.to_owned(),
            details: new.details.map(str::to_owned),
            status: ReportStatus::Open,
            created_at: OffsetDateTime::now_utc(),
            resolved_at: None,
        };

        self.lock().reports.push(report.clone());
        Ok(report)
    }

    async fn reports(&self, filter: ReportFilter) -> Result<Vec<Report>, MenoError> {
        let state = self.lock();

        let mut rows: Vec<Report> = state
            .reports
            .iter()
            .filter(|report| filter.status.is_none_or(|status| report.status == status))
            .filter(|report| match filter.cursor {
                None => true,
                Some((at, id)) => (report.created_at, report.id) < (at, id),
            })
            .cloned()
            .collect();

        rows.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        rows.truncate(filter.limit_plus_one.max(0) as usize);
        Ok(rows)
    }

    async fn report(&self, id: Uuid) -> Result<Option<Report>, MenoError> {
        Ok(self
            .lock()
            .reports
            .iter()
            .find(|report| report.id == id)
            .cloned())
    }

    async fn resolve_report(&self, id: Uuid, status: ReportStatus) -> Result<bool, MenoError> {
        let mut state = self.lock();
        let Some(report) = state.reports.iter_mut().find(|report| report.id == id) else {
            return Ok(false);
        };

        report.status = status;
        report.resolved_at = Some(OffsetDateTime::now_utc());
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    //! The double's own behaviour, where a divergence from the adapter would matter.

    use super::*;
    use crate::modules::moderation::model::ReportSubject;

    fn ada() -> Uuid {
        Uuid::from_u128(1)
    }

    fn grace() -> Uuid {
        Uuid::from_u128(2)
    }

    #[tokio::test]
    async fn a_self_block_is_impossible_and_a_repeat_is_one_row() {
        let repo = InMemoryModerationRepo::new();
        repo.add_user(ada(), "Ada Lovelace");
        repo.add_user(grace(), "Grace Hopper");

        assert!(
            !repo.block(ada(), ada()).await.expect("the attempt runs"),
            "the migration's CHECK makes a self-block unrepresentable"
        );
        assert!(repo.block(ada(), grace()).await.expect("the block runs"));
        assert!(
            !repo.block(ada(), grace()).await.expect("a repeat runs"),
            "a repeated block is the same decision"
        );
        assert_eq!(repo.blocks(), vec![(ada(), grace())]);
    }

    #[tokio::test]
    async fn an_unknown_account_is_not_blocked_the_way_the_sql_refuses_it() {
        let repo = InMemoryModerationRepo::new();
        repo.add_user(ada(), "Ada Lovelace");

        assert!(
            !repo
                .block(ada(), Uuid::from_u128(9))
                .await
                .expect("the attempt runs"),
            "the SQL's WHERE EXISTS inserts nothing for an unknown id"
        );
        assert!(repo.blocks().is_empty());
    }

    #[tokio::test]
    async fn a_report_starts_open_and_a_resolution_stamps_it() {
        let repo = InMemoryModerationRepo::new();

        let report = repo
            .create_report(NewReport {
                reporter_id: ada(),
                subject_type: ReportSubject::User,
                subject_id: grace(),
                reason: "spam",
                details: None,
            })
            .await
            .expect("the insert runs");
        assert_eq!(report.status, ReportStatus::Open);
        assert_eq!(report.resolved_at, None);

        assert!(
            repo.resolve_report(report.id, ReportStatus::Dismissed)
                .await
                .expect("the update runs")
        );
        let stored = &repo.stored_reports()[0];
        assert_eq!(stored.status, ReportStatus::Dismissed);
        assert!(stored.resolved_at.is_some());
    }
}
