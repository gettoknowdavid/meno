//! §10 **service** layer: the chat rules, against the in-memory double.
//!
//! # What this suite proves, and what it cannot
//!
//! Every assertion here runs through a real [`ChatService`] with the doubles of §10
//! installed: [`InMemoryChatRepo`] for storage and [`RecordingPublisher`] for fan-out. So
//! it proves the *rules* — who may post, when an edit is still allowed, which refusals
//! are announced and which are silent — and it proves that a refused write reaches
//! neither the store nor the room.
//!
//! It does **not** run a line of SQL: `repository::pg` is exercised by
//! `chat_repository_pg.rs` (needs a live Postgres), and the SQL's *shape* is held by the
//! unit tests in `repository::pg.rs`. The doubles are kept deliberately not-more-
//! permissive than the SQL — same guard, same soft delete, same `updated_at` stamp — so a
//! test that passes here describes something the statements really do.
//!
//! # The regressions this file exists for
//!
//! `master`'s chat module is the code this replaces, and four of its failures have a test
//! named after them below: a message landing in a room that had already ended, an edit
//! applied *in another broadcast's name*, a delete reporting the wrong refusal, and an
//! edit inside a window that had closed.

// Panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use meno_api::infrastructure::ws::dto::WsPayload;
use meno_api::infrastructure::ws::model::WsEvent;
use meno_api::modules::chat::error;
use meno_api::modules::chat::model::{ChatMessage, MESSAGE_MAX_CHARS, SenderSummary};
use meno_api::modules::chat::publisher::ChatPublisher;
use meno_api::modules::chat::publisher::recording::RecordingPublisher;
use meno_api::modules::chat::repository::{ChatRepo, InMemoryChatRepo};
use meno_api::modules::chat::service::{ChatMember, ChatService};
use meno_core::{Cursor, CursorParams, ErrorCode};
use time::OffsetDateTime;
use uuid::Uuid;

// ── fixtures ──────────────────────────────────────────────────────────────

/// The broadcast every test talks in.
fn room() -> Uuid {
    Uuid::from_u128(0x100)
}

/// A second room, used to prove a message is not editable from outside the one it was
/// written in.
fn other_room() -> Uuid {
    Uuid::from_u128(0x200)
}

/// The author most tests act as.
fn ada() -> Uuid {
    Uuid::from_u128(0x1)
}

/// A second participant, for the "it is not yours" cases.
fn grace() -> Uuid {
    Uuid::from_u128(0x2)
}

/// A registered account who is in no room.
fn stranger() -> Uuid {
    Uuid::from_u128(0x3)
}

/// The service under test, with the doubles left concrete so they can be read.
struct Harness {
    service: ChatService,
    repo: Arc<InMemoryChatRepo>,
    publisher: Arc<RecordingPublisher>,
}

impl Harness {
    /// The caller's view of `room()`, as `user`.
    fn member(&self, user_id: Uuid) -> ChatMember {
        ChatMember {
            broadcast_id: room(),
            user_id,
        }
    }

    /// Everything the room was told, oldest first.
    fn announced(&self) -> Vec<WsEvent> {
        self.publisher.events()
    }

    /// Assert that nothing at all was announced — the half of the contract a client
    /// cannot see and `master` never checked.
    fn assert_silent(&self) {
        assert!(
            self.publisher.is_empty(),
            "nothing should have been announced, got {:?}",
            self.announced()
        );
    }
}

/// A live broadcast with Ada and Grace in it, and a stranger who is registered but not
/// in the room.
fn harness() -> Harness {
    let repo = Arc::new(InMemoryChatRepo::new());
    let publisher = Arc::new(RecordingPublisher::new());

    for (id, name) in [
        (ada(), "Ada Lovelace"),
        (grace(), "Grace Hopper"),
        (stranger(), "Alan Turing"),
    ] {
        repo.add_user(SenderSummary {
            id,
            full_name: name.to_owned(),
            bio: None,
            avatar_id: None,
            avatar_url: None,
        });
    }

    repo.start_broadcast(room());
    repo.add_participant(room(), ada());
    repo.add_participant(room(), grace());

    let service = ChatService::new(
        Arc::clone(&repo) as Arc<dyn ChatRepo>,
        Arc::clone(&publisher) as Arc<dyn ChatPublisher>,
    );

    Harness {
        service,
        repo,
        publisher,
    }
}

/// An instant `seconds` before now, for backdating a seeded message.
fn ago(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::now_utc() - time::Duration::seconds(seconds)
}

/// Write a message straight into the double, with a timestamp of the caller's choosing.
///
/// A fixture, not behaviour: the edit window is a function of the row's `created_at`, and
/// without backdating it could only be tested by waiting fifteen minutes.
fn seed(h: &Harness, id: u128, sender: Uuid, created_at: OffsetDateTime) -> Uuid {
    let message_id = Uuid::from_u128(id);
    h.repo.seed_message(ChatMessage {
        id: message_id,
        broadcast_id: room(),
        sender: sender_summary(sender),
        content: "seeded".to_owned(),
        created_at,
        updated_at: None,
    });
    message_id
}

/// The sender summary the double has for `user`.
fn sender_summary(user: Uuid) -> SenderSummary {
    let name = match user {
        id if id == grace() => "Grace Hopper",
        id if id == stranger() => "Alan Turing",
        _ => "Ada Lovelace",
    };
    SenderSummary {
        id: user,
        full_name: name.to_owned(),
        bio: None,
        avatar_id: None,
        avatar_url: None,
    }
}

/// The one payload the publisher holds, or a failure naming what it holds instead.
fn only_announcement(h: &Harness) -> WsPayload {
    let published = h.publisher.published();
    assert_eq!(published.len(), 1, "expected one announcement");
    published[0].1.clone()
}

/// Pagination at a given size, first page.
fn page_of(limit: i64) -> CursorParams {
    CursorParams {
        cursor: None,
        limit: Some(limit),
    }
}

// ── sending ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_message_from_a_participant_of_a_live_broadcast_is_stored_and_announced() {
    let h = harness();

    let sent = h
        .service
        .send_message(h.member(ada()), "hello everyone")
        .await
        .expect("a participant of a live broadcast may post");

    assert_eq!(sent.content, "hello everyone");
    assert_eq!(sent.broadcast_id, room());
    assert_eq!(sent.sender.id, ada());
    assert_eq!(sent.sender.full_name, "Ada Lovelace");
    assert!(!sent.is_edited, "a fresh message has never been edited");

    // Stored: it is in the history, and the store holds exactly one row.
    assert_eq!(h.repo.stored_message_count(), 1);

    // Announced: one `newMessage` to the room it was posted in, carrying that message.
    assert_eq!(h.announced(), vec![WsEvent::NewMessage]);
    let payload = only_announcement(&h);
    assert_eq!(payload.event, WsEvent::NewMessage);
    assert_eq!(payload.data["id"], sent.id.to_string());
    assert_eq!(payload.data["content"], "hello everyone");
    assert_eq!(h.publisher.published()[0].0, room(), "the room is named");
}

#[tokio::test]
async fn a_refused_send_never_reaches_the_store_or_the_room() {
    // Regression: `master` read "is the broadcast live", read "is the sender in the
    // room", and *then* inserted — so a broadcast that ended between the reads still
    // received the message. Here the guard is inside the insert, so the write does not
    // happen at all, and the service explains the refusal with an extra read.
    let h = harness();
    h.repo.end_broadcast(room());

    let failure = h
        .service
        .send_message(h.member(ada()), "into the void")
        .await
        .expect_err("a broadcast that is not live refuses messages");

    assert_eq!(failure.code(), ErrorCode::Conflict);
    assert_eq!(failure.to_string(), error::broadcast_not_live().to_string());
    assert_eq!(
        h.repo.stored_message_count(),
        0,
        "the message must not be in the table"
    );
    h.assert_silent();
}

#[tokio::test]
async fn a_send_from_somebody_outside_the_room_is_forbidden_and_silent() {
    let h = harness();

    let failure = h
        .service
        .send_message(h.member(stranger()), "let me in")
        .await
        .expect_err("only participants may post");

    assert_eq!(failure.code(), ErrorCode::NotParticipant);
    assert_eq!(h.repo.stored_message_count(), 0);
    h.assert_silent();
}

#[tokio::test]
async fn a_send_of_the_widest_allowed_message_is_accepted() {
    // The service does not validate length — the boundary does, against
    // `MESSAGE_MAX_CHARS` — but the double stores what it is given, so this pins the
    // constant the frame's `Validate` rule and the column both mirror.
    let h = harness();
    let content = "a".repeat(MESSAGE_MAX_CHARS);

    let sent = h
        .service
        .send_message(h.member(ada()), &content)
        .await
        .expect("256 characters fit the column");

    assert_eq!(sent.content.len(), MESSAGE_MAX_CHARS);
    assert_eq!(h.announced(), vec![WsEvent::NewMessage]);
}

// ── reading ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_history_is_newest_first_and_pages_without_losing_a_row() {
    let h = harness();
    let oldest = seed(&h, 1, ada(), ago(300));
    let middle = seed(&h, 2, grace(), ago(200));
    let newest = seed(&h, 3, ada(), ago(100));

    let first = h
        .service
        .messages(h.member(ada()), &page_of(2))
        .await
        .expect("a participant may read the history");

    assert_eq!(
        first.data.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![newest, middle],
        "newest first"
    );
    assert!(first.has_next_page, "a third row is waiting");

    let cursor = first.next_cursor.clone().expect("a cursor for page two");
    let second = h
        .service
        .messages(
            h.member(ada()),
            &CursorParams {
                cursor: Some(cursor),
                limit: Some(2),
            },
        )
        .await
        .expect("the second page reads");

    assert_eq!(
        second.data.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![oldest]
    );
    assert!(
        !second.has_next_page,
        "the cursor must not advertise a page that does not exist"
    );

    // The pages partition the history: no row twice, none skipped.
    let mut seen: Vec<Uuid> = first.data.iter().map(|m| m.id).collect();
    seen.extend(second.data.iter().map(|m| m.id));
    seen.sort();
    assert_eq!(seen, {
        let mut all = vec![oldest, middle, newest];
        all.sort();
        all
    });
}

#[tokio::test]
async fn a_cursor_that_cannot_be_decoded_is_a_400_not_a_panic() {
    let h = harness();
    seed(&h, 1, ada(), ago(10));

    let failure = h
        .service
        .messages(
            h.member(ada()),
            &CursorParams {
                cursor: Some(Cursor("not-a-cursor".to_owned())),
                limit: Some(10),
            },
        )
        .await
        .expect_err("an opaque cursor that does not decode is the client's to fix");

    assert_eq!(
        failure.code(),
        ErrorCode::InvalidCursor,
        "a malformed cursor is the client's to fix, and names itself"
    );
}

#[tokio::test]
async fn somebody_outside_the_room_cannot_read_the_history() {
    let h = harness();
    seed(&h, 1, ada(), ago(10));

    let failure = h
        .service
        .messages(h.member(stranger()), &page_of(10))
        .await
        .expect_err("the history belongs to the room");

    assert_eq!(failure.code(), ErrorCode::NotParticipant);
}

#[tokio::test]
async fn the_history_stays_readable_after_the_broadcast_ends() {
    // The conversation a room just had is part of what the broadcast produced; hiding it
    // the moment the host ends the broadcast would throw it away.
    let h = harness();
    seed(&h, 1, ada(), ago(10));
    h.repo.end_broadcast(room());

    let page = h
        .service
        .messages(h.member(ada()), &page_of(10))
        .await
        .expect("a participant may read a finished broadcast's history");

    assert_eq!(page.data.len(), 1);
}

#[tokio::test]
async fn a_deleted_message_leaves_the_history_but_not_the_table() {
    let h = harness();
    let id = seed(&h, 1, ada(), ago(10));

    h.service
        .delete_message(h.member(ada()), id)
        .await
        .expect("the author may delete their own message");

    let page = h
        .service
        .messages(h.member(ada()), &page_of(10))
        .await
        .expect("reading still works");
    assert!(page.data.is_empty(), "a soft delete hides it from reads");

    // Soft, not hard: the row is still there, which is what makes the delete auditable.
    assert!(
        h.repo.message(id).is_some(),
        "the row must survive the delete"
    );
    assert_eq!(h.announced(), vec![WsEvent::DeletedMessage]);
}

// ── editing ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_author_edits_their_own_message_and_the_room_is_told() {
    let h = harness();
    let id = seed(&h, 1, ada(), OffsetDateTime::now_utc());

    let edited = h
        .service
        .edit_message(h.member(ada()), id, "hello, corrected")
        .await
        .expect("the author may edit inside the window");

    assert_eq!(edited.content, "hello, corrected");
    assert!(
        edited.is_edited,
        "the trigger's `updated_at` is what says so"
    );
    assert!(edited.updated_at.is_some());

    assert_eq!(h.announced(), vec![WsEvent::EditedMessage]);
    let payload = only_announcement(&h);
    assert_eq!(payload.data["content"], "hello, corrected");
    assert_eq!(payload.data["isEdited"], serde_json::json!(true));
}

#[tokio::test]
async fn editing_somebody_elses_message_is_forbidden_and_changes_nothing() {
    let h = harness();
    let id = seed(&h, 1, grace(), OffsetDateTime::now_utc());

    let failure = h
        .service
        .edit_message(h.member(ada()), id, "not mine to change")
        .await
        .expect_err("only the author may edit");

    assert_eq!(failure.code(), ErrorCode::Forbidden);
    assert_eq!(
        failure.to_string(),
        error::not_sender().to_string(),
        "the refusal must say whose it is, not that it is missing"
    );
    assert_eq!(
        h.repo.message(id).expect("still stored").content,
        "seeded",
        "the body must be untouched"
    );
    h.assert_silent();
}

#[tokio::test]
async fn a_message_cannot_be_edited_in_another_broadcasts_name() {
    // Regression: `master` never compared the message's `broadcast_id` to the one in the
    // request, so a member of one room could edit their own message *in another room's
    // name* — the edit was applied and then announced to a room that had never seen it.
    let h = harness();
    let id = Uuid::from_u128(0x11);
    h.repo.seed_message(ChatMessage {
        id,
        broadcast_id: other_room(),
        sender: sender_summary(ada()),
        content: "said elsewhere".to_owned(),
        created_at: OffsetDateTime::now_utc(),
        updated_at: None,
    });

    let failure = h
        .service
        .edit_message(h.member(ada()), id, "rewritten")
        .await
        .expect_err("a message is only editable in the broadcast it was written in");

    assert_eq!(
        failure.code(),
        ErrorCode::NotFound,
        "from this room the message simply does not exist"
    );
    assert_eq!(
        h.repo.message(id).expect("still stored").content,
        "said elsewhere",
        "the other room's message must be untouched"
    );
    h.assert_silent();
}

#[tokio::test]
async fn an_edit_after_the_window_is_bad_request_and_changes_nothing() {
    let h = harness();
    // One second past the fifteen-minute window.
    let id = seed(&h, 1, ada(), ago(901));

    let failure = h
        .service
        .edit_message(h.member(ada()), id, "too late")
        .await
        .expect_err("the window has closed");

    assert_eq!(failure.code(), ErrorCode::BadRequest);
    assert_eq!(
        failure.to_string(),
        error::edit_window_expired().to_string()
    );
    assert_eq!(h.repo.message(id).expect("still stored").content, "seeded");
    h.assert_silent();
}

#[tokio::test]
async fn an_edit_inside_the_window_is_still_allowed() {
    // The boundary from the other side, so the test above cannot pass by refusing
    // everything: exactly fifteen minutes after it was sent, the edit still lands.
    let h = harness();
    let id = seed(&h, 1, ada(), ago(900));

    h.service
        .edit_message(h.member(ada()), id, "just in time")
        .await
        .expect("the last second of the window is inside it");
}

#[tokio::test]
async fn an_edit_while_the_broadcast_is_not_live_is_a_conflict() {
    let h = harness();
    let id = seed(&h, 1, ada(), OffsetDateTime::now_utc());
    h.repo.end_broadcast(room());

    let failure = h
        .service
        .edit_message(h.member(ada()), id, "after the show")
        .await
        .expect_err("an ended broadcast is not editable");

    assert_eq!(failure.code(), ErrorCode::Conflict);
    assert_eq!(h.repo.message(id).expect("still stored").content, "seeded");
    h.assert_silent();
}

#[tokio::test]
async fn an_edit_of_a_deleted_message_is_not_found() {
    let h = harness();
    let id = seed(&h, 1, ada(), OffsetDateTime::now_utc());
    h.repo
        .soft_delete_message(id, ada())
        .await
        .expect("the delete succeeds");

    let failure = h
        .service
        .edit_message(h.member(ada()), id, "back from the dead")
        .await
        .expect_err("a deleted message cannot be edited");

    assert_eq!(failure.code(), ErrorCode::NotFound);
    h.assert_silent();
}

// ── deleting ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn deleting_something_that_is_not_yours_and_something_gone_are_different_answers() {
    // Regression: `master` answered `NOT_MESSAGE_SENDER` for *every* failed delete,
    // including a message that had already been deleted — a state the caller cannot act
    // on, and the wrong explanation for it.
    let h = harness();
    let hers = seed(&h, 1, grace(), OffsetDateTime::now_utc());

    let not_mine = h
        .service
        .delete_message(h.member(ada()), hers)
        .await
        .expect_err("only the author may delete");
    assert_eq!(not_mine.code(), ErrorCode::Forbidden);
    assert_eq!(not_mine.to_string(), error::not_sender().to_string());

    // Now a message that does not exist at all.
    let missing = h
        .service
        .delete_message(h.member(ada()), Uuid::from_u128(0xdead))
        .await
        .expect_err("there is no such message");
    assert_eq!(missing.code(), ErrorCode::NotFound);
    assert_ne!(
        missing.to_string(),
        not_mine.to_string(),
        "a gone message and somebody else's message are not the same answer"
    );

    // And a second delete of Ada's own message is the same "gone" answer.
    let mine = seed(&h, 2, ada(), OffsetDateTime::now_utc());
    h.service
        .delete_message(h.member(ada()), mine)
        .await
        .expect("the first delete succeeds");
    let again = h
        .service
        .delete_message(h.member(ada()), mine)
        .await
        .expect_err("a deleted message is gone");
    assert_eq!(again.code(), ErrorCode::NotFound);

    assert_eq!(h.announced(), vec![WsEvent::DeletedMessage]);
}

#[tokio::test]
async fn a_message_may_be_deleted_after_the_broadcast_ends() {
    // Deliberately laxer than sending and editing: a host clearing something they regret
    // should not need the room to be live to do it.
    let h = harness();
    let id = seed(&h, 1, ada(), ago(10));
    h.repo.end_broadcast(room());

    h.service
        .delete_message(h.member(ada()), id)
        .await
        .expect("the author may still delete their own message");

    assert_eq!(h.announced(), vec![WsEvent::DeletedMessage]);
}

// ── reactions ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_reaction_from_a_participant_is_stored_and_announced() {
    let h = harness();

    let sent = h
        .service
        .send_reaction(h.member(grace()), "👏")
        .await
        .expect("a participant may react");

    assert_eq!(sent.content, "👏");
    assert_eq!(sent.sender_id, grace());
    assert_eq!(sent.broadcast_id, room());
    assert_eq!(h.repo.reactions().len(), 1);

    assert_eq!(h.announced(), vec![WsEvent::NewReaction]);
    let payload = only_announcement(&h);
    assert_eq!(payload.data["content"], "👏");
    assert_eq!(payload.data["senderId"], grace().to_string());
    assert_eq!(h.publisher.published()[0].0, room());
}

#[tokio::test]
async fn a_reaction_outside_a_live_broadcast_is_refused_and_silent() {
    let h = harness();
    h.repo.end_broadcast(room());

    let failure = h
        .service
        .send_reaction(h.member(ada()), "👏")
        .await
        .expect_err("an ended broadcast accepts nothing");

    assert_eq!(failure.code(), ErrorCode::Conflict);
    assert!(h.repo.reactions().is_empty());
    h.assert_silent();
}

#[tokio::test]
async fn a_reaction_from_somebody_outside_the_room_is_forbidden() {
    let h = harness();

    let failure = h
        .service
        .send_reaction(h.member(stranger()), "👏")
        .await
        .expect_err("only participants may react");

    assert_eq!(failure.code(), ErrorCode::NotParticipant);
    assert!(h.repo.reactions().is_empty());
    h.assert_silent();
}

// ── the fan-out contract, on its own ──────────────────────────────────────

#[tokio::test]
async fn every_announcement_names_the_room_it_belongs_to() {
    // A payload delivered to the wrong room is a message shown to people who never asked
    // for it — the one failure mode a client cannot detect and recover from.
    let h = harness();

    h.service
        .send_message(h.member(ada()), "one")
        .await
        .expect("posts");
    h.service
        .send_reaction(h.member(ada()), "👏")
        .await
        .expect("reacts");

    let rooms: Vec<Uuid> = h.publisher.published().iter().map(|(id, _)| *id).collect();
    assert_eq!(rooms, vec![room(), room()]);
}
