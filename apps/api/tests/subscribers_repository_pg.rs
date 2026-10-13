//! §10 **Integration** layer: `PgSubscribersRepo` against a real Postgres.
//!
//! # Why this file exists and why it needs a database
//!
//! `subscribers_service.rs` runs against the in-memory double, which mirrors the
//! statements' *semantics* and can therefore prove the rules — but cannot prove the SQL.
//! Only Postgres reads the migrations, and only this suite executes the statements in
//! `repository/pg.rs`. Four things are assertable nowhere else:
//!
//! - the **counter trigger** (migration `0009`), which is what makes `users.followers` and
//!   `users.following` mean anything on `/users/{id}`. The plan lists this assertion as the
//!   one that would have caught `master`'s dead trigger; here it also pins the *other* half
//!   — that a repeated follow does not count twice;
//! - the **`deleted_at IS NULL` predicates**, in the join and in the guards, which a double
//!   models as absence and therefore cannot get wrong;
//! - the **column choice per direction** — a page that filters on the wrong side of the
//!   join returns an empty list, which looks exactly like a user nobody follows;
//! - the **keyset cursor**, which is a row-value comparison Postgres parses and could still
//!   answer wrongly.
//!
//! Run it with:
//!
//! ```text
//! cargo test -p meno-api --features test-support --test subscribers_repository_pg
//! ```
//!
//! against a `DATABASE_URL` pointing at an empty Postgres. `#[sqlx::test]` applies every
//! migration to a fresh per-test database and rolls it back, so the tests are independent
//! and need no cleanup. `with_pool!` skips rather than fails when `DATABASE_URL` is absent:
//! a contributor with no database must still get a green `cargo test`, and a suite that
//! fails on a missing Docker daemon is a suite people stop running.

// The three panicking lints are denied workspace-wide but explicitly *allowed in
// tests* — see `clippy.toml`. The allow-*-in-tests keys cover `#[test]` functions; the
// helpers below are not `#[test]` functions, so the allowance does not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use meno_api::modules::subscribers::dto::FollowDirection;
use meno_api::modules::subscribers::repository::{
    FollowFilter, PgSubscribersRepo, SubscribersRepo,
};
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

/// Insert a follow with a caller-chosen instant.
///
/// Written directly rather than through `create` so a page can be paginated against known
/// timestamps without a sleep, and so the trigger's own behaviour is not on the path of the
/// fixtures.
async fn followed(pool: &PgPool, follower: Uuid, target: Uuid, created_at: OffsetDateTime) {
    sqlx::query(
        "INSERT INTO user_subscribers (subscriber_id, subscription_id, created_at) \
         VALUES ($1, $2, $3)",
    )
    .bind(follower)
    .bind(target)
    .bind(created_at)
    .execute(pool)
    .await
    .expect("the follow is inserted");
}

/// Soft-delete an account.
async fn soft_delete(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("the account is soft-deleted");
}

/// `(followers, following)` as the trigger left them.
async fn counters(pool: &PgPool, id: Uuid) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>("SELECT followers, following FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("the counters are read")
}

/// How many follow rows the table holds — the direct answer, not a counter.
async fn follow_rows(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM user_subscribers")
        .fetch_one(pool)
        .await
        .expect("the count runs")
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

/// Three live accounts: Ada, Grace and Adam.
async fn accounts(pool: &PgPool) {
    user(pool, ada(), "Ada Lovelace", "ada@example.com").await;
    user(pool, grace(), "Grace Hopper", "grace@example.com").await;
    user(pool, adam(), "Adam Smith", "adam@example.com").await;
}

fn page(direction: FollowDirection, owner: Uuid, viewer: Uuid) -> FollowFilter {
    FollowFilter {
        owner_id: owner,
        viewer_id: viewer,
        direction,
        limit_plus_one: 21,
        cursor: None,
    }
}

fn ids(rows: &[meno_api::modules::subscribers::dto::SubscriberDto]) -> Vec<Uuid> {
    rows.iter().map(|row| row.id).collect()
}

// ── the writes, and the counters they move ─────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_follow_round_trips_and_the_trigger_moves_both_counters(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgSubscribersRepo::new(pool.clone());
        assert_eq!(counters(&pool, ada()).await, (0, 0));

        assert!(
            repo.create(grace(), ada()).await.expect("the insert runs"),
            "a first follow inserts a row"
        );

        assert_eq!(follow_rows(&pool).await, 1);
        assert_eq!(
            counters(&pool, ada()).await,
            (1, 0),
            "migration 0009's trigger increments the followee's followers"
        );
        assert_eq!(
            counters(&pool, grace()).await,
            (0, 1),
            "and the follower's following"
        );

        assert!(
            repo.remove(grace(), ada()).await.expect("the delete runs"),
            "the row was there to remove"
        );
        assert_eq!(follow_rows(&pool).await, 0);
        assert_eq!(
            counters(&pool, ada()).await,
            (0, 0),
            "the trigger decrements on delete"
        );
        assert_eq!(counters(&pool, grace()).await, (0, 0));
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_repeated_follow_inserts_nothing_and_leaves_the_counters_alone(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgSubscribersRepo::new(pool.clone());

        assert!(repo.create(grace(), ada()).await.expect("first"));
        assert!(
            !repo.create(grace(), ada()).await.expect("second"),
            "the conflict clause reports that nothing was inserted"
        );

        assert_eq!(follow_rows(&pool).await, 1, "one row, not two");
        assert_eq!(
            counters(&pool, ada()).await,
            (1, 0),
            "a double tap must not double-count"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn following_a_user_who_does_not_exist_inserts_nothing(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgSubscribersRepo::new(pool.clone());

        assert!(
            !repo
                .create(grace(), unknown())
                .await
                .expect("the insert runs"),
            "the guard keeps the foreign key unreachable, so this is a false"
        );
        assert_eq!(follow_rows(&pool).await, 0);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn following_a_soft_deleted_user_inserts_nothing(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        soft_delete(&pool, adam()).await;
        let repo = PgSubscribersRepo::new(pool.clone());

        assert!(!repo.create(grace(), adam()).await.expect("the insert runs"));
        assert_eq!(follow_rows(&pool).await, 0);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn removing_a_follow_that_is_not_there_reports_nothing_removed(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgSubscribersRepo::new(pool.clone());

        assert!(!repo.remove(grace(), ada()).await.expect("the delete runs"));
        assert_eq!(counters(&pool, ada()).await, (0, 0), "no counter moved");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn user_exists_sees_only_live_accounts(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        soft_delete(&pool, adam()).await;
        let repo = PgSubscribersRepo::new(pool.clone());

        assert!(repo.user_exists(ada()).await.expect("the query runs"));
        assert!(
            !repo.user_exists(adam()).await.expect("the query runs"),
            "a soft-deleted account is not somebody to follow"
        );
        assert!(!repo.user_exists(unknown()).await.expect("the query runs"));
    });
}

// ── the two pages ──────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_followers_page_reads_newest_first_and_annotates_for_the_viewer(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgSubscribersRepo::new(pool.clone());
        // Ada and Adam both follow Grace; Grace follows Ada; Adam follows Ada too.
        followed(&pool, ada(), grace(), at(10)).await;
        followed(&pool, adam(), grace(), at(20)).await;
        followed(&pool, grace(), ada(), at(30)).await;
        followed(&pool, adam(), ada(), at(40)).await;

        let as_grace = repo
            .find_follows(&page(FollowDirection::Followers, grace(), grace()))
            .await
            .expect("the page is read");

        assert_eq!(
            ids(&as_grace),
            vec![adam(), ada()],
            "the follower who arrived last is first"
        );
        assert_eq!(as_grace[0].full_name, "Adam Smith");
        assert!(
            !as_grace[0].is_following,
            "Grace does not follow Adam, so the flag is the viewer's"
        );
        assert!(as_grace[1].is_following, "Grace follows Ada");

        // The same page, read by a different viewer, annotates differently — including for
        // the row that describes the viewer themself.
        let as_adam = repo
            .find_follows(&page(FollowDirection::Followers, grace(), adam()))
            .await
            .expect("the page is read");
        assert_eq!(ids(&as_adam), vec![adam(), ada()]);
        assert!(!as_adam[0].is_following, "Adam does not follow himself");
        assert!(as_adam[1].is_following, "Adam follows Ada");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_following_page_reads_the_other_side_of_the_join(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgSubscribersRepo::new(pool.clone());
        followed(&pool, grace(), ada(), at(10)).await;
        followed(&pool, adam(), grace(), at(20)).await;
        followed(&pool, grace(), adam(), at(30)).await;

        let rows = repo
            .find_follows(&page(FollowDirection::Following, grace(), grace()))
            .await
            .expect("the page is read");

        assert_eq!(ids(&rows), vec![adam(), ada()], "newest follow first");
        assert_eq!(rows[0].full_name, "Adam Smith");
        assert_eq!(
            rows[0].subscribed_at,
            at(30),
            "the row's date is the follow's, not the account's"
        );
        assert!(
            rows.iter().all(|row| row.is_following),
            "every row of one's own following list is somebody one follows"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_keyset_cursor_pages_without_repeating_or_skipping_a_row(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        user(&pool, grace(), "Grace Hopper", "grace@example.com").await;
        let repo = PgSubscribersRepo::new(pool.clone());

        let mut followers = Vec::new();
        for i in 0..5u128 {
            let follower = Uuid::from_u128(0x10 + i);
            user(
                &pool,
                follower,
                "Follower",
                &format!("follower{i}@example.com"),
            )
            .await;
            let created_at = at(i64::try_from(i).expect("small"));
            followed(&pool, follower, grace(), created_at).await;
            followers.push(follower);
        }

        // Two pages of two, then the remainder — each cursor built from the row the previous
        // page ended on, exactly as the service does.
        let mut seen: Vec<Uuid> = Vec::new();
        let mut cursor: Option<(OffsetDateTime, Uuid)> = None;
        // Two full pages (each with its one probe row) and a short last page.
        for (page_size, fetched) in [(2usize, 3usize), (2, 3), (1, 1)] {
            let rows = repo
                .find_follows(&FollowFilter {
                    limit_plus_one: 3,
                    cursor,
                    ..page(FollowDirection::Followers, grace(), grace())
                })
                .await
                .expect("a page of the walk");

            assert_eq!(rows.len(), fetched, "the probe row is included");
            let page_rows = &rows[..page_size];
            cursor = page_rows.last().map(|row| (row.subscribed_at, row.id));
            seen.extend(ids(page_rows));
        }

        assert_eq!(seen.len(), 5, "no row was repeated: {seen:?}");
        let unique: std::collections::HashSet<Uuid> = seen.iter().copied().collect();
        assert_eq!(unique.len(), 5, "no row was skipped: {seen:?}");
        followers.reverse();
        assert_eq!(seen, followers, "newest first, all the way down");
    });
}

// ── deleted accounts ──────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_soft_deleted_user_is_absent_from_a_list(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgSubscribersRepo::new(pool.clone());
        followed(&pool, grace(), ada(), at(10)).await;
        followed(&pool, adam(), ada(), at(20)).await;

        soft_delete(&pool, adam()).await;

        let rows = repo
            .find_follows(&page(FollowDirection::Followers, ada(), ada()))
            .await
            .expect("the page is read");

        assert_eq!(
            ids(&rows),
            vec![grace()],
            "the join drops the deleted follower rather than returning a ghost"
        );
    });
}
