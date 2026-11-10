//! What the notifications module needs from storage, and nothing else.
//!
//! Same split as the other modules: [`pg`] is Postgres, [`memory`] is the §10
//! double, and [`NotificationsRepo`] is the only name the service knows. `state.rs`
//! chooses between them (§5.6), so neither the service nor a handler can reach for
//! `sqlx::PgPool`.
//!
//! # Errors
//!
//! Every method returns [`MenoError`]. A `sqlx::Error` never crosses this boundary:
//! it becomes [`MenoError::Internal`] with the operation named, so driver text
//! reaches the log and not the client (§4.2).
//!
//! # `master`'s repository, and what changed
//!
//! - **It is a trait, not a struct that owns the pool.** `master`'s service built
//!   its own `NotificationRepository` from the pool, so nothing in the module was
//!   testable without Postgres.
//! - **Device tokens are here, not in a second repository.** The push adapter needs
//!   a user's token and needs to delete a dead one; those two reads are part of this
//!   module's own storage, exposed through [`RepoTokenStore`].
//! - **`create` answers one id, not a whole row.** The service logs the id and
//!   nothing else, so returning the row was a projection nobody read.
//! - **A page is one statement.** `master`'s page query was one statement too, but
//!   it selected the type code through a join the code already had on the template —
//!   [`pg`] reads `notification_templates.type` directly and joins `broadcasts` for
//!   the title the sentence needs.

pub mod pg;

// The in-memory repository is a real implementation of this module's trait, not test
// scaffolding: it backs the `tests/` integration suite, which links `meno_api` as an
// external crate where `#[cfg(test)]` is false. Gated on the feature so a production
// build still leaves it out.
#[cfg(any(test, feature = "test-support"))]
pub mod memory;

#[cfg(any(test, feature = "test-support"))]
pub use memory::InMemoryNotificationsRepo;
pub use pg::PgNotificationsRepo;

use std::sync::Arc;

use async_trait::async_trait;
use meno_core::Error as MenoError;
use time::OffsetDateTime;
use uuid::Uuid;

use super::model::{NotificationRow, NotificationTemplate};
use crate::infrastructure::push::PushTokenStore;
use crate::infrastructure::push::error::TokenStoreError;
use crate::infrastructure::push::model::PushTarget;

/// The notifications module's storage.
#[async_trait]
pub trait NotificationsRepo: Send + Sync + std::fmt::Debug {
    // ── templates ─────────────────────────────────────────────────────────

    /// The active template for `type_code`, or `None`.
    ///
    /// The service caches this answer; this is the cold path.
    async fn find_template_by_code(
        &self,
        type_code: &str,
    ) -> Result<Option<NotificationTemplate>, MenoError>;

    /// Every active template — the cache warm path.
    async fn find_all_templates(&self) -> Result<Vec<NotificationTemplate>, MenoError>;

    // ── writes ────────────────────────────────────────────────────────────

    /// Insert one notification and return its id.
    async fn create(
        &self,
        owner_id: Uuid,
        template_id: Uuid,
        actor_id: Option<Uuid>,
        broadcast_id: Option<Uuid>,
    ) -> Result<Uuid, MenoError>;

    /// Insert one notification per owner in a single statement, returning how many
    /// rows were written.
    async fn create_bulk(
        &self,
        owner_ids: &[Uuid],
        template_id: Uuid,
        actor_id: Option<Uuid>,
        broadcast_id: Option<Uuid>,
    ) -> Result<u64, MenoError>;

    /// Mark one notification read, scoped to its owner.
    ///
    /// Answers whether a row changed. `false` covers both "already read" and "not
    /// there"; the caller distinguishes them with [`exists`](Self::exists).
    async fn mark_read(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError>;

    /// Mark every unread notification for `owner_id` read, returning how many.
    async fn mark_all_read(&self, owner_id: Uuid) -> Result<u64, MenoError>;

    /// Hard-delete one notification, scoped to its owner.
    async fn delete(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError>;

    // ── reads ─────────────────────────────────────────────────────────────

    /// One page of an owner's live inbox, newest first.
    async fn find_page(
        &self,
        filter: &NotificationFilter,
    ) -> Result<Vec<NotificationRow>, MenoError>;

    /// Whether a live notification carries this id for this owner.
    async fn exists(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError>;

    /// Whether the notification exists, belongs to the owner, and is unread.
    ///
    /// Asked before a delete so the counter is decremented only when the row was
    /// actually contributing to it.
    async fn is_unread(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError>;

    /// The database's answer to "how many unread", used when the counter is cold.
    async fn count_unread(&self, owner_id: Uuid) -> Result<i64, MenoError>;

    // ── settings, as the push adapter sees them ───────────────────────────

    /// The device token `user_id` last registered, if push is on for them.
    async fn device_token(&self, user_id: Uuid) -> Result<Option<String>, MenoError>;

    /// The device tokens of every user in `user_ids` who has push switched on.
    async fn push_targets(&self, user_ids: &[Uuid]) -> Result<Vec<PushTarget>, MenoError>;

    /// Forget a device token FCM has rejected.
    async fn clear_device_token(&self, user_id: Uuid) -> Result<(), MenoError>;

    /// Whether the recipient wants in-app (WebSocket) notifications.
    async fn app_notifications_enabled(&self, user_id: Uuid) -> Result<bool, MenoError>;
}

/// The page a notification list is read with, already decoded.
#[derive(Debug, Clone, Copy)]
pub struct NotificationFilter {
    /// Whose inbox.
    pub owner_id: Uuid,
    /// Only unread rows.
    pub unread_only: bool,
    /// The page size, plus the one probe row.
    pub limit_plus_one: i64,
    /// The exclusive keyset boundary from the previous page.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
}

/// [`PushTokenStore`] over this module's repository.
///
/// The push adapter is told *which user* through this narrow trait rather than
/// being handed the whole repository (§5.4). It exists because the adapter lives in
/// `infrastructure`, which may not import a module: the module implements the
/// adapter's trait, not the other way round.
#[derive(Debug, Clone)]
pub struct RepoTokenStore {
    repo: Arc<dyn NotificationsRepo>,
}

impl RepoTokenStore {
    /// Wrap the repository the service already holds.
    #[must_use]
    pub fn new(repo: Arc<dyn NotificationsRepo>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl PushTokenStore for RepoTokenStore {
    async fn device_token(&self, user_id: Uuid) -> Result<Option<String>, TokenStoreError> {
        self.repo
            .device_token(user_id)
            .await
            .map_err(|error| TokenStoreError::Lookup(error.to_string()))
    }

    async fn clear_device_token(&self, user_id: Uuid) -> Result<(), TokenStoreError> {
        self.repo
            .clear_device_token(user_id)
            .await
            .map_err(|error| TokenStoreError::Cleanup(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    //! The one behaviour this file owns: the repository's failures become the push
    //! adapter's two failure kinds, and are not silently reported as "no device".

    use super::*;
    use crate::modules::notifications::model::NotificationTemplate;

    #[derive(Debug)]
    struct Broken;

    #[async_trait]
    impl NotificationsRepo for Broken {
        async fn find_template_by_code(
            &self,
            _type_code: &str,
        ) -> Result<Option<NotificationTemplate>, MenoError> {
            Ok(None)
        }

        async fn find_all_templates(&self) -> Result<Vec<NotificationTemplate>, MenoError> {
            Ok(Vec::new())
        }

        async fn create(
            &self,
            _owner_id: Uuid,
            _template_id: Uuid,
            _actor_id: Option<Uuid>,
            _broadcast_id: Option<Uuid>,
        ) -> Result<Uuid, MenoError> {
            Ok(Uuid::nil())
        }

        async fn create_bulk(
            &self,
            _owner_ids: &[Uuid],
            _template_id: Uuid,
            _actor_id: Option<Uuid>,
            _broadcast_id: Option<Uuid>,
        ) -> Result<u64, MenoError> {
            Ok(0)
        }

        async fn mark_read(
            &self,
            _notification_id: Uuid,
            _owner_id: Uuid,
        ) -> Result<bool, MenoError> {
            Ok(false)
        }

        async fn mark_all_read(&self, _owner_id: Uuid) -> Result<u64, MenoError> {
            Ok(0)
        }

        async fn delete(&self, _notification_id: Uuid, _owner_id: Uuid) -> Result<bool, MenoError> {
            Ok(false)
        }

        async fn find_page(
            &self,
            _filter: &NotificationFilter,
        ) -> Result<Vec<NotificationRow>, MenoError> {
            Ok(Vec::new())
        }

        async fn exists(&self, _notification_id: Uuid, _owner_id: Uuid) -> Result<bool, MenoError> {
            Ok(false)
        }

        async fn is_unread(
            &self,
            _notification_id: Uuid,
            _owner_id: Uuid,
        ) -> Result<bool, MenoError> {
            Ok(false)
        }

        async fn count_unread(&self, _owner_id: Uuid) -> Result<i64, MenoError> {
            Ok(0)
        }

        async fn device_token(&self, _user_id: Uuid) -> Result<Option<String>, MenoError> {
            Err(MenoError::Internal {
                context: "device_token",
                detail: "pool timed out".to_owned(),
            })
        }

        async fn push_targets(&self, _user_ids: &[Uuid]) -> Result<Vec<PushTarget>, MenoError> {
            Ok(Vec::new())
        }

        async fn clear_device_token(&self, _user_id: Uuid) -> Result<(), MenoError> {
            Err(MenoError::Internal {
                context: "clear_device_token",
                detail: "deadlock".to_owned(),
            })
        }

        async fn app_notifications_enabled(&self, _user_id: Uuid) -> Result<bool, MenoError> {
            Ok(false)
        }
    }

    #[tokio::test]
    async fn a_lookup_failure_is_not_reported_as_a_quiet_user() {
        let store = RepoTokenStore::new(Arc::new(Broken));

        let error = store
            .device_token(Uuid::from_u128(1))
            .await
            .expect_err("a failed lookup must not look like 'no device'");

        assert!(matches!(error, TokenStoreError::Lookup(_)), "{error:?}");
    }

    #[tokio::test]
    async fn a_cleanup_failure_is_reported_as_one() {
        let store = RepoTokenStore::new(Arc::new(Broken));

        let error = store
            .clear_device_token(Uuid::from_u128(1))
            .await
            .expect_err("a failed delete is not a success");

        assert!(matches!(error, TokenStoreError::Cleanup(_)), "{error:?}");
    }
}
