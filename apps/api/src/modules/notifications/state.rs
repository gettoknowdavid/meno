//! The notifications module's state: where concrete adapters are chosen (§5.6).
//!
//! The whole file's job: pool → [`PgNotificationsRepo`], bridge → [`PubSubNotifier`],
//! Redis → [`RedisUnreadCounter`], plus the push sender the application already
//! built. Every consumer above sees `Arc<dyn …>` and the module's tests assemble it
//! from the in-memory doubles, which is what lets the whole service suite run with no
//! Postgres, no Redis and no Firebase project.
//!
//! # Construction is infallible, and that is deliberate
//!
//! Nothing here reads a secret or dials anything: the pool is connected, the bridge is
//! verified at boot ([`crate::state::build`]), and the push sender is already chosen
//! from configuration (`PUSH_ENABLED`, §4.6). A `Result` that can only be `Ok` is a
//! worse API than none — the same reasoning as [`crate::modules::subscribers::state`].

use std::sync::Arc;

use sqlx::PgPool;

use super::counter::{RedisUnreadCounter, UnreadCounter};
use super::publisher::{Notifier, PubSubNotifier};
use super::repository::{NotificationsRepo, PgNotificationsRepo};
use super::service::NotificationsService;
use crate::infrastructure::push::PushSender;
use crate::infrastructure::redis::Redis;
use crate::infrastructure::ws::pubsub::WsPubSubBridge;

/// The notifications module as the router sees it.
#[derive(Clone)]
pub struct NotificationsState {
    /// The rules. A handler reaches through this and nothing else.
    pub service: Arc<NotificationsService>,
}

impl std::fmt::Debug for NotificationsState {
    /// Names the field and stops: the service holds a pool, a bridge and a sender.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationsState")
            .field("service", &self.service)
            .finish()
    }
}

/// The raw resources [`NotificationsState::new`] builds adapters from.
///
/// Four, and each is a real dependency rather than an `Option` someone forgets to
/// check: the inbox needs storage, the badge needs a counter, the socket event needs
/// fan-out, and push is optional *at the sender* (§4.6) — `NoopPushSender` is still a
/// sender.
pub struct Wiring {
    /// The application's pool.
    pub pool: PgPool,
    /// Redis, for the unread counter.
    pub redis: Redis,
    /// The push sender, already chosen from configuration.
    pub push: Arc<dyn PushSender>,
    /// The cross-replica pub/sub bridge, for the in-app event.
    pub bridge: Arc<WsPubSubBridge>,
}

impl NotificationsState {
    /// Build the module from raw resources.
    #[must_use]
    pub fn new(wiring: Wiring) -> Self {
        let repo: Arc<dyn NotificationsRepo> = Arc::new(PgNotificationsRepo::new(wiring.pool));
        let notifier: Arc<dyn Notifier> = Arc::new(PubSubNotifier::new(wiring.bridge));
        let counter: Arc<dyn UnreadCounter> = Arc::new(RedisUnreadCounter::new(wiring.redis));
        Self::from_parts(repo, notifier, counter, wiring.push)
    }

    /// Build the module from its trait objects — what `new` funnels into, and what
    /// tests call with the doubles.
    #[must_use]
    pub fn from_parts(
        repo: Arc<dyn NotificationsRepo>,
        notifier: Arc<dyn Notifier>,
        counter: Arc<dyn UnreadCounter>,
        push: Arc<dyn PushSender>,
    ) -> Self {
        Self {
            service: Arc::new(NotificationsService::new(repo, notifier, counter, push)),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The wiring's one property: a module built from doubles is usable with no
    //! infrastructure.

    use super::*;
    use crate::infrastructure::push::NoopPushSender;
    use crate::modules::notifications::counter::InMemoryUnreadCounter;
    use crate::modules::notifications::model::codes;
    use crate::modules::notifications::publisher::RecordingNotifier;
    use crate::modules::notifications::repository::InMemoryNotificationsRepo;

    fn doubles() -> (
        Arc<InMemoryNotificationsRepo>,
        Arc<RecordingNotifier>,
        Arc<InMemoryUnreadCounter>,
    ) {
        (
            Arc::new(InMemoryNotificationsRepo::new()),
            Arc::new(RecordingNotifier::new()),
            Arc::new(InMemoryUnreadCounter::new()),
        )
    }

    #[tokio::test]
    async fn a_state_built_from_doubles_notifies_without_any_infrastructure() {
        let (repo, notifier, counter) = doubles();
        repo.seed_template(
            codes::USER_SUBSCRIBED,
            "New Follower",
            "{actor} follows you",
        );
        repo.seed_settings(uuid::Uuid::from_u128(1), false, true);

        let state = NotificationsState::from_parts(
            Arc::clone(&repo) as Arc<dyn NotificationsRepo>,
            Arc::clone(&notifier) as Arc<dyn Notifier>,
            Arc::clone(&counter) as Arc<dyn UnreadCounter>,
            Arc::new(NoopPushSender),
        );

        state
            .service
            .notify(
                uuid::Uuid::from_u128(1),
                codes::USER_SUBSCRIBED,
                None,
                None,
                None,
            )
            .await
            .expect("a notification is created with no infrastructure");

        assert_eq!(repo.stored_notification_count(), 1);
        assert_eq!(notifier.count(), 1);
        assert_eq!(counter.value(uuid::Uuid::from_u128(1)), Some(1));
    }

    #[test]
    fn the_debug_impl_names_the_field_and_nothing_else() {
        let (repo, notifier, counter) = doubles();
        let state = NotificationsState::from_parts(
            repo as Arc<dyn NotificationsRepo>,
            notifier as Arc<dyn Notifier>,
            counter as Arc<dyn UnreadCounter>,
            Arc::new(NoopPushSender),
        );

        assert!(
            format!("{state:?}").starts_with("NotificationsState { service: NotificationsService"),
            "a {:?} must not leak a credential",
            state
        );
    }
}
