//! What the subscribers module needs from storage, and nothing else.
//!
//! Same split as the other modules: [`pg`] is Postgres, [`memory`] is the §10 double, and
//! [`SubscribersRepo`] is the only name the service knows. `state.rs` chooses between them
//! (§5.6), so neither the service nor a handler can reach for `sqlx::PgPool`.
//!
//! # Errors
//!
//! Every method returns [`MenoError`]. A `sqlx::Error` never crosses this boundary: it
//! becomes [`MenoError::Internal`] with the operation named, so driver text reaches the log
//! and not the client (§4.2).
//!
//! # `master`'s repository, and what changed
//!
//! - **The annotation is one statement, not two.** `master`'s `find_subscribers` read the
//!   page, then called `batch_is_subscribed` with the page's ids and mutated every row in
//!   Rust. The viewer's relationship is now an `EXISTS` in the page query, so one call
//!   answers a page and the extra method is gone.
//! - **Two page methods became one.** `find_subscribers` and `find_subscriptions` differed
//!   only in which `user_subscribers` column held the owner and which held the row's user.
//!   [`FollowDirection::owner_is_subscription`] decides both, and
//!   [`SubscribersRepo::find_follows`] is one method.
//! - **Writes report what they changed.** `master`'s `create`/`delete` returned `()`, so the
//!   service could not tell "nothing to do" from "nothing was there" without a second
//!   lookup. Both now return whether a row changed, which is what makes the 404 in the
//!   service cost a query only when nothing changed.

pub mod pg;

// The in-memory repository is a real implementation of this module's trait, not test
// scaffolding: it backs the `tests/` integration suite, which links `meno_api` as an
// external crate where `#[cfg(test)]` is false. Gated on the feature so a production build
// still leaves it out.
#[cfg(any(test, feature = "test-support"))]
pub mod memory;

#[cfg(any(test, feature = "test-support"))]
pub use memory::InMemorySubscribersRepo;
pub use pg::PgSubscribersRepo;

use async_trait::async_trait;
use meno_core::Error as MenoError;
use time::OffsetDateTime;
use uuid::Uuid;

use super::dto::{FollowDirection, SubscriberDto};

/// Storage's reading of the wire's [`FollowDirection`].
///
/// Written as a method on the direction rather than duplicated in each adapter, because
/// the two adapters must agree about which `user_subscribers` column is the *owner*: the
/// statement that gets it wrong returns an empty list, which looks exactly like a user
/// nobody follows.
impl FollowDirection {
    /// Whether `user_subscribers.subscription_id` identifies the list's owner.
    ///
    /// True for [`FollowDirection::Followers`]: `subscription_id` is the person being
    /// followed, and a followers list is the rows pointing at them. False for
    /// [`FollowDirection::Following`], where the owner is `subscriber_id` — the follower.
    #[must_use]
    pub const fn owner_is_subscription(self) -> bool {
        matches!(self, Self::Followers)
    }
}

/// The subscribers module's storage.
#[async_trait]
pub trait SubscribersRepo: Send + Sync + std::fmt::Debug {
    /// Record a follow, and answer whether a row was inserted.
    ///
    /// `false` means the caller was already following `target_id`, *or* that no live
    /// account carries `target_id` — the statement guards on the user's existence, so the
    /// second case is a `false` here rather than the foreign-key violation a plain insert
    /// would raise. The caller distinguishes them with [`user_exists`](Self::user_exists),
    /// which is a query only when this returned `false`. A repeat of an insert that
    /// changed nothing therefore costs one statement and does not move the counters the
    /// trigger maintains.
    async fn create(&self, follower_id: Uuid, target_id: Uuid) -> Result<bool, MenoError>;

    /// Remove a follow, and answer whether a row was removed.
    ///
    /// `false` means there was nothing to remove — which is a success, not a conflict:
    /// `DELETE` is idempotent, and a client that retried a tap must not be told it failed.
    /// The caller uses the same `user_exists` miss path as [`create`](Self::create) to
    /// answer 404 for an id that names no account.
    async fn remove(&self, follower_id: Uuid, target_id: Uuid) -> Result<bool, MenoError>;

    /// Whether a live account carries this id.
    ///
    /// Soft-deleted rows are not live: a bearer token minted before a deletion must not
    /// keep following on behalf of a deleted account.
    async fn user_exists(&self, user_id: Uuid) -> Result<bool, MenoError>;

    /// One page of a user's followers or followees.
    ///
    /// The returned rows are annotated for `filter.viewer_id`, never for the owner: a
    /// followers list shows the caller which of those people the caller also follows.
    async fn find_follows(&self, filter: &FollowFilter) -> Result<Vec<SubscriberDto>, MenoError>;
}

/// The page a follow list is read with, already decoded.
#[derive(Debug, Clone, Copy)]
pub struct FollowFilter {
    /// Whose list is being read.
    pub owner_id: Uuid,
    /// Who is looking — the `is_following` column answers for them.
    pub viewer_id: Uuid,
    /// Which of the two lists.
    pub direction: FollowDirection,
    /// The page size, plus the one probe row.
    pub limit_plus_one: i64,
    /// The exclusive keyset boundary from the previous page.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
}

#[cfg(test)]
mod tests {
    //! The direction mapping — the one thing both adapters hang off.

    use super::*;

    #[test]
    fn each_direction_names_the_other_sides_column() {
        assert!(
            FollowDirection::Followers.owner_is_subscription(),
            "a followers list is the rows whose subscription_id is the owner"
        );
        assert!(
            !FollowDirection::Following.owner_is_subscription(),
            "a following list is the rows whose subscriber_id is the owner"
        );
    }
}
