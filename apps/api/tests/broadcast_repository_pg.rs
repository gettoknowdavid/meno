//! §10 **Integration** layer: `PgBroadcastRepo` against a real Postgres.
//!
//! # Why this file exists
//!
//! `broadcast_service.rs` runs against the in-memory double, which mirrors the
//! statements' *semantics* and can therefore prove the rules — but cannot prove the
//! SQL. This is the broadcast module's half of what `chat`, `notes`, `profile`,
//! `subscribers`, `auth` and `notifications` already have, and it is the only place
//! these facts are assertable:
//!
//! - the **column list** in [`BROADCAST_COLUMNS`](PgBroadcastRepo) matches the
//!   migrations, so a projection that names a column the schema never declared fails
//!   here rather than on a request path;
//! - the **partial unique index** from migration 0017 (one live broadcast per
//!   creator) and the `mark_live` guard that turns its violation into `None`;
//! - the **participant counter**, which migration 0006's trigger moves on insert and
//!   which `join` must bump again on the rejoin arm — the §7.6 drift this repository
//!   was rewritten to fix;
//! - the **creator broadcast counter** trigger, including its `deleted_at` transition;
//! - the **`deleted_at IS NULL` predicates**, which a double models as absence and
//!   therefore cannot get wrong;
//! - the **keyset cursor** and the public-search filter, which Postgres parses and
//!   could still answer wrongly.
//!
//! Run it with:
//!
//! ```text
//! cargo test -p meno-api --features test-support --test broadcast_repository_pg
//! ```
//!
//! against a `DATABASE_URL` pointing at a Postgres the role can create databases on.
//! `#[sqlx::test]` applies every migration to a fresh per-test database and rolls it
//! back, so the tests are independent and need no cleanup.

// The panicking lints are denied workspace-wide but explicitly *allowed in tests* —
// see `clippy.toml`. The allow-*-in-tests keys cover `#[test]` functions; the helpers
// below are not `#[test]` functions, so the allowance does not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use meno_api::modules::broadcast::model::{
    BroadcastPatch, BroadcastState, BroadcastStatus, EndReason, InvitationStatus, JoinOutcome,
    JoinRequest, LeaveOutcome, NewBroadcast, ParticipantRole,
};
use meno_api::modules::broadcast::repository::{
    BroadcastFilter, BroadcastRepo, ParticipantFilter, PgBroadcastRepo, RepoDeps,
};
use meno_core::Order;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// Skip with a readable message rather than failing opaquely when `DATABASE_URL` is unset.
fn database_url() -> Option<String> {
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
}

/// Run the body against the fresh, migrated database `#[sqlx::test]` created.
macro_rules! with_pool {
    (|$pool:ident| $body:block) => {{
        if database_url().is_none() {
            eprintln!("skipping: set DATABASE_URL to a reachable Postgres to run this");
            return;
        }
        let $pool = $pool;
        $body
    }};
}

// ── fixtures ───────────────────────────────────────────────────────────────

/// Insert a live account directly — the fixture, not the subject under test.
async fn user(pool: &PgPool, id: Uuid, full_name: &str, email: &str) {
    sqlx::query(
        "INSERT INTO users (id, full_name, email, verified, created_at) \
         VALUES ($1, $2, $3, true, now())",
    )
    .bind(id)
    .bind(full_name)
    .bind(email)
    .execute(pool)
    .await
    .expect("the fixture account is inserted");
}

/// Three live accounts: Ada, Grace and Adam.
async fn accounts(pool: &PgPool) {
    user(pool, ada(), "Ada Lovelace", "ada@example.com").await;
    user(pool, grace(), "Grace Hopper", "grace@example.com").await;
    user(pool, adam(), "Adam Smith", "adam@example.com").await;
}

fn repo(pool: &PgPool) -> PgBroadcastRepo {
    PgBroadcastRepo::new(RepoDeps { pool: pool.clone() })
}

fn new_broadcast(creator_id: Uuid, title: &str) -> NewBroadcast {
    NewBroadcast {
        creator_id,
        title: title.to_owned(),
        description: None,
        time_zone: None,
        image_id: None,
        image_url: None,
        start_time: None,
        recording_enabled: false,
        cohosts: Vec::new(),
    }
}

/// The creator's `users.broadcasts` counter, as migration 0004's trigger left it.
async fn broadcast_count(pool: &PgPool, id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT broadcasts FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("the counter is read")
}

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds).expect("a plausible instant")
}

fn ada() -> Uuid {
    Uuid::from_u128(1)
}

fn grace() -> Uuid {
    Uuid::from_u128(2)
}

fn adam() -> Uuid {
    Uuid::from_u128(3)
}

fn unknown() -> Uuid {
    Uuid::from_u128(0xff)
}

fn page() -> BroadcastFilter {
    BroadcastFilter {
        limit_plus_one: 21,
        order: Order::Desc,
        ..BroadcastFilter::default()
    }
}

// ── creation, the projection and the creator counter ───────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_broadcast_round_trips_with_its_cohosts_and_moves_the_creator_counter(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);

        let created = repo
            .create(NewBroadcast {
                cohosts: vec![grace()],
                ..new_broadcast(ada(), "Ada's first show")
            })
            .await
            .expect("the broadcast is created");

        // The projection names every column `Broadcast` decodes; a missing one
        // fails here rather than on a detail request.
        let found = repo
            .find_by_id(created.id)
            .await
            .expect("the read runs")
            .expect("the row is there");
        assert_eq!(found.title, "Ada's first show");
        assert_eq!(found.creator_id, ada());
        assert_eq!(found.status, BroadcastStatus::Inactive);
        assert_eq!(found.total_participants, 0);
        assert_eq!(found.state(), BroadcastState::Draft);

        let cohosts = repo
            .list_cohosts(created.id)
            .await
            .expect("the cohost list reads");
        assert_eq!(cohosts.len(), 1, "the create attached its cohost");
        assert_eq!(cohosts[0].cohost_id, grace());
        assert_eq!(
            cohosts[0].invited_by,
            ada(),
            "the creator is recorded as the inviter"
        );

        // Migration 0004's trigger owns this counter; nothing in the repository
        // writes it, so this is the only tier that can prove it moved.
        assert_eq!(broadcast_count(&pool, ada()).await, 1);

        // The broadcast module's own user projection, not auth's.
        let summary = repo
            .find_user(ada())
            .await
            .expect("the user read runs")
            .expect("a live account is found");
        assert_eq!(summary.full_name, "Ada Lovelace");
        assert_eq!(
            repo.find_users(&[ada(), grace(), unknown()])
                .await
                .expect("the batch read runs")
                .len(),
            2,
            "one query returns both live accounts and invents nothing for the unknown one"
        );
    });
}

// ── going live, and the one-live-per-creator index ─────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn going_live_is_once_per_row_and_once_per_creator(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);

        let first = repo
            .create(new_broadcast(ada(), "First"))
            .await
            .expect("created");
        let second = repo
            .create(new_broadcast(ada(), "Second"))
            .await
            .expect("created");

        let live = repo
            .mark_live(first.id, at(10))
            .await
            .expect("the update runs")
            .expect("an inactive row goes live");
        assert_eq!(live.status, BroadcastStatus::Active);
        assert_eq!(live.published_at, Some(at(10)));
        assert_eq!(live.state(), BroadcastState::Live);

        assert!(
            repo.mark_live(first.id, at(11))
                .await
                .expect("the update runs")
                .is_none(),
            "a second go-live must not overwrite the first published_at"
        );

        // 0017's partial unique index: this creator is already on air, so the
        // second row's go-live is a refusal rather than a 500.
        assert!(
            repo.mark_live(second.id, at(12))
                .await
                .expect("the update runs")
                .is_none(),
            "the one-live-per-creator index turns the race into a refusal"
        );

        let on_air = repo
            .find_live_by_creator(ada())
            .await
            .expect("the read runs")
            .expect("Ada is live");
        assert_eq!(on_air.id, first.id);

        let live_rows = repo.list_live().await.expect("the sweep read runs");
        assert_eq!(live_rows.len(), 1, "only one row is on air");
        assert_eq!(live_rows[0].creator_id, ada());
        assert_eq!(live_rows[0].abandoned_since, None);
    });
}

// ── patches ────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_patch_leaves_absent_columns_alone_and_replaces_the_cohost_list(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);

        let created = repo
            .create(NewBroadcast {
                description: Some("original".to_owned()),
                cohosts: vec![grace()],
                ..new_broadcast(ada(), "Original title")
            })
            .await
            .expect("created");

        let updated = repo
            .update(
                created.id,
                BroadcastPatch {
                    title: Some("Renamed".to_owned()),
                    recording_enabled: Some(true),
                    cohosts: Some(vec![adam()]),
                    ..BroadcastPatch::default()
                },
            )
            .await
            .expect("the update runs");

        assert_eq!(updated.title, "Renamed");
        assert!(updated.recording_enabled);
        assert_eq!(
            updated.description.as_deref(),
            Some("original"),
            "an absent field must leave its column alone"
        );

        let cohosts = repo
            .list_cohosts(created.id)
            .await
            .expect("the cohost list reads");
        assert_eq!(cohosts.len(), 1);
        assert_eq!(
            cohosts[0].cohost_id,
            adam(),
            "the patch's list is a full replacement"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn soft_deleting_hides_the_row_and_decrements_the_creator_counter(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);

        let created = repo
            .create(new_broadcast(ada(), "Temporary"))
            .await
            .expect("created");
        assert_eq!(broadcast_count(&pool, ada()).await, 1);

        assert!(repo.soft_delete(created.id).await.expect("the update runs"));
        assert!(
            repo.find_by_id(created.id)
                .await
                .expect("the read runs")
                .is_none(),
            "every read filters deleted_at"
        );
        assert!(
            !repo.soft_delete(created.id).await.expect("the update runs"),
            "a second delete changes nothing"
        );
        assert_eq!(
            broadcast_count(&pool, ada()).await,
            0,
            "the trigger decrements on the deleted_at transition"
        );
        assert!(
            repo.list(page()).await.expect("the list runs").is_empty(),
            "a soft-deleted row is not a list result"
        );
    });
}

// ── participants and the all-time counter ──────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn joining_has_three_outcomes_and_only_a_rejoin_moves_the_all_time_count(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);
        let broadcast = repo
            .create(new_broadcast(ada(), "Room"))
            .await
            .expect("created");

        let join = |participant_id: Uuid, role: ParticipantRole| JoinRequest {
            broadcast_id: broadcast.id,
            participant_id,
            role,
        };

        assert_eq!(
            repo.join(join(grace(), ParticipantRole::Participant))
                .await
                .expect("the join runs"),
            JoinOutcome::Joined
        );
        assert_eq!(
            repo.join(join(grace(), ParticipantRole::Participant))
                .await
                .expect("the join runs"),
            JoinOutcome::AlreadyPresent,
            "a double tap is not a second join"
        );
        assert_eq!(
            repo.live_participant_count(broadcast.id)
                .await
                .expect("the count runs"),
            1
        );

        assert_eq!(
            repo.leave(broadcast.id, grace())
                .await
                .expect("the leave runs"),
            LeaveOutcome::Left
        );
        let left = repo
            .find_participant(broadcast.id, grace())
            .await
            .expect("the read runs")
            .expect("the row is kept");
        assert!(!left.is_present(), "leaving closes the row, not deletes it");

        assert_eq!(
            repo.join(join(grace(), ParticipantRole::Participant))
                .await
                .expect("the join runs"),
            JoinOutcome::Rejoined,
            "a return visit re-opens the row"
        );

        let row = repo
            .find_by_id(broadcast.id)
            .await
            .expect("the read runs")
            .expect("the broadcast is there");
        assert_eq!(
            row.total_participants, 2,
            "the insert arm moved the trigger's counter once and the rejoin arm once more"
        );
        assert_eq!(
            repo.live_participant_count(broadcast.id)
                .await
                .expect("the count runs"),
            1,
            "the live count is rows, not visits"
        );

        assert_eq!(
            repo.leave(broadcast.id, unknown())
                .await
                .expect("the leave runs"),
            LeaveOutcome::NotPresent
        );

        // The host cannot leave their own room.
        repo.join(join(ada(), ParticipantRole::Host))
            .await
            .expect("the host joins");
        assert_eq!(
            repo.leave(broadcast.id, ada())
                .await
                .expect("the leave runs"),
            LeaveOutcome::IsHost
        );

        let roster = repo
            .list_participants(
                broadcast.id,
                ParticipantFilter {
                    present_only: true,
                    limit_plus_one: 21,
                    order: Order::Asc,
                    ..ParticipantFilter::default()
                },
            )
            .await
            .expect("the roster reads");
        assert_eq!(roster.len(), 2, "Grace and the host are present");

        let hosts = repo
            .list_participants(
                broadcast.id,
                ParticipantFilter {
                    role: Some(ParticipantRole::Host),
                    limit_plus_one: 21,
                    order: Order::Asc,
                    ..ParticipantFilter::default()
                },
            )
            .await
            .expect("the roster reads");
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].participant_id, ada());
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn ending_closes_every_open_row_and_clears_the_abandonment_clock(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);
        let broadcast = repo
            .create(new_broadcast(ada(), "Room"))
            .await
            .expect("created");
        repo.mark_live(broadcast.id, at(5))
            .await
            .expect("go live")
            .expect("the row was inactive");

        for (id, role) in [
            (ada(), ParticipantRole::Host),
            (grace(), ParticipantRole::Participant),
            (adam(), ParticipantRole::Participant),
        ] {
            repo.join(JoinRequest {
                broadcast_id: broadcast.id,
                participant_id: id,
                role,
            })
            .await
            .expect("the join runs");
        }
        assert_eq!(
            repo.live_participant_count(broadcast.id)
                .await
                .expect("the count runs"),
            3
        );

        repo.set_abandoned_since(broadcast.id, Some(at(20)))
            .await
            .expect("the clock is written");
        let reconnecting = repo
            .find_by_id(broadcast.id)
            .await
            .expect("the read runs")
            .expect("the row is there");
        assert_eq!(reconnecting.state(), BroadcastState::Reconnecting);
        assert_eq!(
            repo.list_live().await.expect("the sweep read runs")[0].abandoned_since,
            Some(at(20)),
            "the clock lives in the row, so a restart cannot lose it"
        );

        assert_eq!(
            repo.close_participants(broadcast.id, at(60))
                .await
                .expect("the close runs"),
            3
        );
        assert_eq!(
            repo.live_participant_count(broadcast.id)
                .await
                .expect("the count runs"),
            0
        );

        let ended = repo
            .mark_ended(broadcast.id, EndReason::Normal, at(60))
            .await
            .expect("the end runs")
            .expect("a live row ends once");
        assert_eq!(ended.status, BroadcastStatus::Inactive);
        assert_eq!(ended.end_time, Some(at(60)));
        assert_eq!(ended.end_reason, Some(EndReason::Normal));
        assert_eq!(
            ended.abandoned_since, None,
            "an ended row must not keep a clock the reaper would act on"
        );
        assert_eq!(ended.state(), BroadcastState::Ended);

        assert!(
            repo.mark_ended(broadcast.id, EndReason::AdminForced, at(61))
                .await
                .expect("the end runs")
                .is_none(),
            "a second end must not rewrite the first reason"
        );
        assert!(
            repo.find_live_by_creator(ada())
                .await
                .expect("the read runs")
                .is_none()
        );
        assert!(
            repo.list_live()
                .await
                .expect("the sweep read runs")
                .is_empty()
        );
    });
}

// ── cohost invitations ─────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn an_invitation_is_reissued_pending_and_only_a_pending_row_can_answer(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);
        let broadcast = repo
            .create(new_broadcast(ada(), "Room"))
            .await
            .expect("created");

        let invitation = repo
            .create_invitation(broadcast.id, ada(), grace())
            .await
            .expect("the invitation is created");
        assert_eq!(invitation.status, InvitationStatus::Pending);
        assert_eq!(invitation.responded_at, None);

        let accepted = repo
            .respond_invitation(invitation.id, InvitationStatus::Accepted, at(30))
            .await
            .expect("the answer runs")
            .expect("a pending row can be answered");
        assert_eq!(accepted.status, InvitationStatus::Accepted);
        assert_eq!(accepted.responded_at, Some(at(30)));

        assert!(
            repo.respond_invitation(invitation.id, InvitationStatus::Declined, at(31))
                .await
                .expect("the answer runs")
                .is_none(),
            "an answered invitation is not answerable again"
        );

        // Re-inviting an answered invitee makes the handshake pending again
        // rather than violating the unique (broadcast, invitee) row.
        let reissued = repo
            .create_invitation(broadcast.id, ada(), grace())
            .await
            .expect("the re-invite runs");
        assert_eq!(reissued.id, invitation.id, "the row is reused");
        assert_eq!(reissued.status, InvitationStatus::Pending);
        assert_eq!(reissued.responded_at, None);

        let declined = repo
            .respond_invitation(reissued.id, InvitationStatus::Declined, at(40))
            .await
            .expect("the answer runs")
            .expect("the reissued row is pending");
        assert_eq!(declined.status, InvitationStatus::Declined);

        assert!(
            repo.find_invitation(uuid::Uuid::new_v4())
                .await
                .expect("the read runs")
                .is_none()
        );
    });
}

// ── scheduled reminders ────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_due_sweep_lists_started_rows_and_claims_each_reminder_once(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);

        let due = repo
            .create(NewBroadcast {
                start_time: Some(at(-60)),
                ..new_broadcast(ada(), "Already started")
            })
            .await
            .expect("created");
        let future = repo
            .create(NewBroadcast {
                start_time: Some(at(3_600)),
                ..new_broadcast(grace(), "Later")
            })
            .await
            .expect("created");
        let unscheduled = repo
            .create(new_broadcast(adam(), "Whenever"))
            .await
            .expect("created");

        let rows = repo
            .list_due_scheduled(at(0))
            .await
            .expect("the sweep read runs");
        assert_eq!(
            rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![due.id],
            "only the started, not-yet-notified row is due; a future or unscheduled row is not"
        );

        assert!(
            repo.mark_scheduled_notified(due.id, at(0))
                .await
                .expect("the claim runs"),
            "the first claim sends the reminder"
        );
        assert!(
            !repo
                .mark_scheduled_notified(due.id, at(1))
                .await
                .expect("the claim runs"),
            "a worker restart must not remind the same subscribers again"
        );
        assert!(
            repo.list_due_scheduled(at(1))
                .await
                .expect("the sweep read runs")
                .is_empty()
        );

        assert!(
            repo.find_by_id(future.id)
                .await
                .expect("the read runs")
                .is_some()
        );
        assert!(
            repo.find_by_id(unscheduled.id)
                .await
                .expect("the read runs")
                .is_some()
        );
    });
}

// ── lists, search and the keyset cursor ────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_search_is_public_and_a_draft_is_never_a_result(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);

        let live = repo
            .create(new_broadcast(ada(), "Live launch"))
            .await
            .expect("created");
        repo.mark_live(live.id, at(10)).await.expect("go live");

        let finished = repo
            .create(new_broadcast(grace(), "Past launch"))
            .await
            .expect("created");
        repo.mark_live(finished.id, at(11)).await.expect("go live");
        repo.mark_ended(finished.id, EndReason::Normal, at(12))
            .await
            .expect("end");

        let draft = repo
            .create(new_broadcast(adam(), "Draft launch"))
            .await
            .expect("created");

        let results = repo
            .list(BroadcastFilter {
                q: Some("launch".to_owned()),
                ..page()
            })
            .await
            .expect("the search runs");

        let ids: Vec<Uuid> = results.iter().map(|row| row.id).collect();
        assert!(ids.contains(&live.id), "a live match is public");
        assert!(ids.contains(&finished.id), "an ended match is public");
        assert!(
            !ids.contains(&draft.id),
            "a draft is not something a stranger may find by typing at it"
        );

        let by_creator = repo
            .list(BroadcastFilter {
                creator_id: Some(ada()),
                ..page()
            })
            .await
            .expect("the list runs");
        assert_eq!(by_creator.len(), 1);
        assert_eq!(by_creator[0].id, live.id);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_keyset_cursor_pages_newest_first_without_repeating_or_skipping(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);

        let mut created = Vec::new();
        for i in 0..5i64 {
            let row = repo
                .create(new_broadcast(ada(), &format!("Show {i}")))
                .await
                .expect("created");
            // Pin a known order: `create` uses `now()`, and five rows in one test
            // can share a millisecond, which would make the cursor's boundary
            // ambiguous rather than testing it.
            sqlx::query("UPDATE broadcasts SET created_at = $2 WHERE id = $1")
                .bind(row.id)
                .bind(at(i))
                .execute(&pool)
                .await
                .expect("the fixture pins the order");
            created.push(row.id);
        }

        let mut seen = Vec::new();
        let mut cursor = None;
        let mut pages = 0;
        loop {
            let rows = repo
                .list(BroadcastFilter {
                    limit_plus_one: 3,
                    cursor,
                    ..page()
                })
                .await
                .expect("a page of the walk");
            pages += 1;
            let page_rows: Vec<_> = rows.iter().take(2).collect();
            cursor = page_rows.last().map(|row| (row.created_at, row.id));
            seen.extend(page_rows.iter().map(|row| row.id));
            if rows.len() < 3 {
                break;
            }
            assert!(pages < 10, "the walk must terminate");
        }

        created.reverse();
        assert_eq!(seen, created, "newest first, every row once");
    });
}

// ── the clock, and the interval that clears it ─────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_abandonment_clock_is_cleared_when_the_host_returns(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = repo(&pool);
        let broadcast = repo
            .create(new_broadcast(ada(), "Room"))
            .await
            .expect("created");
        repo.mark_live(broadcast.id, at(5)).await.expect("go live");

        repo.set_abandoned_since(broadcast.id, Some(at(20)))
            .await
            .expect("the clock is written");
        assert_eq!(
            repo.find_by_id(broadcast.id)
                .await
                .expect("the read runs")
                .expect("the row is there")
                .state(),
            BroadcastState::Reconnecting
        );

        repo.set_abandoned_since(broadcast.id, None)
            .await
            .expect("the clock is cleared");
        let healthy = repo
            .find_by_id(broadcast.id)
            .await
            .expect("the read runs")
            .expect("the row is there");
        assert_eq!(healthy.state(), BroadcastState::Live);

        // A stale sweep cannot write a clock onto a row somebody ended.
        repo.mark_ended(broadcast.id, EndReason::Normal, at(30))
            .await
            .expect("the end runs");
        repo.set_abandoned_since(broadcast.id, Some(at(31)))
            .await
            .expect("the write is a no-op");
        assert_eq!(
            repo.find_by_id(broadcast.id)
                .await
                .expect("the read runs")
                .expect("the row is there")
                .abandoned_since,
            None
        );
    });
}
