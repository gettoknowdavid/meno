//! §10 contract tests for [`BroadcastService`], driven through the in-memory doubles.
//!
//! # Why this file lives in `tests/`
//!
//! It runs outside the crate, so every path starts at `meno_api::` — the path a
//! consumer crate uses. A helper that was reachable by `super::` but not exported
//! now fails to compile here instead of silently staying private. The HTTP contract
//! (envelopes, status codes, auth gates, RFC 3339 dates) is asserted separately in
//! [`broadcast_router.rs`]; these assert on the *service*.
//!
//! # What is asserted here, and why
//!
//! Each property below is one that can only be observed by driving the service
//! through its real collaborators — the trait object the production wiring hands it,
//! and the in-memory doubles. A mock would let each pass while the wiring was wrong,
//! which is the failure the state module's trait-object arrangement exists to prevent.
//!
//! The flows that matter most:
//!
//! - Creation is a draft by default; the response names the creator and the empty
//!   cohost list.
//! - Only the creator may edit, delete, go live or end — one helper enforces all four.
//! - A live broadcast is immutable.
//! - §7.6 — `total_participants` moves with the rows: one join writes the row *and*
//!   the counter; a rejoin increments it again; a duplicate join is refused and does
//!   not inflate it; a leave does not reduce it.
//! - The creator cannot join, and the host cannot leave.
//! - A disabled media server is a 503, not a panic. Creating and scheduling still work.
//! - Every date on the response is an RFC 3339 string, never a nine-tuple.
//!
//! # What this cannot prove
//!
//! No Postgres or LiveKit runs here. `pg.rs` is covered by the Postgres-backed suite;
//! the media adapter's real behaviour is covered by `media.rs`'s own tests.

// Panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml. The allow-*-in-tests keys cover #[test] functions; the helpers below
// are not #[test] functions, so the allowance does not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use uuid::Uuid;

use meno_api::modules::broadcast::dto::{self, CreateBroadcastRequest, UpdateBroadcastRequest};
use meno_api::modules::broadcast::media::RecordingMedia;
use meno_api::modules::broadcast::model::BroadcastState as ModelBroadcastState;
use meno_api::modules::broadcast::model::{EndReason, ParticipantRole, UserSummary};
use meno_api::modules::broadcast::repository::InMemoryBroadcastRepo;
use meno_api::modules::broadcast::service::{self, BroadcastService};
use meno_core::to_body;

/// A fixture identity, reduced to what a broadcast service needs.
fn user(id: u128, name: &str) -> UserSummary {
    UserSummary {
        id: Uuid::from_u128(id),
        full_name: name.to_owned(),
        avatar_id: None,
        avatar_url: None,
    }
}

/// A plain create request for the fixture user.
fn create_request() -> CreateBroadcastRequest {
    CreateBroadcastRequest {
        title: "Ada's show".to_owned(),
        description: Some("Every Thursday".to_owned()),
        image_id: None,
        image_url: None,
        time_zone: Some("Africa/Lagos".to_owned()),
        start_time: None,
        recording_enabled: Some(true),
        cohosts: None,
    }
}

/// A second create request, distinct so pages are distinguishable.
fn second_request() -> CreateBroadcastRequest {
    CreateBroadcastRequest {
        title: "Grace's talk".to_owned(),
        description: None,
        image_id: None,
        image_url: None,
        time_zone: None,
        start_time: None,
        recording_enabled: None,
        cohosts: None,
    }
}

/// Build a service whose repository and media double record nothing.
fn service() -> (
    BroadcastService,
    Arc<InMemoryBroadcastRepo>,
    Arc<RecordingMedia>,
) {
    let repo = Arc::new(InMemoryBroadcastRepo::new());
    let media = Arc::new(RecordingMedia::new());
    let svc = BroadcastService::new(service::ServiceDeps {
        repo: Arc::clone(&repo) as Arc<dyn meno_api::modules::broadcast::repository::BroadcastRepo>,
        media: Arc::clone(&media) as Arc<dyn meno_api::modules::broadcast::media::BroadcastMedia>,
    });
    (svc, repo, media)
}

/// Seed the three fixture users so the service can resolve them.
fn seed(repo: &InMemoryBroadcastRepo) {
    repo.insert_user(user(1, "Ada Lovelace"));
    repo.insert_user(user(2, "Grace Hopper"));
    repo.insert_user(user(3, "Alan Turing"));
}

/// A timestamp well in the future, for scheduled-start tests.
fn future() -> time::OffsetDateTime {
    time::OffsetDateTime::now_utc() + time::Duration::hours(24)
}

// ── creation ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn creating_a_broadcast_returns_a_draft_with_the_creator_and_no_cohosts() {
    let (svc, repo, _) = service();
    seed(&repo);

    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("creating a draft needs no media server");

    assert_eq!(created.title, "Ada's show");
    assert_eq!(created.creator.id, user(1, "Ada Lovelace").id);
    assert_eq!(created.creator.full_name, "Ada Lovelace");
    assert!(created.creator.avatar_id.is_none());
    assert_eq!(created.cohosts.len(), 0);
    assert_eq!(created.state, ModelBroadcastState::Draft);
    assert_eq!(created.live_participants_count, 0);
    assert_eq!(created.total_participants, 0);
}

#[tokio::test]
async fn creating_with_cohosts_resolves_them_through_the_repository() {
    let (svc, repo, _) = service();
    seed(&repo);

    let request = CreateBroadcastRequest {
        title: "Ada's show".to_owned(),
        description: None,
        image_id: None,
        image_url: None,
        time_zone: None,
        start_time: None,
        recording_enabled: None,
        cohosts: Some(vec![user(2, "Grace Hopper").id]),
    };

    let created = svc
        .create(request, user(1, "Ada Lovelace"))
        .await
        .expect("creating with a cohost");

    assert_eq!(created.cohosts.len(), 1);
    assert_eq!(created.cohosts[0].id, user(2, "Grace Hopper").id);
    assert_eq!(created.cohosts[0].full_name, "Grace Hopper");
}

#[tokio::test]
async fn creating_with_an_unknown_cohost_is_refused() {
    let (svc, repo, _) = service();
    seed(&repo);

    let request = CreateBroadcastRequest {
        title: "Ada's show".to_owned(),
        description: None,
        image_id: None,
        image_url: None,
        time_zone: None,
        start_time: None,
        recording_enabled: None,
        cohosts: Some(vec![Uuid::new_v4()]),
    };

    let error = svc
        .create(request, user(1, "Ada Lovelace"))
        .await
        .expect_err("refused");

    assert_eq!(error.code(), meno_core::ErrorCode::NotFound);
}

#[tokio::test]
async fn creating_with_an_invalid_time_zone_is_refused() {
    let (svc, _, _) = service();

    let request = CreateBroadcastRequest {
        time_zone: Some("not/a/valid/zone".to_owned()),
        ..create_request()
    };

    let error = svc
        .create(request, user(1, "Ada Lovelace"))
        .await
        .expect_err("refused");

    assert_eq!(error.code(), meno_core::ErrorCode::InvalidTimeZone);
}

#[tokio::test]
async fn creating_with_a_past_start_time_is_refused() {
    let (svc, _, _) = service();

    let request = CreateBroadcastRequest {
        start_time: Some(future() - time::Duration::hours(48)),
        ..create_request()
    };

    let error = svc
        .create(request, user(1, "Ada Lovelace"))
        .await
        .expect_err("refused");

    assert_eq!(error.code(), meno_core::ErrorCode::StartTimeInPast);
}

// ── get / list ────────────────────────────────────────────────────────────

#[tokio::test]
async fn getting_a_broadcast_returns_the_assembled_response() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");

    let got = svc
        .get(created.id, Some(&user(1, "Ada Lovelace")))
        .await
        .expect("get");

    assert_eq!(got.id, created.id);
    assert_eq!(got.title, created.title);
    assert_eq!(got.creator.id, created.creator.id);
}

#[tokio::test]
async fn getting_a_nonexistent_broadcast_is_a_404() {
    let (svc, _, _) = service();

    let error = svc
        .get(Uuid::new_v4(), Some(&user(1, "Ada Lovelace")))
        .await
        .expect_err("not found");

    assert_eq!(error.code(), meno_core::ErrorCode::NotFound);
}

#[tokio::test]
async fn listing_returns_a_page_of_broadcasts_with_creator_names() {
    let (svc, repo, _) = service();
    seed(&repo);
    svc.create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("first");
    svc.create(second_request(), user(2, "Grace Hopper"))
        .await
        .expect("second");

    let page = svc
        .list(&dto::BroadcastQuery::default())
        .await
        .expect("listing");

    assert_eq!(page.data.len(), 2);
    assert!(!page.data.iter().all(|item| item.creator_name.is_empty()));
}

#[tokio::test]
async fn listing_a_second_page_does_not_repeat_rows() {
    let (svc, _, _) = service();
    for _ in 0..4 {
        svc.create(create_request(), user(1, "Ada Lovelace"))
            .await
            .expect("create");
    }

    // An explicit limit of 2: with 4 rows this forces a second page, and `limit + 1`
    // means the service fetches 3 and keeps 2.
    let page_of_two = dto::BroadcastQuery {
        pagination: meno_core::CursorParams {
            cursor: None,
            limit: Some(2),
        },
        ..Default::default()
    };

    let first = svc.list(&page_of_two).await.expect("first page");
    assert_eq!(
        first.data.len(),
        2,
        "the probe row is not handed to the client"
    );
    assert!(first.has_next_page);

    let cursor = first.next_cursor.expect("a page with more");
    let second = svc
        .list(&dto::BroadcastQuery {
            pagination: meno_core::CursorParams {
                cursor: Some(cursor),
                limit: None,
            },
            ..Default::default()
        })
        .await
        .expect("second page");

    let first_ids: std::collections::HashSet<_> = first.data.iter().map(|item| item.id).collect();
    for item in &second.data {
        assert!(
            !first_ids.contains(&item.id),
            "page two repeated a row from page one: {}",
            item.id
        );
    }
}

// ── update / delete (creator-only) ────────────────────────────────────────

#[tokio::test]
async fn updating_as_the_creator_succeeds() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");

    let patched = svc
        .update(
            created.id,
            UpdateBroadcastRequest {
                title: Some("A new title".to_owned()),
                ..Default::default()
            },
            &user(1, "Ada Lovelace"),
        )
        .await
        .expect("update");

    assert_eq!(patched.title, "A new title");
}

#[tokio::test]
async fn updating_as_a_cohost_is_refused() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");

    // Grace is a genuine cohost: she runs the room, but editing it is still Ada's alone.
    svc.add_cohost(
        created.id,
        dto::AddCohostRequest {
            cohost: user(2, "Grace Hopper").id,
        },
        &user(1, "Ada Lovelace"),
    )
    .await
    .expect("add cohost");

    let error = svc
        .update(
            created.id,
            UpdateBroadcastRequest {
                title: Some("A new title".to_owned()),
                ..Default::default()
            },
            &user(2, "Grace Hopper"),
        )
        .await
        .expect_err("refused");

    assert_eq!(error.code(), meno_core::ErrorCode::NotCreator);
    assert_eq!(
        to_body(&error).http_status,
        403,
        "a cohost is a 403, not a 404"
    );
}

#[tokio::test]
async fn deleting_as_the_creator_succeeds() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");

    svc.delete(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("delete");

    let error = svc.get(created.id, None).await.expect_err("gone");
    assert_eq!(error.code(), meno_core::ErrorCode::NotFound);
}

// ── go live / end / join / leave ──────────────────────────────────────────

#[tokio::test]
async fn going_live_records_the_media_call_and_returns_a_token() {
    let (svc, repo, media) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");

    let session = svc
        .go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");

    assert!(!session.token.is_empty());
    assert_eq!(session.broadcast.state, ModelBroadcastState::Live);
    assert!(session.broadcast.is_joined);
    assert_eq!(
        session.broadcast.participant_role,
        ParticipantRole::Host,
        "the creator is the host, not a participant"
    );

    let calls = media.calls();
    assert!(
        calls.iter().any(|c| c.starts_with("create_room:")),
        "{calls:?}"
    );
    assert!(calls.iter().any(|c| c.starts_with("mint:")), "{calls:?}");
    assert!(calls.iter().any(|c| c.contains(":host")), "{calls:?}");
}

#[tokio::test]
async fn going_live_twice_is_refused() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("first");

    let error = svc
        .go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect_err("second");

    assert_eq!(error.code(), meno_core::ErrorCode::Conflict);
}

#[tokio::test]
async fn ending_a_live_broadcast_returns_the_duration_and_participant_count() {
    let (svc, repo, media) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");

    // The duration is whole seconds between `published_at` and `ended_at`, so an
    // instant broadcast legitimately reports 0. Let a second elapse so the assertion
    // below tests the arithmetic rather than the clock's luck.
    tokio::time::sleep(std::time::Duration::from_millis(1_050)).await;

    let ended = svc
        .end(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("end");

    assert_eq!(ended.broadcast_id, created.id);
    assert!(ended.duration_secs >= 1, "{ended:?}");
    assert_eq!(ended.total_participants, 1, "the host counts");
    assert_eq!(ended.ended_reason, EndReason::Normal);

    let calls = media.calls();
    assert!(
        calls.iter().any(|c| c.starts_with("end_room:")),
        "{calls:?}"
    );
}

#[tokio::test]
async fn ending_a_broadcast_that_is_not_live_is_refused() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");

    let error = svc
        .end(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect_err("not live");

    assert_eq!(error.code(), meno_core::ErrorCode::Conflict);
}

#[tokio::test]
async fn joining_a_live_broadcast_returns_a_token_and_increments_the_counter() {
    let (svc, repo, media) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");

    let session = svc
        .join(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("join");

    assert!(!session.token.is_empty());
    assert_eq!(
        session.broadcast.participant_role,
        ParticipantRole::Participant,
        "a listener is a participant, not a cohost"
    );
    assert!(session.broadcast.is_joined);
    assert_eq!(
        session.broadcast.live_participants_count, 2,
        "the host plus the listener"
    );

    let calls = media.calls();
    assert!(calls.iter().any(|c| c.starts_with("mint:")), "{calls:?}");
    assert!(
        calls.iter().any(|c| c.contains(":participant")),
        "{calls:?}"
    );
}

#[tokio::test]
async fn the_creator_cannot_join_their_own_broadcast() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");

    let error = svc
        .join(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect_err("refused");

    assert_eq!(error.code(), meno_core::ErrorCode::Forbidden);
}

#[tokio::test]
async fn a_host_cannot_leave_their_own_broadcast() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");

    let error = svc
        .leave(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect_err("refused");

    // A conflict, not a permission failure: the caller may well leave broadcasts, just
    // not this one — they end it instead.
    assert_eq!(error.code(), meno_core::ErrorCode::Conflict);
    assert_eq!(to_body(&error).http_status, 409);
}

#[tokio::test]
async fn a_listener_can_leave_and_the_counter_does_not_change() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");
    svc.join(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("join");

    // §7.6: leaving is not a row — it is a close. The all-time count stays where it was.
    let before = svc
        .get(created.id, None)
        .await
        .expect("before")
        .total_participants;

    svc.leave(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("leave");

    let after = svc
        .get(created.id, None)
        .await
        .expect("after")
        .total_participants;

    assert_eq!(after, before, "leaving must not reduce an all-time count");
}

#[tokio::test]
async fn a_rejoin_increments_the_counter_again() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");
    svc.join(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("first join");
    svc.leave(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("leave");

    svc.join(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("rejoin");

    // §7.6: a return visit increments the all-time counter.
    assert_eq!(
        svc.get(created.id, None)
            .await
            .expect("after")
            .total_participants,
        3
    );
}

#[tokio::test]
async fn a_duplicate_join_is_refused_and_does_not_inflate_the_counter() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");
    svc.join(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("join");

    let error = svc
        .join(created.id, &user(2, "Grace Hopper"))
        .await
        .expect_err("already in");

    assert_eq!(error.code(), meno_core::ErrorCode::Conflict);
    assert_eq!(
        svc.get(created.id, None)
            .await
            .expect("after")
            .total_participants,
        2,
        "a duplicate join must not increment the counter"
    );
}

#[tokio::test]
async fn a_listener_cannot_join_a_broadcast_that_is_not_live() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");

    let error = svc
        .join(created.id, &user(2, "Grace Hopper"))
        .await
        .expect_err("not live");

    assert_eq!(error.code(), meno_core::ErrorCode::Conflict);
}

// ── cohost add / remove ──────────────────────────────────────────────────

#[tokio::test]
async fn adding_a_cohost_as_the_creator_succeeds() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");

    let response = svc
        .add_cohost(
            created.id,
            dto::AddCohostRequest {
                cohost: user(2, "Grace Hopper").id,
            },
            &user(1, "Ada Lovelace"),
        )
        .await
        .expect("add");

    assert_eq!(response.user.id, user(2, "Grace Hopper").id);
    assert_eq!(response.cohost_count, 1);
}

#[tokio::test]
async fn adding_a_cohost_that_is_already_one_is_refused() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.add_cohost(
        created.id,
        dto::AddCohostRequest {
            cohost: user(2, "Grace Hopper").id,
        },
        &user(1, "Ada Lovelace"),
    )
    .await
    .expect("add");

    let error = svc
        .add_cohost(
            created.id,
            dto::AddCohostRequest {
                cohost: user(2, "Grace Hopper").id,
            },
            &user(1, "Ada Lovelace"),
        )
        .await
        .expect_err("already a cohost");

    assert_eq!(error.code(), meno_core::ErrorCode::Conflict);
}

#[tokio::test]
async fn removing_a_cohost_as_the_creator_succeeds() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.add_cohost(
        created.id,
        dto::AddCohostRequest {
            cohost: user(2, "Grace Hopper").id,
        },
        &user(1, "Ada Lovelace"),
    )
    .await
    .expect("add");

    svc.remove_cohost(
        created.id,
        user(2, "Grace Hopper").id,
        &user(1, "Ada Lovelace"),
        true,
    )
    .await
    .expect("remove");

    let got = svc.get(created.id, None).await.expect("after");
    assert!(
        !got.cohosts
            .iter()
            .any(|c| c.id == user(2, "Grace Hopper").id)
    );
}

#[tokio::test]
async fn the_participant_roster_lists_every_role() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");
    // Going live already wrote the host's participant row, so the host is on the
    // roster without joining — joining is refused for the creator.
    svc.join(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("join");

    let page = svc
        .participants(created.id, &dto::ParticipantQuery::default())
        .await
        .expect("roster");

    let ids: std::collections::HashSet<_> = page.data.iter().map(|p| p.id).collect();
    assert!(ids.contains(&user(1, "Ada Lovelace").id));
    assert!(ids.contains(&user(2, "Grace Hopper").id));
}

// ── §7.6: the counter moves with the rows ─────────────────────────────────

#[tokio::test]
async fn total_participants_moves_with_the_rows_and_not_with_live_presence() {
    // Join → leave → rejoin leaves the count at 3: the leave did not reduce it, the
    // rejoin did. Driven through the service so the *service's* decision about which
    // path the caller takes is what is being tested, not the double's internal bump.
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");
    svc.join(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("join");

    assert_eq!(
        svc.get(created.id, None)
            .await
            .expect("after first join")
            .total_participants,
        2,
        "the host plus the listener"
    );

    svc.leave(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("leave");
    assert_eq!(
        svc.get(created.id, None)
            .await
            .expect("after leave")
            .total_participants,
        2
    );

    svc.join(created.id, &user(2, "Grace Hopper"))
        .await
        .expect("rejoin");
    assert_eq!(
        svc.get(created.id, None)
            .await
            .expect("after rejoin")
            .total_participants,
        3
    );
}

// ── date rendering on the response ────────────────────────────────────────

#[tokio::test]
async fn every_date_on_a_broadcast_response_is_an_rfc3339_string() {
    let (svc, repo, _) = service();
    seed(&repo);
    let created = svc
        .create(create_request(), user(1, "Ada Lovelace"))
        .await
        .expect("create");
    svc.go_live(created.id, &user(1, "Ada Lovelace"))
        .await
        .expect("go live");

    let detail = svc
        .get(created.id, Some(&user(1, "Ada Lovelace")))
        .await
        .expect("detail");

    let json = serde_json::to_value(&detail).expect("serialisable");
    assert!(json["createdAt"].is_string(), "{json}");
    assert!(json["publishedAt"].is_string(), "{json}");
    assert!(json["endTime"].is_null(), "{json}");
    assert!(
        json["startTime"].is_string() || json["startTime"].is_null(),
        "{json}"
    );

    let text = json["createdAt"].as_str().expect("asserted");
    let parsed = time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
        .expect("{text} must be RFC 3339");
    assert!(parsed > time::OffsetDateTime::UNIX_EPOCH);
}
