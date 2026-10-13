//! The in-memory double of [`SubscribersRepo`] (§10).
//!
//! The point of this file is that it is *not* more permissive than the SQL: `create`
//! inserts only when the target is a live account and only once, `remove` reports whether a
//! row went, `user_exists` ignores nothing but the absent, and `find_follows` filters the
//! owner's side, joins only live users, cuts the page in the same newest-first order the
//! index serves, and annotates for the viewer. A test that passes here describes something
//! the statements really do.
//!
//! # The two things it does not mirror
//!
//! - **`user_subscribers_no_self_follow`.** The CHECK constraint is unreachable from the
//!   service, which refuses a self-follow before the repository is called; a double that
//!   enforced it would be testing its own copy of a rule the service already owns.
//! - **The counter trigger.** `users.followers` and `users.following` are columns of a
//!   table this double does not carry, and the trigger that maintains them is
//!   `subscribers_repository_pg.rs`'s subject — it asserts the counters move against a real
//!   Postgres, which is the only place they can be asserted at all.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

use time::OffsetDateTime;
use uuid::Uuid;

use super::{FollowFilter, SubscribersRepo};
use crate::modules::subscribers::dto::{FollowDirection, SubscriberDto};
use meno_core::Error as MenoError;

/// A live account, as the `users` join sees it.
#[derive(Debug, Clone)]
struct User {
    full_name: String,
    bio: Option<String>,
    avatar_url: Option<String>,
}

/// One row of `user_subscribers`.
#[derive(Debug, Clone)]
struct Follow {
    follower_id: Uuid,
    target_id: Uuid,
    created_at: OffsetDateTime,
}

/// Everything the double remembers, behind one lock.
#[derive(Debug, Default)]
struct State {
    /// Live accounts, by id. A soft-deleted account is simply absent.
    users: HashMap<Uuid, User>,
    /// Every follow ever recorded.
    follows: Vec<Follow>,
}

/// The double the subscribers suites run against.
#[derive(Debug, Default)]
pub struct InMemorySubscribersRepo {
    state: Mutex<State>,
}

impl InMemorySubscribersRepo {
    /// An empty repository.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a live account, bypassing every guard.
    ///
    /// A fixture, not behaviour: the double has no registration flow, and a followers list
    /// needs accounts to join against.
    pub fn seed_user(
        &self,
        user_id: Uuid,
        full_name: &str,
        bio: Option<&str>,
        avatar_url: Option<&str>,
    ) {
        self.lock().users.insert(
            user_id,
            User {
                full_name: full_name.to_owned(),
                bio: bio.map(str::to_owned),
                avatar_url: avatar_url.map(str::to_owned),
            },
        );
    }

    /// Record a follow with a caller-chosen timestamp, bypassing every guard.
    ///
    /// A fixture: it exists so a list can be paginated against known instants without a
    /// sleep, and so a follow can be seeded for accounts that were never "registered".
    pub fn seed_follow(&self, follower_id: Uuid, target_id: Uuid, created_at: OffsetDateTime) {
        self.lock().follows.push(Follow {
            follower_id,
            target_id,
            created_at,
        });
    }

    /// How many follow rows the double holds.
    #[must_use]
    pub fn stored_follow_count(&self) -> usize {
        self.lock().follows.len()
    }

    /// Whether `follower_id` follows `target_id` — for assertions that do not go through
    /// the trait.
    #[must_use]
    pub fn follows(&self, follower_id: Uuid, target_id: Uuid) -> bool {
        self.lock()
            .follows
            .iter()
            .any(|f| f.follower_id == follower_id && f.target_id == target_id)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The id of the user a follow row *describes*, given the list it is being read for.
///
/// A followers list describes the follower; a following list describes the followee. This
/// is the double's half of the column choice [`super::pg::PgSubscribersRepo`] makes.
fn described_user(follow: &Follow, direction: FollowDirection) -> Uuid {
    if direction.owner_is_subscription() {
        follow.follower_id
    } else {
        follow.target_id
    }
}

/// Whether this follow row belongs in the list `owner_id` is reading.
fn is_owned_by(follow: &Follow, owner_id: Uuid, direction: FollowDirection) -> bool {
    if direction.owner_is_subscription() {
        follow.target_id == owner_id
    } else {
        follow.follower_id == owner_id
    }
}

#[async_trait::async_trait]
impl SubscribersRepo for InMemorySubscribersRepo {
    async fn create(&self, follower_id: Uuid, target_id: Uuid) -> Result<bool, MenoError> {
        let mut state = self.lock();

        // The statement's `WHERE EXISTS (SELECT 1 FROM users …)`.
        if !state.users.contains_key(&target_id) {
            return Ok(false);
        }
        // The statement's `ON CONFLICT (subscriber_id, subscription_id) DO NOTHING`.
        if state
            .follows
            .iter()
            .any(|f| f.follower_id == follower_id && f.target_id == target_id)
        {
            return Ok(false);
        }

        state.follows.push(Follow {
            follower_id,
            target_id,
            created_at: OffsetDateTime::now_utc(),
        });
        Ok(true)
    }

    async fn remove(&self, follower_id: Uuid, target_id: Uuid) -> Result<bool, MenoError> {
        let mut state = self.lock();
        let before = state.follows.len();
        state
            .follows
            .retain(|f| !(f.follower_id == follower_id && f.target_id == target_id));
        Ok(state.follows.len() < before)
    }

    async fn user_exists(&self, user_id: Uuid) -> Result<bool, MenoError> {
        Ok(self.lock().users.contains_key(&user_id))
    }

    async fn find_follows(&self, filter: &FollowFilter) -> Result<Vec<SubscriberDto>, MenoError> {
        let state = self.lock();

        // The `EXISTS` subquery, hoisted out of the row loop the way the statement's
        // planner hoists it: one set of the viewer's followees, then a lookup per row.
        let viewer_follows: HashSet<Uuid> = state
            .follows
            .iter()
            .filter(|f| f.follower_id == filter.viewer_id)
            .map(|f| f.target_id)
            .collect();

        let limit = usize::try_from(filter.limit_plus_one).unwrap_or(usize::MAX);

        let mut rows: Vec<SubscriberDto> = state
            .follows
            .iter()
            .filter(|f| is_owned_by(f, filter.owner_id, filter.direction))
            .filter(|f| match filter.cursor {
                None => true,
                Some((ts, id)) => (f.created_at, described_user(f, filter.direction)) < (ts, id),
            })
            // The `JOIN users … AND u.deleted_at IS NULL`: no account, no row.
            .filter(|f| {
                state
                    .users
                    .contains_key(&described_user(f, filter.direction))
            })
            .map(|follow| {
                let user_id = described_user(follow, filter.direction);
                let user = &state.users[&user_id];
                SubscriberDto {
                    id: user_id,
                    full_name: user.full_name.clone(),
                    bio: user.bio.clone(),
                    avatar_url: user.avatar_url.clone(),
                    is_following: viewer_follows.contains(&user_id),
                    subscribed_at: follow.created_at,
                }
            })
            .collect();

        // `ORDER BY us.created_at DESC, us.<counterpart> DESC`, written as the ascending
        // sort the statement's index serves and then reversed — the same shape the notes
        // double uses.
        rows.sort_by_key(|row| (row.subscribed_at, row.id));
        rows.reverse();
        rows.truncate(limit);
        Ok(rows)
    }
}
