//! §10 **Integration** layer: `PgModerationRepo` against a real Postgres.
//!
//! # Why this file exists and why it needs a database
//!
//! `moderation_service.rs` runs against the in-memory double, which mirrors the
//! statements' *semantics* and can therefore prove the rules — but cannot prove the SQL.
//! Four things are assertable nowhere else:
//!
//! - the **`WHERE EXISTS` guard on `block`**, which is what makes an unknown account a
//!   `false` rather than a foreign-key violation;
//! - **`ON CONFLICT DO NOTHING`**, which is what makes a repeated block one row;
//! - the **migration's CHECK constraints** on `subject_type`, `status` and the
//!   `resolved_at`/`status` pairing, which a double models by construction and therefore
//!   cannot get wrong;
//! - the **`is_blocked_by` / `creator_blocked` joins** other modules read the table with,
//!   which a double that reasons about tuples cannot exercise at all.
//!
//! Run it with:
//!
//! ```text
//! cargo test -p meno-api --features test-support --test moderation_repository_pg
//! ```
//!
//! against a `DATABASE_URL` pointing at an empty Postgres. `with_pool!` skips rather than
//! fails when `DATABASE_URL` is absent — a contributor with no database must still get a
//! green `cargo test`.

// The three panicking lints are denied workspace-wide but explicitly *allowed in
// tests* — see `clippy.toml`. The allow-*-in-tests keys cover `#[test]` functions; the
// helpers below are not `#[test]` functions, so the allowance does not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use meno_api::modules::moderation::model::{NewReport, ReportStatus, ReportSubject};
use meno_api::modules::moderation::repository::{ModerationRepo, PgModerationRepo, ReportFilter};
use meno_api::modules::subscribers::repository::{PgSubscribersRepo, SubscribersRepo};
use sqlx::PgPool;
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

/// Insert a broadcast owned by `creator_id`.
async fn broadcast(pool: &PgPool, id: Uuid, creator_id: Uuid) {
    sqlx::query(
        "INSERT INTO broadcasts (id, title, creator_id, status, created_at) \
         VALUES ($1, 'A broadcast', $2, 'inactive', now())",
    )
    .bind(id)
    .bind(creator_id)
    .execute(pool)
    .await
    .expect("the fixture broadcast is inserted");
}

fn ada() -> Uuid {
    Uuid::from_u128(1)
}

fn grace() -> Uuid {
    Uuid::from_u128(2)
}

fn room() -> Uuid {
    Uuid::from_u128(0x100)
}

// ── blocks ─────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_block_is_one_row_and_an_unknown_account_inserts_nothing(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "Ada Lovelace", "ada@example.test").await;
        user(&pool, grace(), "Grace Hopper", "grace@example.test").await;
        let repo = PgModerationRepo::new(pool.clone());

        assert!(
            repo.block(ada(), grace()).await.expect("the block runs"),
            "the first block inserts a row"
        );
        assert!(
            !repo.block(ada(), grace()).await.expect("a repeat runs"),
            "ON CONFLICT DO NOTHING makes a repeat one row"
        );
        assert!(
            !repo
                .block(ada(), Uuid::from_u128(0x99))
                .await
                .expect("an unknown id runs"),
            "the WHERE EXISTS inserts nothing rather than raising the foreign key"
        );

        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_blocks")
            .fetch_one(&pool)
            .await
            .expect("the count runs");
        assert_eq!(rows, 1);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_block_list_is_the_blockers_own_and_names_the_account(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "Ada Lovelace", "ada@example.test").await;
        user(&pool, grace(), "Grace Hopper", "grace@example.test").await;
        let repo = PgModerationRepo::new(pool.clone());

        repo.block(ada(), grace()).await.expect("the block runs");

        let list = repo.blocked(ada()).await.expect("the list is read");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, grace());
        assert_eq!(list[0].full_name, "Grace Hopper");

        assert!(
            repo.blocked(grace())
                .await
                .expect("the list is read")
                .is_empty(),
            "a block is one-directional"
        );

        assert!(repo.unblock(ada(), grace()).await.expect("the delete runs"));
        assert!(
            !repo
                .unblock(ada(), grace())
                .await
                .expect("a second delete runs"),
            "removing nothing reports nothing"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_follow_and_chat_joins_read_the_same_direction(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "Ada Lovelace", "ada@example.test").await;
        user(&pool, grace(), "Grace Hopper", "grace@example.test").await;
        broadcast(&pool, room(), grace()).await;

        PgModerationRepo::new(pool.clone())
            .block(grace(), ada())
            .await
            .expect("Grace blocks Ada");

        let subscribers = PgSubscribersRepo::new(pool.clone());
        assert!(
            subscribers
                .is_blocked_by(grace(), ada())
                .await
                .expect("the follow check runs"),
            "the account being followed is the blocker"
        );
        assert!(
            !subscribers
                .is_blocked_by(ada(), grace())
                .await
                .expect("the reversed check runs"),
            "the other direction is untouched"
        );
    });
}

// ── reports ────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_report_round_trips_and_a_resolution_stamps_it(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "Ada Lovelace", "ada@example.test").await;
        user(&pool, grace(), "Grace Hopper", "grace@example.test").await;
        broadcast(&pool, room(), grace()).await;
        let repo = PgModerationRepo::new(pool.clone());

        let report = repo
            .create_report(NewReport {
                reporter_id: ada(),
                subject_type: ReportSubject::User,
                subject_id: grace(),
                reason: "spam",
                details: None,
            })
            .await
            .expect("the insert runs");

        assert_eq!(report.status, ReportStatus::Open);
        assert_eq!(report.resolved_at, None);
        assert_eq!(report.subject_type, ReportSubject::User);

        let read = repo
            .report(report.id)
            .await
            .expect("the read runs")
            .expect("the row is there");
        assert_eq!(read, report);

        assert!(
            repo.resolve_report(report.id, ReportStatus::Reviewed)
                .await
                .expect("the update runs")
        );
        let resolved = repo
            .report(report.id)
            .await
            .expect("the read runs")
            .expect("the row is still there");
        assert_eq!(resolved.status, ReportStatus::Reviewed);
        assert!(resolved.resolved_at.is_some());

        assert!(
            !repo
                .resolve_report(Uuid::from_u128(0x99), ReportStatus::Dismissed)
                .await
                .expect("the update runs"),
            "an unknown id updates nothing"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_queue_filters_and_orders_newest_first(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "Ada Lovelace", "ada@example.test").await;
        user(&pool, grace(), "Grace Hopper", "grace@example.test").await;
        broadcast(&pool, room(), grace()).await;
        let repo = PgModerationRepo::new(pool.clone());

        let first = repo
            .create_report(NewReport {
                reporter_id: ada(),
                subject_type: ReportSubject::User,
                subject_id: grace(),
                reason: "spam",
                details: Some("repeated links"),
            })
            .await
            .expect("the first insert runs");
        let second = repo
            .create_report(NewReport {
                reporter_id: ada(),
                subject_type: ReportSubject::Broadcast,
                subject_id: room(),
                reason: "harassment",
                details: None,
            })
            .await
            .expect("the second insert runs");

        repo.resolve_report(first.id, ReportStatus::Dismissed)
            .await
            .expect("the first report is resolved");

        let open = repo
            .reports(ReportFilter {
                status: Some(ReportStatus::Open),
                limit_plus_one: 11,
                cursor: None,
            })
            .await
            .expect("the filtered page reads");
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].id, second.id);

        let all = repo
            .reports(ReportFilter {
                status: None,
                limit_plus_one: 11,
                cursor: None,
            })
            .await
            .expect("the unfiltered page reads");
        assert_eq!(all.len(), 2);

        // A page of one, then the cursor it advertises: the second page is the other row.
        let page = repo
            .reports(ReportFilter {
                status: None,
                limit_plus_one: 2,
                cursor: None,
            })
            .await
            .expect("the first page reads");
        assert_eq!(page.len(), 2);
        assert_ne!(page[0].created_at, page[1].created_at);
        let next = repo
            .reports(ReportFilter {
                status: None,
                limit_plus_one: 1,
                cursor: Some((page[0].created_at, page[0].id)),
            })
            .await
            .expect("the second page reads");
        assert!(
            !next.iter().any(|report| report.id == page[0].id),
            "the cursor is exclusive"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_deleted_broadcast_can_still_be_reported(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, grace(), "Grace Hopper", "grace@example.test").await;
        broadcast(&pool, room(), grace()).await;
        sqlx::query("UPDATE broadcasts SET deleted_at = now() WHERE id = $1")
            .bind(room())
            .execute(&pool)
            .await
            .expect("the broadcast is soft-deleted");

        let repo = PgModerationRepo::new(pool.clone());
        assert!(
            repo.broadcast_exists(room())
                .await
                .expect("the existence check runs"),
            "a report about a broadcast that has since been removed is still evidence"
        );
        assert!(
            !repo
                .broadcast_exists(Uuid::from_u128(0x99))
                .await
                .expect("the existence check runs")
        );
    });
}
