//! §10 **service** layer: the moderation rules, against the in-memory double.
//!
//! # What this suite proves
//!
//! Every assertion runs through a real [`ModerationService`] with
//! [`InMemoryModerationRepo`] behind it, so it proves the *rules*: a report must name
//! something that exists, the queue is an operator's, `open` is not a resolution, a
//! self-block is a 400 and a repeated block is one row.
//!
//! # The two rules that live in other modules
//!
//! A block only means something where something is delivered, so the last two sections
//! drive the modules that honour one: `SubscribersService` refuses a follow of an account
//! that blocked the follower, and `ChatService` refuses a message in a room whose host
//! blocked the sender. Both run against their own doubles, with the block seeded the way
//! `user_blocks` would hold it.
//!
//! It does **not** run a line of SQL: `moderation_repository_pg.rs` does, against a live
//! Postgres, and holds the statements' shape.

// The panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use std::sync::Arc;

use meno_api::middleware::auth::{AuthUser, UserRole};
use meno_api::modules::chat::model::SenderSummary;
use meno_api::modules::chat::publisher::ChatPublisher;
use meno_api::modules::chat::publisher::recording::RecordingPublisher;
use meno_api::modules::chat::repository::{ChatRepo, InMemoryChatRepo};
use meno_api::modules::chat::service::{ChatMember, ChatService};
use meno_api::modules::moderation::dto::CreateReportRequest;
use meno_api::modules::moderation::model::{ReportStatus, ReportSubject};
use meno_api::modules::moderation::repository::{InMemoryModerationRepo, ModerationRepo};
use meno_api::modules::moderation::service::ModerationService;
use meno_api::modules::subscribers::repository::{InMemorySubscribersRepo, SubscribersRepo};
use meno_api::modules::subscribers::service::SubscribersService;
use meno_core::{CursorParams, ErrorCode};
use uuid::Uuid;

// ── fixtures ──────────────────────────────────────────────────────────────

fn ada() -> Uuid {
    Uuid::from_u128(0x1)
}

fn grace() -> Uuid {
    Uuid::from_u128(0x2)
}

fn admin() -> Uuid {
    Uuid::from_u128(0xa)
}

fn room() -> Uuid {
    Uuid::from_u128(0x100)
}

fn viewer(id: Uuid, role: UserRole) -> AuthUser {
    AuthUser {
        id,
        jti: Uuid::from_u128(0xdead),
        full_name: "A Caller".to_owned(),
        email: "caller@example.test".to_owned(),
        verified: true,
        providers: Vec::new(),
        role,
    }
}

fn page(limit: i64) -> CursorParams {
    CursorParams {
        cursor: None,
        limit: Some(limit),
    }
}

struct Harness {
    service: ModerationService,
    repo: Arc<InMemoryModerationRepo>,
}

/// Two live accounts — Ada and Grace — one broadcast, and no reports or blocks yet.
fn harness() -> Harness {
    let repo = Arc::new(InMemoryModerationRepo::new());
    repo.add_user(ada(), "Ada Lovelace");
    repo.add_user(grace(), "Grace Hopper");
    repo.add_broadcast(room());

    let service = ModerationService::new(Arc::clone(&repo) as Arc<dyn ModerationRepo>);
    Harness { service, repo }
}

fn report_about_user(subject: Uuid, reason: &str) -> CreateReportRequest {
    CreateReportRequest {
        subject_type: ReportSubject::User,
        subject_id: subject,
        reason: reason.to_owned(),
        details: None,
    }
}

// ── filing a report ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_report_about_a_live_account_is_stored_and_starts_open() {
    let harness = harness();

    let report = harness
        .service
        .report(
            &viewer(ada(), UserRole::User),
            &report_about_user(grace(), "  spam  "),
        )
        .await
        .expect("a report about a live account is accepted");

    assert_eq!(report.status, ReportStatus::Open);
    assert!(report.resolved_at.is_none());
    assert_eq!(report.reporter_id, ada());
    assert_eq!(report.subject_id, grace());
    assert_eq!(
        report.reason, "spam",
        "the reason is trimmed at the boundary"
    );
    assert_eq!(harness.repo.stored_reports().len(), 1);
}

#[tokio::test]
async fn a_report_about_something_that_does_not_exist_is_a_404() {
    let harness = harness();

    let missing_user = harness
        .service
        .report(
            &viewer(ada(), UserRole::User),
            &report_about_user(Uuid::from_u128(0x99), "spam"),
        )
        .await
        .expect_err("an unknown account cannot be reported");
    assert_eq!(missing_user.code(), ErrorCode::NotFound);

    let missing_broadcast = harness
        .service
        .report(
            &viewer(ada(), UserRole::User),
            &CreateReportRequest {
                subject_type: ReportSubject::Broadcast,
                subject_id: Uuid::from_u128(0x98),
                reason: "harassment".to_owned(),
                details: None,
            },
        )
        .await
        .expect_err("an unknown broadcast cannot be reported");
    assert_eq!(missing_broadcast.code(), ErrorCode::NotFound);

    assert!(harness.repo.stored_reports().is_empty());
}

#[tokio::test]
async fn a_body_that_breaks_the_bounds_never_reaches_storage() {
    let harness = harness();

    let refusal = harness
        .service
        .report(
            &viewer(ada(), UserRole::User),
            &report_about_user(grace(), ""),
        )
        .await
        .expect_err("a blank reason is refused");

    assert_eq!(refusal.code(), ErrorCode::ValidationFailed);
    assert!(harness.repo.stored_reports().is_empty());
}

#[tokio::test]
async fn a_report_about_a_broadcast_is_accepted() {
    let harness = harness();

    let report = harness
        .service
        .report(
            &viewer(ada(), UserRole::User),
            &CreateReportRequest {
                subject_type: ReportSubject::Broadcast,
                subject_id: room(),
                reason: "harassment".to_owned(),
                details: Some("repeated slurs".to_owned()),
            },
        )
        .await
        .expect("the broadcast exists");

    assert_eq!(report.subject_type, ReportSubject::Broadcast);
    assert_eq!(report.details.as_deref(), Some("repeated slurs"));
}

// ── the queue ────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_queue_is_an_operators_and_says_so() {
    let harness = harness();

    for role in [UserRole::User, UserRole::Creator] {
        let refusal = harness
            .service
            .reports(&viewer(ada(), role), None, &page(10))
            .await
            .expect_err("a non-operator cannot read the queue");
        assert_eq!(refusal.code(), ErrorCode::Forbidden);

        let resolve = harness
            .service
            .resolve(&viewer(ada(), role), Uuid::new_v4(), ReportStatus::Reviewed)
            .await
            .expect_err("a non-operator cannot resolve a report");
        assert_eq!(resolve.code(), ErrorCode::Forbidden);
    }
}

#[tokio::test]
async fn the_queue_filters_by_status_and_paginates() {
    let harness = harness();

    let first = harness
        .service
        .report(
            &viewer(ada(), UserRole::User),
            &report_about_user(grace(), "spam"),
        )
        .await
        .expect("the first report is filed");
    harness
        .service
        .report(
            &viewer(ada(), UserRole::User),
            &report_about_user(grace(), "abuse"),
        )
        .await
        .expect("the second report is filed");

    let everything = harness
        .service
        .reports(&viewer(admin(), UserRole::Admin), None, &page(1))
        .await
        .expect("an operator reads the queue");
    assert_eq!(everything.data.len(), 1, "the page carries its limit");
    assert!(everything.has_next_page, "a second report is waiting");

    harness
        .service
        .resolve(
            &viewer(admin(), UserRole::Admin),
            first.id,
            ReportStatus::Reviewed,
        )
        .await
        .expect("an operator resolves the first report");

    let open = harness
        .service
        .reports(
            &viewer(admin(), UserRole::Admin),
            Some(ReportStatus::Open),
            &page(10),
        )
        .await
        .expect("the filtered queue is read");
    assert_eq!(open.data.len(), 1);
    assert_eq!(open.data[0].status, ReportStatus::Open);
}

#[tokio::test]
async fn a_resolution_records_the_state_and_the_instant() {
    let harness = harness();

    let report = harness
        .service
        .report(
            &viewer(ada(), UserRole::User),
            &report_about_user(grace(), "spam"),
        )
        .await
        .expect("the report is filed");

    let resolved = harness
        .service
        .resolve(
            &viewer(admin(), UserRole::Admin),
            report.id,
            ReportStatus::Dismissed,
        )
        .await
        .expect("an operator resolves it");

    assert_eq!(resolved.id, report.id);
    assert_eq!(resolved.status, ReportStatus::Dismissed);
    assert!(resolved.resolved_at.is_some());
}

#[tokio::test]
async fn open_is_not_a_resolution_and_a_missing_report_is_a_404() {
    let harness = harness();

    let report = harness
        .service
        .report(
            &viewer(ada(), UserRole::User),
            &report_about_user(grace(), "spam"),
        )
        .await
        .expect("the report is filed");

    let reopened = harness
        .service
        .resolve(
            &viewer(admin(), UserRole::Admin),
            report.id,
            ReportStatus::Open,
        )
        .await
        .expect_err("a report cannot be reopened");
    assert_eq!(reopened.code(), ErrorCode::BadRequest);

    let missing = harness
        .service
        .resolve(
            &viewer(admin(), UserRole::Admin),
            Uuid::from_u128(0x99),
            ReportStatus::Reviewed,
        )
        .await
        .expect_err("no report carries the id");
    assert_eq!(missing.code(), ErrorCode::NotFound);
}

// ── blocks ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn blocking_is_one_row_and_idempotent_in_both_directions() {
    let harness = harness();

    harness
        .service
        .block(&viewer(ada(), UserRole::User), grace())
        .await
        .expect("Ada blocks Grace");
    harness
        .service
        .block(&viewer(ada(), UserRole::User), grace())
        .await
        .expect("blocking twice is the same decision");
    assert_eq!(harness.repo.blocks(), vec![(ada(), grace())]);

    let list = harness
        .service
        .blocked(&viewer(ada(), UserRole::User))
        .await
        .expect("the block list is read");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, grace());
    assert_eq!(list[0].full_name, "Grace Hopper");

    harness
        .service
        .unblock(&viewer(ada(), UserRole::User), grace())
        .await
        .expect("Ada unblocks Grace");
    harness
        .service
        .unblock(&viewer(ada(), UserRole::User), grace())
        .await
        .expect("unblocking what is not there is a success");
    assert!(harness.repo.blocks().is_empty());
}

#[tokio::test]
async fn a_block_is_one_directional() {
    let harness = harness();

    harness
        .service
        .block(&viewer(ada(), UserRole::User), grace())
        .await
        .expect("Ada blocks Grace");

    let grace_list = harness
        .service
        .blocked(&viewer(grace(), UserRole::User))
        .await
        .expect("Grace's list is read");
    assert!(
        grace_list.is_empty(),
        "a block is a decision by the blocker, not a mutual state"
    );
}

#[tokio::test]
async fn blocking_yourself_is_a_400_and_an_unknown_account_is_a_404() {
    let harness = harness();

    let own_account = harness
        .service
        .block(&viewer(ada(), UserRole::User), ada())
        .await
        .expect_err("the migration makes a self-block unrepresentable");
    assert_eq!(own_account.code(), ErrorCode::BadRequest);

    let missing = harness
        .service
        .block(&viewer(ada(), UserRole::User), Uuid::from_u128(0x99))
        .await
        .expect_err("no live account carries the id");
    assert_eq!(missing.code(), ErrorCode::NotFound);

    assert!(harness.repo.blocks().is_empty());
}

// ── the follow rule ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_blocked_account_cannot_follow_the_account_that_blocked_it() {
    let repo = Arc::new(InMemorySubscribersRepo::new());
    repo.seed_user(ada(), "Ada Lovelace", None, None);
    repo.seed_user(grace(), "Grace Hopper", None, None);
    repo.seed_block(grace(), ada());

    let service = SubscribersService::new(Arc::clone(&repo) as Arc<dyn SubscribersRepo>);

    let refusal = service
        .subscribe(ada(), grace())
        .await
        .expect_err("Grace has blocked Ada");
    assert_eq!(refusal.code(), ErrorCode::Forbidden);
    assert!(
        meno_core::to_body(&refusal).message.contains("blocked"),
        "the caller is told what refused them: {refusal}"
    );
    assert_eq!(repo.stored_follow_count(), 0);

    // The other direction is untouched: Grace may still follow Ada.
    service
        .subscribe(grace(), ada())
        .await
        .expect("a block is one-directional");
    assert!(repo.follows(grace(), ada()));
}

// ── the chat rule ────────────────────────────────────────────────────────

fn chat_harness() -> (ChatService, Arc<InMemoryChatRepo>, Arc<RecordingPublisher>) {
    let repo = Arc::new(InMemoryChatRepo::new());
    let publisher = Arc::new(RecordingPublisher::new());

    for (id, name) in [(ada(), "Ada Lovelace"), (grace(), "Grace Hopper")] {
        repo.add_user(SenderSummary {
            id,
            full_name: name.to_owned(),
            bio: None,
            avatar_id: None,
            avatar_url: None,
        });
    }

    repo.start_broadcast(room());
    // Grace is the host; both accounts are in the room.
    repo.set_creator(room(), grace());
    repo.add_participant(room(), ada());
    repo.add_participant(room(), grace());

    let service = ChatService::new(
        Arc::clone(&repo) as Arc<dyn ChatRepo>,
        Arc::clone(&publisher) as Arc<dyn ChatPublisher>,
    );
    (service, repo, publisher)
}

#[tokio::test]
async fn a_message_from_somebody_the_host_blocked_never_lands() {
    let (service, repo, publisher) = chat_harness();
    repo.add_block(grace(), ada());

    let refusal = service
        .send_message(
            ChatMember {
                broadcast_id: room(),
                user_id: ada(),
            },
            "hello",
        )
        .await
        .expect_err("the host has blocked Ada");

    assert_eq!(refusal.code(), ErrorCode::Forbidden);
    assert!(
        meno_core::to_body(&refusal).message.contains("blocked"),
        "the caller is told what refused them: {refusal}"
    );
    assert_eq!(repo.stored_message_count(), 0, "nothing was written");
    assert!(publisher.is_empty(), "nothing was announced");
}

#[tokio::test]
async fn the_host_itself_is_not_blocked_by_its_own_room() {
    let (service, repo, _publisher) = chat_harness();
    repo.add_block(grace(), ada());

    service
        .send_message(
            ChatMember {
                broadcast_id: room(),
                user_id: grace(),
            },
            "welcome",
        )
        .await
        .expect("only the blocked account is refused");
}
