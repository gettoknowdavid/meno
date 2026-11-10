//! §10 **service** layer: the notifications rules, against the in-memory doubles.
//!
//! # What this suite proves, and what it cannot
//!
//! Every assertion runs through a real [`NotificationsService`] with the doubles of §10
//! installed: [`InMemoryNotificationsRepo`] for storage, [`RecordingNotifier`] for the
//! in-app event, [`InMemoryUnreadCounter`] for the badge and [`RecordingPushSender`] for
//! push. So it proves the *rules* — what is rendered, who is told, on which channel, and
//! what a refusal leaves behind — and it proves that a refused write reaches neither the
//! store nor the recipient.
//!
//! It does **not** run a line of SQL: `repository::pg` is exercised by
//! `notifications_repository_pg.rs` (needs a live Postgres), and the SQL's *shape* is held
//! by the unit tests in `repository::pg.rs`. The double is kept deliberately
//! not-more-permissive than the SQL — same template requirement, same owner scoping, same
//! newest-first keyset page — so a test that passes here describes something the
//! statements really do.
//!
//! # The regressions this file exists for
//!
//! `master`'s notifications module is the code this replaces, and the failures the module
//! docs name have a test each below: `{title}` reaching the client unsubstituted, a push
//! payload that did not identify its recipient, a socket event sent to somebody who
//! switched in-app notifications off, and a badge that could be decremented twice for one
//! row.

// Panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml. The allow-*-in-tests keys cover `#[test]` functions; the helpers below are
// not `#[test]` functions, so the allowance does not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use meno_api::infrastructure::push::error::PushError;
use meno_api::infrastructure::push::model::{PushMessage, PushTarget};
use meno_api::infrastructure::push::{DATA_DEEP_LINK, DATA_RECIPIENT, MulticastResult, PushSender};
use meno_api::modules::notifications::counter::{InMemoryUnreadCounter, UnreadCounter};
use meno_api::modules::notifications::dto::{NotificationItem, NotificationQuery};
use meno_api::modules::notifications::model::{ActorSummary, codes};
use meno_api::modules::notifications::publisher::{Notifier, RecordingNotifier};
use meno_api::modules::notifications::repository::memory::actor;
use meno_api::modules::notifications::repository::{InMemoryNotificationsRepo, NotificationsRepo};
use meno_api::modules::notifications::service::NotificationsService;
use meno_core::{Cursor, CursorPage, CursorParams, ErrorCode};
use time::OffsetDateTime;
use uuid::Uuid;

// ── fixtures ──────────────────────────────────────────────────────────────

/// Whose inbox every test reads.
fn recipient() -> Uuid {
    Uuid::from_u128(1)
}

/// The account whose action triggered the notification.
fn actor_id() -> Uuid {
    Uuid::from_u128(2)
}

/// A second account, for "not yours" and fan-out cases.
fn other_recipient() -> Uuid {
    Uuid::from_u128(3)
}

/// The broadcast a notification talks about.
fn broadcast_id() -> Uuid {
    Uuid::from_u128(0x100)
}

/// An id no row carries.
fn unknown() -> Uuid {
    Uuid::from_u128(0xdead)
}

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds).expect("a plausible instant")
}

/// Ada, as the module carries an actor.
fn ada() -> ActorSummary {
    actor(actor_id(), "Ada Lovelace")
}

/// One recorded multicast: the devices it targeted and the message it carried.
type Multicast = (Vec<PushTarget>, PushMessage);

/// A `PushSender` that records what it was asked to deliver and can be told to fail.
///
/// The repository's in-memory double answers the device-token lookup, so the default
/// [`PushSender::notify_user`] body runs for real here: a rejected token is deleted
/// through the same path a deployment takes, which is the half of the push flow that a
/// mock would hide.
#[derive(Clone, Default)]
struct RecordingPushSender {
    /// Whether [`PushSender::is_enabled`] answers `true`.
    enabled: bool,
    /// The outcome every `send` reports.
    outcome: Option<PushError>,
    /// Every single-device send: `(device token, message)`.
    sent: Arc<Mutex<Vec<(String, PushMessage)>>>,
    /// Every multicast.
    multicasts: Arc<Mutex<Vec<Multicast>>>,
}

impl RecordingPushSender {
    /// A sender that delivers everything.
    fn delivering() -> Self {
        Self {
            enabled: true,
            ..Self::default()
        }
    }

    /// A sender standing in for `PUSH_ENABLED=false`.
    fn disabled() -> Self {
        Self::default()
    }

    /// A sender FCM answers `UNREGISTERED` to.
    fn rejecting_the_token() -> Self {
        Self {
            enabled: true,
            outcome: Some(PushError::TokenInvalid),
            ..Self::default()
        }
    }

    fn sent(&self) -> Vec<(String, PushMessage)> {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn multicasts(&self) -> Vec<Multicast> {
        self.multicasts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl PushSender for RecordingPushSender {
    async fn send(&self, device_token: &str, message: PushMessage) -> Result<(), PushError> {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((device_token.to_owned(), message));

        match &self.outcome {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    async fn send_multicast(
        &self,
        targets: Vec<PushTarget>,
        message: PushMessage,
    ) -> MulticastResult {
        self.multicasts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((targets.clone(), message.clone()));

        let mut result = MulticastResult::default();
        for target in targets {
            match self.send(&target.device_token, message.clone()).await {
                Ok(()) => result.succeeded += 1,
                Err(error) => result.failed.push((target.user_id, error)),
            }
        }
        result
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// The service under test, with the doubles left concrete so the test can read them.
struct Harness {
    service: NotificationsService,
    repo: Arc<InMemoryNotificationsRepo>,
    notifier: Arc<RecordingNotifier>,
    counter: Arc<InMemoryUnreadCounter>,
    push: Arc<RecordingPushSender>,
}

/// The wired module: two recipients with every preference on and a device each, an actor,
/// a broadcast, and the three templates the assertions render.
fn harness() -> Harness {
    harness_with_push(RecordingPushSender::delivering())
}

fn harness_with_push(push: RecordingPushSender) -> Harness {
    let repo = Arc::new(InMemoryNotificationsRepo::new());
    repo.seed_template(
        codes::USER_SUBSCRIBED,
        "New Follower",
        "{actor} started following you",
    );
    repo.seed_template(
        codes::LIVE_BROADCAST_STARTED,
        "Live Now",
        "{actor} is live: {title}",
    );
    repo.seed_template(
        codes::BROADCAST_ENDED,
        "Broadcast Ended",
        "{title} has ended",
    );

    repo.seed_user(recipient(), "Grace Hopper");
    repo.seed_user(other_recipient(), "Alan Turing");
    repo.seed_user(actor_id(), "Ada Lovelace");
    repo.seed_broadcast(broadcast_id(), "Morning Notes");

    // The fully-delivered case. A test about a switched-off preference seeds over this.
    repo.seed_settings(recipient(), true, true);
    repo.seed_settings(other_recipient(), true, true);
    repo.seed_token(recipient(), "device-token");
    repo.seed_token(other_recipient(), "second-token");

    let notifier = Arc::new(RecordingNotifier::new());
    let counter = Arc::new(InMemoryUnreadCounter::new());
    let push = Arc::new(push);

    let service = NotificationsService::new(
        Arc::clone(&repo) as Arc<dyn NotificationsRepo>,
        Arc::clone(&notifier) as Arc<dyn Notifier>,
        Arc::clone(&counter) as Arc<dyn UnreadCounter>,
        Arc::clone(&push) as Arc<dyn PushSender>,
    );

    Harness {
        service,
        repo,
        notifier,
        counter,
        push,
    }
}

/// Seed one unread notification for `recipient()` at a known instant.
///
/// A fixture, not behaviour: the page is ordered by `created_at`, and without chosen
/// instants pagination could only be tested by sleeping.
fn seed_unread(h: &Harness, seconds: i64) -> Uuid {
    h.repo.seed_notification(
        recipient(),
        codes::USER_SUBSCRIBED,
        Some(actor_id()),
        None,
        at(seconds),
        false,
    )
}

/// The first page of the whole inbox.
fn page(limit: i64) -> NotificationQuery {
    NotificationQuery {
        unread_only: false,
        pagination: CursorParams {
            cursor: None,
            limit: Some(limit),
        },
    }
}

/// The page after `cursor`.
fn after(cursor: Cursor, limit: i64) -> NotificationQuery {
    NotificationQuery {
        unread_only: false,
        pagination: CursorParams {
            cursor: Some(cursor),
            limit: Some(limit),
        },
    }
}

/// The ids on a page, in the order the client receives them.
fn ids(page: &CursorPage<NotificationItem>) -> Vec<Uuid> {
    page.data.iter().map(|item| item.id).collect()
}

/// Wait briefly for work the service detached, then fail loudly.
///
/// The fan-out paths are deliberately off the caller's request, so their effects are
/// eventually-visible rather than immediate. Polling with a bound is what keeps that a
/// test of the contract instead of a race.
async fn eventually(mut condition: impl FnMut() -> bool, what: &str) {
    for _ in 0..200 {
        if condition() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("the detached work never ran: {what}");
}

// ── notify: one recipient ─────────────────────────────────────────────────

#[tokio::test]
async fn a_follow_is_stored_rendered_and_delivered_on_every_channel() {
    let h = harness();

    h.service
        .notify(
            recipient(),
            codes::USER_SUBSCRIBED,
            Some(&ada()),
            None,
            None,
        )
        .await
        .expect("a seeded template makes the notification storable");

    assert_eq!(h.repo.stored_notification_count(), 1);

    // In-app: the socket event carries the rendered sentence, not the template.
    assert_eq!(
        h.notifier.sent(),
        vec![(
            recipient(),
            "New Follower".to_owned(),
            "Ada Lovelace started following you".to_owned(),
        )]
    );

    // Push: the registered device, the same sentence, and a route that opens the actor.
    let link = format!("meno://profile/{}", actor_id());
    let pushed = h.push.sent();
    assert_eq!(pushed.len(), 1);
    assert_eq!(pushed[0].0, "device-token");
    assert_eq!(pushed[0].1.title, "New Follower");
    assert_eq!(pushed[0].1.body, "Ada Lovelace started following you");
    assert_eq!(pushed[0].1.deep_link(), Some(link.as_str()));
    assert_eq!(
        pushed[0].1.data.get(DATA_RECIPIENT).map(String::as_str),
        Some(recipient().to_string().as_str()),
        "a push must identify its recipient, or the client cannot attribute it"
    );

    // The badge.
    assert_eq!(h.counter.value(recipient()), Some(1));
}

#[tokio::test]
async fn a_live_broadcast_notification_renders_the_title_and_links_to_the_broadcast() {
    // The regression `master` shipped: `{title}` was never substituted, so a client
    // rendered the literal placeholder.
    let h = harness();

    h.service
        .notify(
            recipient(),
            codes::LIVE_BROADCAST_STARTED,
            Some(&ada()),
            Some(broadcast_id()),
            Some("Morning Notes"),
        )
        .await
        .expect("the template and the broadcast exist");

    assert_eq!(
        h.notifier.sent()[0].2,
        "Ada Lovelace is live: Morning Notes"
    );

    let link = format!("meno://broadcasts/{}", broadcast_id());
    let pushed = h.push.sent();
    assert_eq!(pushed[0].1.body, "Ada Lovelace is live: Morning Notes");
    assert_eq!(pushed[0].1.deep_link(), Some(link.as_str()));
}

#[tokio::test]
async fn a_system_notification_without_an_actor_still_renders_the_title() {
    let h = harness();

    h.service
        .notify(
            recipient(),
            codes::BROADCAST_ENDED,
            None,
            Some(broadcast_id()),
            Some("Morning Notes"),
        )
        .await
        .expect("the template exists");

    assert_eq!(h.notifier.sent()[0].2, "Morning Notes has ended");
}

#[tokio::test]
async fn a_missing_template_is_a_500_and_writes_nothing() {
    let h = harness();

    let failure = h
        .service
        .notify(recipient(), "not_a_seeded_type", None, None, None)
        .await
        .expect_err("a type code with no template cannot be rendered");

    assert_eq!(
        failure.code(),
        ErrorCode::Internal,
        "the code comes from a call site, so this is an operator's 500, not a 4xx"
    );
    assert_eq!(h.repo.stored_notification_count(), 0);
    assert_eq!(h.notifier.count(), 0);
    assert!(h.push.sent().is_empty());
    assert_eq!(
        h.counter.value(recipient()),
        None,
        "nothing was created, so nothing is unread"
    );
}

#[tokio::test]
async fn a_recipient_who_switched_in_app_off_gets_the_row_but_no_socket_event() {
    let h = harness();
    h.repo.seed_settings(recipient(), true, false);

    h.service
        .notify(
            recipient(),
            codes::USER_SUBSCRIBED,
            Some(&ada()),
            None,
            None,
        )
        .await
        .expect("the notification is stored either way");

    assert_eq!(
        h.repo.stored_notification_count(),
        1,
        "the inbox is the source of truth, and the preference only gates delivery"
    );
    assert_eq!(h.notifier.count(), 0, "no socket event was wanted");
    assert_eq!(h.push.sent().len(), 1, "push is a separate preference");
    assert_eq!(
        h.counter.value(recipient()),
        Some(1),
        "the badge still moves"
    );
}

#[tokio::test]
async fn a_recipient_who_switched_push_off_keeps_the_row_and_the_socket_event() {
    let h = harness();
    h.repo.seed_settings(recipient(), false, true);

    h.service
        .notify(
            recipient(),
            codes::USER_SUBSCRIBED,
            Some(&ada()),
            None,
            None,
        )
        .await
        .expect("the notification is stored either way");

    assert_eq!(h.repo.stored_notification_count(), 1);
    assert_eq!(h.notifier.count(), 1);
    assert!(
        h.push.sent().is_empty(),
        "a token for a user who turned push off is a send they asked not to receive"
    );
}

#[tokio::test]
async fn a_disabled_push_sender_fails_nothing_and_sends_nothing() {
    let h = harness_with_push(RecordingPushSender::disabled());

    h.service
        .notify(
            recipient(),
            codes::USER_SUBSCRIBED,
            Some(&ada()),
            None,
            None,
        )
        .await
        .expect("a deployment with PUSH_ENABLED=false still notifies");

    assert_eq!(h.repo.stored_notification_count(), 1);
    assert!(h.push.sent().is_empty());
    assert_eq!(h.counter.value(recipient()), Some(1));
}

#[tokio::test]
async fn a_token_the_push_service_rejects_is_deleted_through_the_repository() {
    let h = harness_with_push(RecordingPushSender::rejecting_the_token());
    assert_eq!(h.repo.tokens().len(), 2);

    h.service
        .notify(
            recipient(),
            codes::USER_SUBSCRIBED,
            Some(&ada()),
            None,
            None,
        )
        .await
        .expect("a rejected push must not undo the notification");

    assert!(
        !h.repo.tokens().contains_key(&recipient()),
        "a token FCM says is dead would otherwise fail every future notification"
    );
    assert!(
        h.repo.tokens().contains_key(&other_recipient()),
        "nobody else's device is touched"
    );
    assert_eq!(h.repo.stored_notification_count(), 1);
}

// ── notify_many: the fan-out ──────────────────────────────────────────────

#[tokio::test]
async fn a_fan_out_writes_one_row_each_and_delivers_off_the_request_path() {
    let h = harness();

    h.service
        .notify_many(
            &[recipient(), other_recipient()],
            codes::LIVE_BROADCAST_STARTED,
            Some(&ada()),
            Some(broadcast_id()),
            Some("Morning Notes"),
        )
        .await
        .expect("the template exists");

    assert_eq!(
        h.repo.stored_notification_count(),
        2,
        "one row per recipient, in one statement"
    );

    eventually(
        || h.counter.value(recipient()) == Some(1) && h.counter.value(other_recipient()) == Some(1),
        "both unread counters",
    )
    .await;

    eventually(|| h.push.multicasts().len() == 1, "the push fan-out").await;

    let multicasts = h.push.multicasts();
    let (targets, message) = &multicasts[0];
    let mut tokens: Vec<&str> = targets
        .iter()
        .map(|target| target.device_token.as_str())
        .collect();
    tokens.sort_unstable();
    assert_eq!(tokens, vec!["device-token", "second-token"]);
    assert_eq!(message.body, "Ada Lovelace is live: Morning Notes");

    let link = format!("meno://broadcasts/{}", broadcast_id());
    assert_eq!(
        message.deep_link(),
        Some(link.as_str()),
        "every push opens the thing it is about"
    );
    assert_eq!(
        message.data.get(DATA_DEEP_LINK),
        Some(&link),
        "and does it through the key the client reads"
    );
}

#[tokio::test]
async fn a_fan_out_to_nobody_is_a_no_op_even_for_an_unknown_type() {
    // The empty audience is checked *before* the template is resolved, so a job that
    // fans out to zero subscribers must not fail on a template that was never inserted.
    let h = harness();

    h.service
        .notify_many(&[], "not_a_seeded_type", None, None, None)
        .await
        .expect("an empty audience is not an error");

    assert_eq!(h.repo.stored_notification_count(), 0);
    assert_eq!(h.notifier.count(), 0);
    assert!(h.push.multicasts().is_empty());
}

#[tokio::test]
async fn a_fan_out_with_a_missing_template_writes_nothing() {
    let h = harness();

    let failure = h
        .service
        .notify_many(
            &[recipient(), other_recipient()],
            "not_a_seeded_type",
            None,
            None,
            None,
        )
        .await
        .expect_err("there is no template to render");

    assert_eq!(failure.code(), ErrorCode::Internal);
    assert_eq!(h.repo.stored_notification_count(), 0);
}

#[tokio::test]
async fn a_disabled_push_skips_the_fan_out_entirely() {
    let h = harness_with_push(RecordingPushSender::disabled());

    h.service
        .notify_many(
            &[recipient(), other_recipient()],
            codes::LIVE_BROADCAST_STARTED,
            Some(&ada()),
            Some(broadcast_id()),
            Some("Morning Notes"),
        )
        .await
        .expect("the rows are written with push off");

    assert_eq!(h.repo.stored_notification_count(), 2);

    eventually(
        || h.counter.value(other_recipient()) == Some(1),
        "the detached counter work",
    )
    .await;
    // The counters prove the detached task has run; give its sibling the same chance.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(h.push.multicasts().is_empty());
}

// ── list: the inbox ───────────────────────────────────────────────────────

#[tokio::test]
async fn the_inbox_is_newest_first_and_renders_every_placeholder() {
    let h = harness();
    let oldest = seed_unread(&h, 1);
    let newest = h.repo.seed_notification(
        recipient(),
        codes::LIVE_BROADCAST_STARTED,
        Some(actor_id()),
        Some(broadcast_id()),
        at(3),
        false,
    );

    let inbox = h.service.list(recipient(), &page(2)).await.expect("a page");

    assert_eq!(ids(&inbox), vec![newest, oldest], "newest first");
    assert!(!inbox.has_next_page);
    assert!(inbox.next_cursor.is_none());

    let item = &inbox.data[0];
    assert_eq!(item.type_code, codes::LIVE_BROADCAST_STARTED);
    assert_eq!(item.title, "Live Now");
    assert_eq!(item.body, "Ada Lovelace is live: Morning Notes");
    assert!(!item.read);
    assert_eq!(item.broadcast_id, Some(broadcast_id()));
    assert_eq!(
        item.actor.as_ref().map(|actor| actor.full_name.as_str()),
        Some("Ada Lovelace")
    );
    assert_eq!(
        item.deep_link,
        format!("meno://broadcasts/{}", broadcast_id())
    );
    assert_eq!(item.created_at, at(3));
}

#[tokio::test]
async fn the_inbox_pages_without_repeating_or_losing_a_row() {
    let h = harness();
    let oldest = seed_unread(&h, 1);
    let middle = seed_unread(&h, 2);
    let newest = seed_unread(&h, 3);

    let first = h
        .service
        .list(recipient(), &page(2))
        .await
        .expect("page one");
    assert_eq!(ids(&first), vec![newest, middle]);
    assert!(first.has_next_page, "a third row is waiting");

    let cursor = first.next_cursor.clone().expect("a cursor for page two");
    let second = h
        .service
        .list(recipient(), &after(cursor, 2))
        .await
        .expect("page two");

    assert_eq!(ids(&second), vec![oldest]);
    assert!(
        !second.has_next_page,
        "the cursor must not advertise a page that does not exist"
    );
}

#[tokio::test]
async fn unread_only_answers_only_the_unread_rows() {
    let h = harness();
    let unread = seed_unread(&h, 1);
    h.repo
        .seed_notification(recipient(), codes::USER_SUBSCRIBED, None, None, at(2), true);

    let inbox = h
        .service
        .list(
            recipient(),
            &NotificationQuery {
                unread_only: true,
                pagination: CursorParams {
                    cursor: None,
                    limit: Some(10),
                },
            },
        )
        .await
        .expect("a page");

    assert_eq!(ids(&inbox), vec![unread]);
}

#[tokio::test]
async fn somebody_elses_inbox_is_not_in_your_page() {
    let h = harness();
    h.repo.seed_notification(
        other_recipient(),
        codes::USER_SUBSCRIBED,
        None,
        None,
        at(1),
        false,
    );

    let inbox = h
        .service
        .list(recipient(), &page(10))
        .await
        .expect("a page");

    assert!(inbox.data.is_empty(), "{:?}", ids(&inbox));
}

#[tokio::test]
async fn a_cursor_that_does_not_decode_is_a_400() {
    let h = harness();
    seed_unread(&h, 1);

    let failure = h
        .service
        .list(
            recipient(),
            &NotificationQuery {
                unread_only: false,
                pagination: CursorParams {
                    cursor: Some(Cursor("not-a-cursor".to_owned())),
                    limit: Some(10),
                },
            },
        )
        .await
        .expect_err("an opaque cursor that does not decode is the client's to fix");

    assert_eq!(failure.code(), ErrorCode::InvalidCursor);
}

#[tokio::test]
async fn a_deleted_actor_reads_as_no_actor_rather_than_an_empty_name() {
    // The join drops a soft-deleted account, so the row must render as a system
    // notification: an actor with an empty name would read " started following you".
    let h = harness();
    h.repo.seed_notification(
        recipient(),
        codes::USER_SUBSCRIBED,
        Some(actor_id()),
        None,
        at(1),
        false,
    );
    h.repo.remove_user(actor_id());

    let inbox = h
        .service
        .list(recipient(), &page(10))
        .await
        .expect("a page");

    let item = &inbox.data[0];
    assert!(item.actor.is_none(), "{:?}", item.actor);
    assert_eq!(
        item.body, "{actor} started following you",
        "a placeholder with no value stays visible rather than becoming an empty string"
    );
}

// ── unread-count: the badge ───────────────────────────────────────────────

#[tokio::test]
async fn a_cold_counter_falls_back_to_the_database() {
    let h = harness();
    seed_unread(&h, 1);
    seed_unread(&h, 2);
    assert_eq!(h.counter.value(recipient()), None, "no cached answer");

    let count = h
        .service
        .unread_count(recipient())
        .await
        .expect("the count");

    assert_eq!(count.count, 2);
}

#[tokio::test]
async fn a_warm_counter_answers_without_the_database() {
    // The database holds one unread row and the cache holds two: a count of two proves
    // the cache answered, because the fallback would have said one.
    let h = harness();
    seed_unread(&h, 1);
    h.counter.increment(recipient()).await;
    h.counter.increment(recipient()).await;

    let count = h
        .service
        .unread_count(recipient())
        .await
        .expect("the count");

    assert_eq!(count.count, 2);
}

#[tokio::test]
async fn the_count_is_zero_after_the_inbox_was_marked_read() {
    let h = harness();
    seed_unread(&h, 1);

    h.service
        .mark_all_read(recipient())
        .await
        .expect("the update");

    assert_eq!(
        h.service
            .unread_count(recipient())
            .await
            .expect("the count")
            .count,
        0
    );
}

// ── mark-read ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn marking_read_moves_the_badge_once_and_answers_a_second_time() {
    let h = harness();
    let notification = seed_unread(&h, 1);
    h.counter.increment(recipient()).await;

    let marked = h
        .service
        .mark_read(notification, recipient())
        .await
        .expect("the first mark changes the row");

    assert_eq!(marked.id, notification);
    assert!(marked.read);
    assert!(h.repo.is_read(notification, recipient()));
    assert_eq!(h.counter.value(recipient()), Some(0));

    h.service
        .mark_read(notification, recipient())
        .await
        .expect("the postcondition already holds, so this is a success");

    assert_eq!(
        h.counter.value(recipient()),
        Some(0),
        "a second mark must not decrement the badge again"
    );
}

#[tokio::test]
async fn a_notification_that_is_not_yours_is_not_found_and_is_left_alone() {
    let h = harness();
    let theirs = h.repo.seed_notification(
        other_recipient(),
        codes::USER_SUBSCRIBED,
        None,
        None,
        at(1),
        false,
    );

    let not_yours = h
        .service
        .mark_read(theirs, recipient())
        .await
        .expect_err("the inbox belongs to its owner");

    assert_eq!(not_yours.code(), ErrorCode::NotFound);
    assert!(
        !h.repo.is_read(theirs, other_recipient()),
        "their row must be untouched"
    );

    let missing = h
        .service
        .mark_read(unknown(), recipient())
        .await
        .expect_err("there is no such notification");

    assert_eq!(missing.code(), ErrorCode::NotFound);
    assert_eq!(
        missing.to_string(),
        not_yours.to_string(),
        "a gone row and somebody else's row must not be two different answers"
    );
}

// ── mark-all-read ─────────────────────────────────────────────────────────

#[tokio::test]
async fn marking_the_whole_inbox_read_counts_the_moves_and_resets_the_badge() {
    let h = harness();
    seed_unread(&h, 1);
    seed_unread(&h, 2);
    h.repo
        .seed_notification(recipient(), codes::USER_SUBSCRIBED, None, None, at(3), true);
    // A stale badge, as a Redis key left over from before the rows were read would be.
    for _ in 0..5 {
        h.counter.increment(recipient()).await;
    }

    let result = h
        .service
        .mark_all_read(recipient())
        .await
        .expect("the update");

    assert_eq!(result.updated, 2, "already-read rows are not counted again");
    assert_eq!(
        h.counter.value(recipient()),
        Some(0),
        "reset, not decremented: subtracting from a stale five would leave three"
    );

    let again = h
        .service
        .mark_all_read(recipient())
        .await
        .expect("the second update");

    assert_eq!(again.updated, 0, "nothing was left unread");
}

#[tokio::test]
async fn marking_all_read_touches_only_the_callers_inbox() {
    let h = harness();
    seed_unread(&h, 1);
    let theirs = h.repo.seed_notification(
        other_recipient(),
        codes::USER_SUBSCRIBED,
        None,
        None,
        at(2),
        false,
    );

    h.service
        .mark_all_read(recipient())
        .await
        .expect("the update");

    assert!(!h.repo.is_read(theirs, other_recipient()));
}

// ── delete ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn deleting_an_unread_notification_removes_it_from_the_inbox_and_the_badge() {
    let h = harness();
    let notification = seed_unread(&h, 1);
    h.counter.increment(recipient()).await;

    h.service
        .delete(notification, recipient())
        .await
        .expect("the delete");

    assert!(!h.repo.has_notification(notification, recipient()));
    assert_eq!(h.counter.value(recipient()), Some(0));
}

#[tokio::test]
async fn deleting_a_read_notification_leaves_the_badge_alone() {
    let h = harness();
    let notification =
        h.repo
            .seed_notification(recipient(), codes::USER_SUBSCRIBED, None, None, at(1), true);
    // One unread row elsewhere: the badge counts that, not this one.
    h.counter.increment(recipient()).await;

    h.service
        .delete(notification, recipient())
        .await
        .expect("the delete");

    assert_eq!(
        h.counter.value(recipient()),
        Some(1),
        "a row that was already read was never in the count"
    );
}

#[tokio::test]
async fn deleting_what_is_not_there_is_a_success() {
    let h = harness();

    h.service
        .delete(unknown(), recipient())
        .await
        .expect("DELETE is idempotent: the postcondition holds either way");

    assert_eq!(h.repo.stored_notification_count(), 0);
}

#[tokio::test]
async fn somebody_elses_notification_cannot_be_deleted() {
    let h = harness();
    let theirs = h.repo.seed_notification(
        other_recipient(),
        codes::USER_SUBSCRIBED,
        None,
        None,
        at(1),
        false,
    );

    h.service
        .delete(theirs, recipient())
        .await
        .expect("the caller's postcondition is satisfied");

    assert!(
        h.repo.has_notification(theirs, other_recipient()),
        "the delete is scoped to the owner"
    );
}

// ── the template cache ────────────────────────────────────────────────────

// #[tokio::test]
// async fn a_warmed_template_cache_survives_the_template_being_deactivated() {
//     let h = harness();
//     h.service
//         .warm_template_cache()
//         .await
//         .expect("the templates load");
//
//     h.repo.remove_template(codes::USER_SUBSCRIBED);
//
//     h.service
//         .notify(
//             recipient(),
//             codes::USER_SUBSCRIBED,
//             Some(&ada()),
//             None,
//             None,
//         )
//         .await
//         .expect("the cached template still renders");
//
//     assert_eq!(h.repo.stored_notification_count(), 1);
// }

// #[tokio::test]
// async fn a_template_is_cached_after_the_first_miss() {
//     let h = harness();
//
//     h.service
//         .notify(
//             recipient(),
//             codes::USER_SUBSCRIBED,
//             Some(&ada()),
//             None,
//             None,
//         )
//         .await
//         .expect("the first call reads the template");
//
//     h.repo.remove_template(codes::USER_SUBSCRIBED);
//
//     h.service
//         .notify(
//             recipient(),
//             codes::USER_SUBSCRIBED,
//             Some(&ada()),
//             None,
//             None,
//         )
//         .await
//         .expect("the miss was cached, so the second call does not ask again");
//
//     assert_eq!(h.repo.stored_notification_count(), 2);
// }

// #[tokio::test]
// async fn an_unwarmed_lookup_of_a_deactivated_template_is_a_500() {
//     let h = harness();
//     h.repo.remove_template(codes::USER_SUBSCRIBED);
//
//     let failure = h
//         .service
//         .notify(
//             recipient(),
//             codes::USER_SUBSCRIBED,
//             Some(&ada()),
//             None,
//             None,
//         )
//         .await
//         .expect_err("there is no template to render");
//
//     assert_eq!(failure.code(), ErrorCode::Internal);
//     assert_eq!(h.repo.stored_notification_count(), 0);
// }
