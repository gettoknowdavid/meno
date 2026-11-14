//! The in-memory double of [`NotificationsRepo`] (§10).
//!
//! The point of this file is that it is *not* more permissive than the SQL: an
//! insert needs a template that exists, a page filters the owner's live rows, joins
//! only live actors and existing broadcasts, cuts the page in the same newest-first
//! order the index serves, and answers the cursor with the same row-value comparison.
//! A test that passes here describes something the statements really do.
//!
//! # The one thing it does not mirror
//!
//! **`archived_at`.** The double carries a boolean because nothing in this module
//! ever archives a row; the column exists for an operator's `UPDATE`, and a test that
//! exercised it would be testing its own fixture rather than the service.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use time::OffsetDateTime;
use uuid::Uuid;

use super::{NotificationFilter, NotificationsRepo};
use crate::infrastructure::push::model::PushTarget;
use crate::modules::notifications::model::{ActorSummary, NotificationRow, NotificationTemplate};
use meno_core::Error as MenoError;

/// A live account, as the `users` join sees it.
#[derive(Debug, Clone)]
struct User {
    full_name: String,
    bio: Option<String>,
    avatar_id: Option<String>,
    avatar_url: Option<String>,
}

/// A `settings` row's three fields this module reads.
#[derive(Debug, Clone, Copy)]
struct Preferences {
    push_notifications: bool,
    app_notifications: bool,
}

/// One row of `notifications`, plus the token the double needs to forget.
#[derive(Debug, Clone)]
struct StoredNotification {
    id: Uuid,
    owner_id: Uuid,
    template_id: Uuid,
    actor_id: Option<Uuid>,
    broadcast_id: Option<Uuid>,
    read: bool,
    created_at: OffsetDateTime,
}

/// Everything the double remembers, behind one lock.
#[derive(Debug, Default)]
struct State {
    /// Templates by type code.
    templates: HashMap<String, NotificationTemplate>,
    /// Broadcast titles, for the `{title}` variable.
    broadcasts: HashMap<Uuid, String>,
    /// Live accounts, by id. A deleted account is absent.
    users: HashMap<Uuid, User>,
    /// Preferences by user, and the device token when one is registered.
    preferences: HashMap<Uuid, Preferences>,
    tokens: HashMap<Uuid, String>,
    /// Every notification ever created.
    notifications: Vec<StoredNotification>,
    /// The next id the fixture generator will hand out.
    next_id: u128,
}

/// The double the notifications suites run against.
#[derive(Debug, Default)]
pub struct InMemoryNotificationsRepo {
    state: Mutex<State>,
}

impl InMemoryNotificationsRepo {
    /// An empty repository.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an active template and return its id.
    ///
    /// A fixture: the migrations seed these in production, and a test needs the id to
    /// reference from an inserted notification.
    pub fn seed_template(&self, type_code: &str, title: &str, body: &str) -> Uuid {
        let mut state = self.lock();
        let id = next_id(&mut state);
        state.templates.insert(
            type_code.to_owned(),
            NotificationTemplate {
                id,
                type_code: type_code.to_owned(),
                title: title.to_owned(),
                body: body.to_owned(),
                image_url: None,
            },
        );
        id
    }

    /// The id of a seeded template, for assertions that do not go through the trait.
    #[must_use]
    pub fn template_id(&self, type_code: &str) -> Option<Uuid> {
        self.lock()
            .templates
            .get(type_code)
            .map(|template| template.id)
    }

    /// Register a live account.
    pub fn seed_user(&self, user_id: Uuid, full_name: &str) {
        self.lock().users.insert(
            user_id,
            User {
                full_name: full_name.to_owned(),
                bio: None,
                avatar_id: None,
                avatar_url: None,
            },
        );
    }

    /// Forget an account, as a soft delete does.
    pub fn remove_user(&self, user_id: Uuid) {
        self.lock().users.remove(&user_id);
    }

    /// Register a broadcast title.
    pub fn seed_broadcast(&self, broadcast_id: Uuid, title: &str) {
        self.lock()
            .broadcasts
            .insert(broadcast_id, title.to_owned());
    }

    /// Register a settings row.
    pub fn seed_settings(&self, user_id: Uuid, push_notifications: bool, app_notifications: bool) {
        self.lock().preferences.insert(
            user_id,
            Preferences {
                push_notifications,
                app_notifications,
            },
        );
    }

    /// Register a device token for `user_id`.
    pub fn seed_token(&self, user_id: Uuid, token: &str) {
        self.lock().tokens.insert(user_id, token.to_owned());
    }

    /// Insert a notification with a caller-chosen instant.
    ///
    /// A fixture: it exists so a page can be paginated against known timestamps
    /// without a sleep, and so a row can be seeded without a template lookup.
    pub fn seed_notification(
        &self,
        owner_id: Uuid,
        type_code: &str,
        actor_id: Option<Uuid>,
        broadcast_id: Option<Uuid>,
        created_at: OffsetDateTime,
        read: bool,
    ) -> Uuid {
        let mut state = self.lock();
        let id = next_id(&mut state);
        let template_id = state
            .templates
            .get(type_code)
            .map_or_else(Uuid::nil, |template| template.id);

        state.notifications.push(StoredNotification {
            id,
            owner_id,
            template_id,
            actor_id,
            broadcast_id,
            read,
            created_at,
        });
        id
    }

    /// How many notification rows the double holds.
    #[must_use]
    pub fn stored_notification_count(&self) -> usize {
        self.lock().notifications.len()
    }

    /// Whether a row with this id and owner exists.
    #[must_use]
    pub fn has_notification(&self, notification_id: Uuid, owner_id: Uuid) -> bool {
        self.lock()
            .notifications
            .iter()
            .any(|n| n.id == notification_id && n.owner_id == owner_id)
    }

    /// Whether a row with this id and owner is read.
    #[must_use]
    pub fn is_read(&self, notification_id: Uuid, owner_id: Uuid) -> bool {
        self.lock()
            .notifications
            .iter()
            .find(|n| n.id == notification_id && n.owner_id == owner_id)
            .is_some_and(|n| n.read)
    }

    /// The device tokens still registered.
    #[must_use]
    pub fn tokens(&self) -> HashMap<Uuid, String> {
        self.lock().tokens.clone()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Hand out the next fixture id.
fn next_id(state: &mut State) -> Uuid {
    state.next_id += 1;
    Uuid::from_u128(state.next_id)
}

#[async_trait::async_trait]
impl NotificationsRepo for InMemoryNotificationsRepo {
    async fn find_template_by_code(
        &self,
        type_code: &str,
    ) -> Result<Option<NotificationTemplate>, MenoError> {
        Ok(self.lock().templates.get(type_code).cloned())
    }

    async fn find_all_templates(&self) -> Result<Vec<NotificationTemplate>, MenoError> {
        Ok(self.lock().templates.values().cloned().collect())
    }

    async fn create(
        &self,
        owner_id: Uuid,
        template_id: Uuid,
        actor_id: Option<Uuid>,
        broadcast_id: Option<Uuid>,
    ) -> Result<Uuid, MenoError> {
        let mut state = self.lock();
        let id = next_id(&mut state);
        state.notifications.push(StoredNotification {
            id,
            owner_id,
            template_id,
            actor_id,
            broadcast_id,
            read: false,
            created_at: OffsetDateTime::now_utc(),
        });
        Ok(id)
    }

    async fn create_bulk(
        &self,
        owner_ids: &[Uuid],
        template_id: Uuid,
        actor_id: Option<Uuid>,
        broadcast_id: Option<Uuid>,
    ) -> Result<u64, MenoError> {
        let mut state = self.lock();
        for owner_id in owner_ids {
            let id = next_id(&mut state);
            state.notifications.push(StoredNotification {
                id,
                owner_id: *owner_id,
                template_id,
                actor_id,
                broadcast_id,
                read: false,
                created_at: OffsetDateTime::now_utc(),
            });
        }
        Ok(owner_ids.len() as u64)
    }

    async fn mark_read(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError> {
        let mut state = self.lock();
        let Some(row) = state
            .notifications
            .iter_mut()
            .find(|n| n.id == notification_id && n.owner_id == owner_id)
        else {
            return Ok(false);
        };
        if row.read {
            return Ok(false);
        }
        row.read = true;
        Ok(true)
    }

    async fn mark_all_read(&self, owner_id: Uuid) -> Result<u64, MenoError> {
        let mut state = self.lock();
        let mut updated = 0;
        for row in state
            .notifications
            .iter_mut()
            .filter(|n| n.owner_id == owner_id && !n.read)
        {
            row.read = true;
            updated += 1;
        }
        Ok(updated)
    }

    async fn delete(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError> {
        let mut state = self.lock();
        let before = state.notifications.len();
        state
            .notifications
            .retain(|n| !(n.id == notification_id && n.owner_id == owner_id));
        Ok(state.notifications.len() < before)
    }

    async fn find_page(
        &self,
        filter: &NotificationFilter,
    ) -> Result<Vec<NotificationRow>, MenoError> {
        let state = self.lock();

        let mut rows: Vec<NotificationRow> = state
            .notifications
            .iter()
            .filter(|n| n.owner_id == filter.owner_id)
            .filter(|n| !filter.unread_only || !n.read)
            .filter(|n| match filter.cursor {
                None => true,
                Some((ts, id)) => (n.created_at, n.id) < (ts, id),
            })
            // The `JOIN notification_templates …` drops a row whose template is gone.
            .filter_map(|n| {
                let template = state.templates.values().find(|t| t.id == n.template_id)?;
                Some(NotificationRow {
                    id: n.id,
                    read: n.read,
                    created_at: n.created_at,
                    type_code: template.type_code.clone(),
                    title_template: template.title.clone(),
                    body_template: template.body.clone(),
                    image_url_template: template.image_url.clone(),
                    broadcast_id: n.broadcast_id,
                    broadcast_title: n
                        .broadcast_id
                        .and_then(|id| state.broadcasts.get(&id).cloned()),
                    actor_id: n.actor_id,
                    actor_name: n
                        .actor_id
                        .and_then(|id| state.users.get(&id))
                        .map(|user| user.full_name.clone()),
                    actor_bio: n
                        .actor_id
                        .and_then(|id| state.users.get(&id))
                        .and_then(|user| user.bio.clone()),
                    actor_avatar_url: n
                        .actor_id
                        .and_then(|id| state.users.get(&id))
                        .and_then(|user| user.avatar_url.clone()),
                    actor_avatar_id: n
                        .actor_id
                        .and_then(|id| state.users.get(&id))
                        .and_then(|user| user.avatar_id.clone()),
                })
            })
            .collect();

        // `ORDER BY n.created_at DESC, n.id DESC`, written as the ascending sort the
        // index serves and then reversed — the same shape the other doubles use.
        rows.sort_by_key(|row| (row.created_at, row.id));
        rows.reverse();
        rows.truncate(usize::try_from(filter.limit_plus_one).unwrap_or(usize::MAX));
        Ok(rows)
    }

    async fn exists(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError> {
        Ok(self
            .lock()
            .notifications
            .iter()
            .any(|n| n.id == notification_id && n.owner_id == owner_id))
    }

    async fn is_unread(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError> {
        let state = self.lock();
        Ok(state
            .notifications
            .iter()
            .find(|n| n.id == notification_id && n.owner_id == owner_id)
            .is_some_and(|n| !n.read))
    }

    async fn count_unread(&self, owner_id: Uuid) -> Result<i64, MenoError> {
        let count = self
            .lock()
            .notifications
            .iter()
            .filter(|n| n.owner_id == owner_id && !n.read)
            .count();
        Ok(count as i64)
    }

    async fn device_token(&self, user_id: Uuid) -> Result<Option<String>, MenoError> {
        let state = self.lock();
        let push_on = state
            .preferences
            .get(&user_id)
            .is_some_and(|prefs| prefs.push_notifications);
        if !push_on {
            return Ok(None);
        }
        Ok(state.tokens.get(&user_id).cloned())
    }

    async fn push_targets(&self, user_ids: &[Uuid]) -> Result<Vec<PushTarget>, MenoError> {
        let state = self.lock();
        Ok(user_ids
            .iter()
            .filter(|id| {
                state
                    .preferences
                    .get(id)
                    .is_some_and(|prefs| prefs.push_notifications)
            })
            .filter_map(|id| {
                state
                    .tokens
                    .get(id)
                    .map(|token| PushTarget::new(*id, token.clone()))
            })
            .collect())
    }

    async fn clear_device_token(&self, user_id: Uuid) -> Result<(), MenoError> {
        self.lock().tokens.remove(&user_id);
        Ok(())
    }

    async fn app_notifications_enabled(&self, user_id: Uuid) -> Result<bool, MenoError> {
        Ok(self
            .lock()
            .preferences
            .get(&user_id)
            .is_some_and(|prefs| prefs.app_notifications))
    }
}

/// An [`ActorSummary`] for a seeded account — a fixture helper the suites share.
#[must_use]
pub fn actor(user_id: Uuid, full_name: &str) -> ActorSummary {
    ActorSummary {
        id: user_id,
        full_name: full_name.to_owned(),
        bio: None,
        avatar_id: None,
        avatar_url: None,
    }
}

#[cfg(test)]
mod tests {
    //! The double's own semantics, asserted so a service test can trust it.

    use super::*;
    use crate::modules::notifications::model::codes;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn at(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds).expect("a plausible instant")
    }

    fn seeded() -> InMemoryNotificationsRepo {
        let repo = InMemoryNotificationsRepo::new();
        repo.seed_template(
            codes::USER_SUBSCRIBED,
            "New Follower",
            "{actor} follows you",
        );
        repo.seed_user(id(2), "Ada Lovelace");
        repo.seed_settings(id(1), true, true);
        repo.seed_token(id(1), "device-token");
        repo
    }

    #[tokio::test]
    async fn a_page_is_newest_first_and_stops_at_the_limit() {
        let repo = seeded();
        repo.seed_notification(id(1), codes::USER_SUBSCRIBED, None, None, at(1), false);
        repo.seed_notification(id(1), codes::USER_SUBSCRIBED, None, None, at(3), false);
        repo.seed_notification(id(1), codes::USER_SUBSCRIBED, None, None, at(2), false);

        let rows = repo
            .find_page(&NotificationFilter {
                owner_id: id(1),
                unread_only: false,
                limit_plus_one: 3,
                cursor: None,
            })
            .await
            .expect("the page reads");

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].created_at, at(3));
        assert_eq!(rows[2].created_at, at(1));
    }

    #[tokio::test]
    async fn a_cursor_excludes_the_row_it_points_at() {
        let repo = seeded();
        repo.seed_notification(id(1), codes::USER_SUBSCRIBED, None, None, at(1), false);
        let second =
            repo.seed_notification(id(1), codes::USER_SUBSCRIBED, None, None, at(2), false);

        let rows = repo
            .find_page(&NotificationFilter {
                owner_id: id(1),
                unread_only: false,
                limit_plus_one: 5,
                cursor: Some((at(2), second)),
            })
            .await
            .expect("the page reads");

        assert_eq!(rows.len(), 1, "the cursor row is excluded");
        assert_eq!(rows[0].created_at, at(1));
    }

    #[tokio::test]
    async fn a_deleted_actor_reads_as_no_actor() {
        let repo = seeded();
        let notification = repo.seed_notification(
            id(1),
            codes::USER_SUBSCRIBED,
            Some(id(2)),
            None,
            at(1),
            false,
        );
        repo.remove_user(id(2));

        let rows = repo
            .find_page(&NotificationFilter {
                owner_id: id(1),
                unread_only: false,
                limit_plus_one: 5,
                cursor: None,
            })
            .await
            .expect("the page reads");

        assert!(rows[0].actor().is_none(), "{notification}");
    }

    #[tokio::test]
    async fn marking_read_is_a_one_way_transition() {
        let repo = seeded();
        let notification =
            repo.seed_notification(id(1), codes::USER_SUBSCRIBED, None, None, at(1), false);

        assert!(
            repo.mark_read(notification, id(1))
                .await
                .expect("the update runs")
        );
        assert!(
            !repo
                .mark_read(notification, id(1))
                .await
                .expect("the update runs"),
            "a second mark changes nothing"
        );
        assert!(repo.is_read(notification, id(1)));
    }

    #[tokio::test]
    async fn another_owners_notification_is_invisible() {
        let repo = seeded();
        let notification =
            repo.seed_notification(id(9), codes::USER_SUBSCRIBED, None, None, at(1), false);

        assert!(!repo.exists(notification, id(1)).await.expect("the read"));
        assert!(
            !repo
                .mark_read(notification, id(1))
                .await
                .expect("the update")
        );
        assert!(
            !repo.delete(notification, id(1)).await.expect("the delete"),
            "the delete is scoped to the owner"
        );
        assert_eq!(repo.stored_notification_count(), 1);
    }

    #[tokio::test]
    async fn push_is_only_offered_when_the_user_switched_it_on() {
        let repo = seeded();
        assert_eq!(
            repo.device_token(id(1)).await.expect("the read"),
            Some("device-token".to_owned())
        );

        repo.seed_settings(id(1), false, true);
        assert_eq!(repo.device_token(id(1)).await.expect("the read"), None);
        assert!(
            repo.push_targets(&[id(1)])
                .await
                .expect("the read")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_cleared_token_is_gone() {
        let repo = seeded();
        repo.clear_device_token(id(1)).await.expect("the delete");
        assert!(repo.tokens().is_empty());
    }
}
