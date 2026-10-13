//! The subscribers rules, as orchestration (§5.1's `service` row).
//!
//! # What changed from `master`
//!
//! - **It holds one trait object, not a pool and two collaborators.** `master`'s service
//!   carried `Arc<dyn SubscribersRepo>`, an `Arc<dyn IdentityReader>` and an
//!   `Arc<NotificationService>`; the last two existed for a user lookup and a notification,
//!   and the lookup is now a method on the repository. One dependency, one thing to
//!   substitute — which is what lets `tests/subscribers_service.rs` run the whole rule set
//!   with no Postgres.
//! - **One page method, not four.** `get_my_subscribers`, `get_my_subscriptions`,
//!   `get_user_subscribers` and `get_user_subscriptions` were the same body four times,
//!   differing in which direction they read and whether the owner was the caller. [`page`]
//!   takes both, and the handler passes `auth.id` twice when they are equal.
//! - **The 404 costs a query only when nothing happened.** Both writes answer whether they
//!   changed a row; the second query, which turns "nothing changed" into "there is no such
//!   user", runs only in that case.
//! - **Unfollowing yourself is a success.** `master` answered
//!   `CANNOT_SUBSCRIBE_TO_SELF` here too — a copy-paste of the subscribe guard, with a
//!   message about subscribing. There is no row to delete (the migration's CHECK constraint
//!   makes one impossible), so the postcondition the caller asked for already holds and
//!   `DELETE` stays idempotent. Only [`subscribe`] refuses, and only for the verb it names.
//!
//! [`page`]: SubscribersService::page
//! [`subscribe`]: SubscribersService::subscribe

use std::sync::Arc;

use meno_core::{Cursor, CursorPage, CursorParams, Error as MenoError};
use uuid::Uuid;

use super::dto::{FollowDirection, SubscriberDto};
use super::error;
use super::repository::{FollowFilter, SubscribersRepo};

/// The subscribers rules.
#[derive(Clone)]
pub struct SubscribersService {
    repo: Arc<dyn SubscribersRepo>,
}

impl SubscribersService {
    /// Build the service over its repository.
    #[must_use]
    pub fn new(repo: Arc<dyn SubscribersRepo>) -> Self {
        Self { repo }
    }

    /// Follow `target_id`.
    ///
    /// Idempotent: following somebody twice is one row, and the counters the trigger
    /// maintains do not move twice. This is also the call site the `notifications` module
    /// attaches to when it lands (`master` sent a `USER_SUBSCRIBED` row from here).
    ///
    /// # Errors
    ///
    /// [`error::cannot_subscribe_to_self`] as a `400`; [`error::user_not_found`] as a `404`
    /// when no live account carries `target_id`.
    pub async fn subscribe(&self, follower_id: Uuid, target_id: Uuid) -> Result<(), MenoError> {
        if follower_id == target_id {
            return Err(error::cannot_subscribe_to_self());
        }

        if !self.repo.create(follower_id, target_id).await?
            && !self.repo.user_exists(target_id).await?
        {
            return Err(error::user_not_found());
        }
        Ok(())
    }

    /// Stop following `target_id`.
    ///
    /// Idempotent: a follow that is not there is not an error, so a retried tap is a
    /// success rather than a `404` the client would have to interpret.
    ///
    /// # Errors
    ///
    /// [`error::user_not_found`] as a `404` when no live account carries `target_id`.
    pub async fn unsubscribe(&self, follower_id: Uuid, target_id: Uuid) -> Result<(), MenoError> {
        if !self.repo.remove(follower_id, target_id).await?
            && !self.repo.user_exists(target_id).await?
        {
            return Err(error::user_not_found());
        }
        Ok(())
    }

    /// One page of `owner_id`'s followers or followees, annotated for `viewer_id`.
    ///
    /// The cursor decodes here, where a bad one becomes a `400` through
    /// `From<CursorError> for Error`, and binds in `repository` — a half-decoded cursor
    /// never reaches SQL.
    ///
    /// # Errors
    ///
    /// [`error::user_not_found`] when no live account carries `owner_id`; a `400`
    /// `INVALID_CURSOR` for a malformed cursor; a repository failure otherwise.
    pub async fn page(
        &self,
        direction: FollowDirection,
        owner_id: Uuid,
        viewer_id: Uuid,
        params: &CursorParams,
    ) -> Result<CursorPage<SubscriberDto>, MenoError> {
        let cursor = params
            .cursor
            .as_ref()
            .map(Cursor::to_timestamp_id)
            .transpose()?;

        let rows = self
            .repo
            .find_follows(&FollowFilter {
                owner_id,
                viewer_id,
                direction,
                cursor,
                limit_plus_one: params.limit_plus_one(),
            })
            .await?;

        // An empty page is either the end of a list or a user who does not exist, and only
        // the second is a 404. Asking only when the page came back empty is what keeps the
        // common path at one query.
        if rows.is_empty() && !self.repo.user_exists(owner_id).await? {
            return Err(error::user_not_found());
        }

        Ok(CursorPage::from_rows(rows, params.limit(), |row| {
            Cursor::from_timestamp_id(row.subscribed_at, row.id)
        }))
    }
}
