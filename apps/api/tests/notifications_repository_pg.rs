//! §10 **Integration** layer: `PgNotificationsRepo` against a real Postgres.
//!
//! # Why this file exists and why it needs a database
//!
//! `notifications_service.rs` runs against the in-memory double, which mirrors the
//! statements' *semantics* and can therefore prove the rules — but cannot prove the SQL.
//! Only Postgres reads the migrations, and only this suite executes the statements in
//! `repository/pg.rs`. Five things are assertable nowhere else:
//!
//! - the **template predicate** (`is_active = true AND deleted_at IS NULL`), which is what
//!   makes deactivating a template stop notifications rather than merely hide one;
//! - the **joins** that render a sentence — `notification_templates` for the text,
//!   `broadcasts` for `{title}`, and `users … AND deleted_at IS NULL` for `{actor}`;
//! - the **keyset cursor**, which is a row-value comparison Postgres parses and could
//!   still answer wrongly;
//! - the **`archived_at IS NULL` predicates**, in the page, the guards and the count;
//! - the **settings reads** the push adapter depends on: a token is offered only when push
//!   is switched on, and a fan-out selects only users who did.
//!
//! Run it with:
//!
//! ```text
//! cargo test -p meno-api --features test-support --test notifications_repository_pg
//! ```
//!
//! against a `DATABASE_URL` pointing at an empty Postgres. `#[sqlx::test]` applies every
//! migration to a fresh per-test database and rolls it back, so the tests are independent
//! and need no cleanup. `with_pool!` skips rather than fails when `DATABASE_URL` is absent:
//! a contributor with no database must still get a green `cargo test`, and a suite that
//! fails on a missing Docker daemon is a suite people stop running.

// The three panicking lints are denied workspace-wide but explicitly *allowed in tests* —
// see `clippy.toml`.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use meno_api::modules::notifications::model::{NotificationTemplate, codes};
use meno_api::modules::notifications::repository::{
    NotificationFilter, NotificationsRepo, PgNotificationsRepo,
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

fn ada() -> Uuid {
    Uuid::from_u128(1)
}

fn grace() -> Uuid {
    Uuid::from_u128(2)
}

fn adam() -> Uuid {
    Uuid::from_u128(3)
}

fn broadcast() -> Uuid {
    Uuid::from_u128(0x100)
}

fn unknown() -> Uuid {
    Uuid::from_u128(0xff)
}

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds).expect("a plausible instant")
}

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

/// Remove the five templates migration `0012` seeds.
///
/// A test that counts templates or asserts a lookup answers `None` needs the table to
/// contain only its own fixtures: the seed is real, and `find_template_by_code` would
/// otherwise fall back to it.
async fn clear_templates(pool: &PgPool) {
    sqlx::query("DELETE FROM notification_templates")
        .execute(pool)
        .await
        .expect("the seeded templates are cleared");
}

/// Insert an active template for `type_code` and return its id.
///
/// Written directly rather than through a repository method because the module has no
/// `create_template`: templates are seeded by migration `0012` and edited by an operator.
async fn template(pool: &PgPool, type_code: &str, title: &str, body: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO notification_templates (type, title, body) VALUES ($1, $2, $3) \
         RETURNING id",
    )
    .bind(type_code)
    .bind(title)
    .bind(body)
    .fetch_one(pool)
    .await
    .expect("the fixture template is inserted")
}

/// The seeded template for a code, as migration `0012` creates it.
async fn seeded_template(pool: &PgPool, type_code: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM notification_templates WHERE type = $1 AND deleted_at IS NULL LIMIT 1",
    )
    .bind(type_code)
    .fetch_one(pool)
    .await
    .expect("migration 0012 seeds this template")
}

/// Insert a broadcast owned by `creator`.
async fn broadcast_with(pool: &PgPool, id: Uuid, creator: Uuid, title: &str) {
    sqlx::query("INSERT INTO broadcasts (id, title, creator_id) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(title)
        .bind(creator)
        .execute(pool)
        .await
        .expect("the fixture broadcast is inserted");
}

/// Insert a notification with a caller-chosen instant.
///
/// Written directly so a page can be paginated against known timestamps without a sleep,
/// and so a row exists without going through the statement under test.
#[allow(clippy::too_many_arguments)]
async fn notification(
    pool: &PgPool,
    id: Uuid,
    owner_id: Uuid,
    template_id: Uuid,
    actor_id: Option<Uuid>,
    broadcast_id: Option<Uuid>,
    created_at: OffsetDateTime,
    read: bool,
) {
    sqlx::query(
        "INSERT INTO notifications (id, owner_id, template_id, actor_id, broadcast_id, read, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(owner_id)
    .bind(template_id)
    .bind(actor_id)
    .bind(broadcast_id)
    .bind(read)
    .bind(created_at)
    .execute(pool)
    .await
    .expect("the fixture notification is inserted");
}

/// Soft-delete an account.
async fn soft_delete(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("the account is soft-deleted");
}

/// Archive a notification, as an operator's `UPDATE` does.
async fn archive(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE notifications SET archived_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("the row is archived");
}

/// Insert a settings row, with a device token when one is given.
async fn settings(
    pool: &PgPool,
    user_id: Uuid,
    push_notifications: bool,
    app_notifications: bool,
    token: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO settings \
         (user_id, push_notifications, app_notifications, push_notification_token) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(push_notifications)
    .bind(app_notifications)
    .bind(token)
    .execute(pool)
    .await
    .expect("the fixture settings row is inserted");
}

/// Three live accounts.
async fn accounts(pool: &PgPool) {
    user(pool, ada(), "Ada Lovelace", "ada@example.com").await;
    user(pool, grace(), "Grace Hopper", "grace@example.com").await;
    user(pool, adam(), "Adam Smith", "adam@example.com").await;
}

fn page(owner: Uuid, unread_only: bool) -> NotificationFilter {
    NotificationFilter {
        owner_id: owner,
        unread_only,
        limit_plus_one: 21,
        cursor: None,
    }
}

fn ids(rows: &[meno_api::modules::notifications::model::NotificationRow]) -> Vec<Uuid> {
    rows.iter().map(|row| row.id).collect()
}

// ── templates ─────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn an_active_template_is_found_and_a_deactivated_one_is_not(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        clear_templates(&pool).await;
        let repo = PgNotificationsRepo::new(pool.clone());
        let id = template(
            &pool,
            codes::USER_SUBSCRIBED,
            "New Follower",
            "{actor} follows you",
        )
        .await;

        let found = repo
            .find_template_by_code(codes::USER_SUBSCRIBED)
            .await
            .expect("the query runs")
            .expect("the template is active");

        assert_eq!(found.id, id);
        assert_eq!(found.type_code, codes::USER_SUBSCRIBED);
        assert_eq!(found.body, "{actor} follows you");

        sqlx::query("UPDATE notification_templates SET is_active = false WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .expect("the template is deactivated");

        assert!(
            repo.find_template_by_code(codes::USER_SUBSCRIBED)
                .await
                .expect("the query runs")
                .is_none(),
            "a deactivated template must stop rendering notifications"
        );

        sqlx::query(
            "UPDATE notification_templates SET is_active = true, deleted_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .expect("the template is soft-deleted");

        assert!(
            repo.find_template_by_code(codes::USER_SUBSCRIBED)
                .await
                .expect("the query runs")
                .is_none(),
            "a soft-deleted template is gone too, active flag or not"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn every_active_template_is_listed_for_the_cache(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        clear_templates(&pool).await;
        let repo = PgNotificationsRepo::new(pool.clone());
        let follower = template(
            &pool,
            codes::USER_SUBSCRIBED,
            "New Follower",
            "{actor} started following you",
        )
        .await;
        let live = template(
            &pool,
            codes::LIVE_BROADCAST_STARTED,
            "Live Now",
            "{actor} is live: {title}",
        )
        .await;
        let retired = template(&pool, codes::BROADCAST_ENDED, "Ended", "{title} has ended").await;
        sqlx::query("UPDATE notification_templates SET is_active = false WHERE id = $1")
            .bind(retired)
            .execute(&pool)
            .await
            .expect("the template is deactivated");

        let all: Vec<NotificationTemplate> =
            repo.find_all_templates().await.expect("the query runs");

        let mut found: Vec<Uuid> = all.iter().map(|template| template.id).collect();
        found.sort();
        let mut expected = vec![follower, live];
        expected.sort();
        assert_eq!(found, expected, "an inactive template is not cached");
    });
}

// ── the page ───────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_page_reads_newest_first_with_the_sentence_inputs_joined(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        broadcast_with(&pool, broadcast(), grace(), "Morning Notes").await;
        let template_id = seeded_template(&pool, codes::LIVE_BROADCAST_STARTED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        let oldest = Uuid::from_u128(0x11);
        let newest = Uuid::from_u128(0x12);
        notification(&pool, oldest, ada(), template_id, None, None, at(0), false).await;
        notification(
            &pool,
            newest,
            ada(),
            template_id,
            Some(grace()),
            Some(broadcast()),
            at(60),
            false,
        )
        .await;

        let rows = repo
            .find_page(&page(ada(), false))
            .await
            .expect("the page reads");

        assert_eq!(ids(&rows), vec![newest, oldest], "newest first");
        let row = &rows[0];
        assert_eq!(row.type_code, codes::LIVE_BROADCAST_STARTED);
        assert_eq!(row.title_template, "Live Now");
        assert_eq!(
            row.body_template, "{actor} is live: {title}",
            "rendering is the service's job, so the template text arrives intact"
        );
        assert_eq!(
            row.broadcast_title.as_deref(),
            Some("Morning Notes"),
            "`{{title}}` needs the broadcast join on the list path"
        );
        assert_eq!(row.actor_name.as_deref(), Some("Grace Hopper"));

        let actor = row.actor().expect("a live account is an actor");
        assert_eq!(actor.id, grace());
        assert_eq!(actor.full_name, "Grace Hopper");
        assert!(
            rows[1].actor().is_none(),
            "a system notification has no actor"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_cursor_excludes_the_row_it_points_at(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        let oldest = Uuid::from_u128(0x11);
        let newest = Uuid::from_u128(0x12);
        notification(&pool, oldest, ada(), template_id, None, None, at(0), false).await;
        notification(&pool, newest, ada(), template_id, None, None, at(60), false).await;

        let rows = repo
            .find_page(&NotificationFilter {
                owner_id: ada(),
                unread_only: false,
                limit_plus_one: 21,
                cursor: Some((at(60), newest)),
            })
            .await
            .expect("the page reads");

        assert_eq!(ids(&rows), vec![oldest], "the cursor row is excluded");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_unread_filter_excludes_read_rows(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        let read = Uuid::from_u128(0x11);
        let unread = Uuid::from_u128(0x12);
        notification(&pool, read, ada(), template_id, None, None, at(0), true).await;
        notification(&pool, unread, ada(), template_id, None, None, at(60), false).await;

        let rows = repo
            .find_page(&page(ada(), true))
            .await
            .expect("the page reads");

        assert_eq!(ids(&rows), vec![unread]);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn an_archived_row_leaves_the_page(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        let archived = Uuid::from_u128(0x11);
        notification(
            &pool,
            archived,
            ada(),
            template_id,
            None,
            None,
            at(0),
            false,
        )
        .await;
        notification(
            &pool,
            Uuid::from_u128(0x12),
            ada(),
            template_id,
            None,
            None,
            at(60),
            false,
        )
        .await;
        archive(&pool, archived).await;

        let rows = repo
            .find_page(&page(ada(), false))
            .await
            .expect("the page reads");

        assert_eq!(rows.len(), 1);
        assert!(
            !ids(&rows).contains(&archived),
            "an archived row is retired"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_page_is_scoped_to_its_owner(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        notification(
            &pool,
            Uuid::from_u128(0x11),
            grace(),
            template_id,
            None,
            None,
            at(0),
            false,
        )
        .await;

        let rows = repo
            .find_page(&page(ada(), false))
            .await
            .expect("the page reads");

        assert!(rows.is_empty(), "somebody else's inbox is not yours");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_deleted_actor_reads_as_no_actor(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        notification(
            &pool,
            Uuid::from_u128(0x11),
            ada(),
            template_id,
            Some(grace()),
            None,
            at(0),
            false,
        )
        .await;
        soft_delete(&pool, grace()).await;

        let rows = repo
            .find_page(&page(ada(), false))
            .await
            .expect("the page reads");

        assert!(
            rows[0].actor().is_none(),
            "the join drops a soft-deleted account, so the row is a system notification — \
             an actor with an empty name would render `\" started following you\"`"
        );
    });
}

// ── the writes ─────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn create_bulk_writes_one_row_per_owner(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::LIVE_BROADCAST_STARTED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        broadcast_with(&pool, broadcast(), adam(), "Morning Notes").await;

        let inserted = repo
            .create_bulk(
                &[ada(), grace()],
                template_id,
                Some(adam()),
                Some(broadcast()),
            )
            .await
            .expect("the insert runs");

        assert_eq!(inserted, 2);
        assert_eq!(
            repo.count_unread(ada()).await.expect("the count"),
            1,
            "one row for the first recipient"
        );
        assert_eq!(repo.count_unread(grace()).await.expect("the count"), 1);
        assert_eq!(repo.count_unread(adam()).await.expect("the count"), 0);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn create_bulk_with_no_owners_writes_nothing(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        assert_eq!(
            repo.create_bulk(&[], template_id, None, None)
                .await
                .expect("the no-op succeeds"),
            0
        );
        assert_eq!(repo.count_unread(ada()).await.expect("the count"), 0);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn create_stores_one_row_and_answers_its_id(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        let id = repo
            .create(ada(), template_id, Some(grace()), None)
            .await
            .expect("the insert runs");

        assert!(repo.exists(id, ada()).await.expect("the guard runs"));
        assert!(
            !repo.exists(id, grace()).await.expect("the guard runs"),
            "the row belongs to its owner"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn mark_read_transitions_once_and_is_owner_scoped(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        let id = repo
            .create(ada(), template_id, None, None)
            .await
            .expect("the insert runs");

        assert!(
            !repo.mark_read(id, grace()).await.expect("the update runs"),
            "somebody else's mark changes nothing"
        );
        assert!(repo.is_unread(id, ada()).await.expect("the read runs"));
        assert!(
            repo.mark_read(id, ada()).await.expect("the update runs"),
            "the owner's first mark moves the row"
        );
        assert!(
            !repo.mark_read(id, ada()).await.expect("the update runs"),
            "a second mark changes nothing"
        );
        assert!(!repo.is_unread(id, ada()).await.expect("the read runs"));
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn mark_all_read_counts_only_the_callers_unread_rows(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        repo.create(ada(), template_id, None, None)
            .await
            .expect("the insert runs");
        repo.create(ada(), template_id, None, None)
            .await
            .expect("the insert runs");
        repo.create(grace(), template_id, None, None)
            .await
            .expect("the insert runs");
        repo.create(ada(), template_id, None, None)
            .await
            .expect("the insert runs");
        let already_read = repo
            .create(ada(), template_id, None, None)
            .await
            .expect("the insert runs");
        repo.mark_read(already_read, ada())
            .await
            .expect("the update runs");

        assert_eq!(
            repo.mark_all_read(ada()).await.expect("the update runs"),
            3,
            "the already-read row is not counted again"
        );
        assert_eq!(repo.count_unread(ada()).await.expect("the count"), 0);
        assert_eq!(
            repo.count_unread(grace()).await.expect("the count"),
            1,
            "nobody else's inbox is touched"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn delete_is_owner_scoped_and_hard(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        let mine = repo
            .create(ada(), template_id, None, None)
            .await
            .expect("the insert runs");
        let theirs = repo
            .create(grace(), template_id, None, None)
            .await
            .expect("the insert runs");

        assert!(
            !repo.delete(mine, grace()).await.expect("the delete runs"),
            "the delete is scoped to the owner"
        );
        assert!(repo.exists(mine, ada()).await.expect("the guard runs"));
        assert!(repo.delete(mine, ada()).await.expect("the delete runs"));
        assert!(
            !repo.exists(mine, ada()).await.expect("the guard runs"),
            "the row is gone, not archived"
        );
        assert!(repo.exists(theirs, grace()).await.expect("the guard runs"));
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn count_unread_ignores_read_and_archived_rows(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let template_id = seeded_template(&pool, codes::USER_SUBSCRIBED).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        notification(
            &pool,
            Uuid::from_u128(0x11),
            ada(),
            template_id,
            None,
            None,
            at(0),
            false,
        )
        .await;
        notification(
            &pool,
            Uuid::from_u128(0x12),
            ada(),
            template_id,
            None,
            None,
            at(1),
            true,
        )
        .await;
        notification(
            &pool,
            Uuid::from_u128(0x13),
            ada(),
            template_id,
            None,
            None,
            at(2),
            false,
        )
        .await;
        archive(&pool, Uuid::from_u128(0x13)).await;

        assert_eq!(
            repo.count_unread(ada()).await.expect("the count"),
            1,
            "a read row and an archived row are both outside the badge"
        );
        assert!(
            !repo
                .is_unread(Uuid::from_u128(0x13), ada())
                .await
                .expect("the read runs"),
            "an archived row is not unread either"
        );
    });
}

// ── the settings reads the push adapter depends on ────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_device_token_is_offered_only_when_push_is_switched_on(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        settings(&pool, ada(), true, true, Some("device-token")).await;
        settings(&pool, grace(), false, true, Some("silent-token")).await;
        settings(&pool, adam(), true, true, None).await;

        assert_eq!(
            repo.device_token(ada()).await.expect("the read runs"),
            Some("device-token".to_owned())
        );
        assert_eq!(
            repo.device_token(grace()).await.expect("the read runs"),
            None,
            "a token for a user who turned push off is a send they asked not to receive"
        );
        assert_eq!(
            repo.device_token(adam()).await.expect("the read runs"),
            None,
            "push on with no token registered is no device"
        );
        assert_eq!(
            repo.device_token(unknown()).await.expect("the read runs"),
            None,
            "a user with no settings row has no device"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn push_targets_selects_only_push_enabled_users_with_a_token(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        settings(&pool, ada(), true, true, Some("device-token")).await;
        settings(&pool, grace(), false, true, Some("silent-token")).await;
        settings(&pool, adam(), true, true, None).await;

        let targets = repo
            .push_targets(&[ada(), grace(), adam()])
            .await
            .expect("the query runs");

        assert_eq!(
            targets.len(),
            1,
            "one of the three has a deliverable device"
        );
        assert_eq!(targets[0].user_id, ada());
        assert_eq!(targets[0].device_token, "device-token");

        assert!(
            repo.push_targets(&[])
                .await
                .expect("the no-op runs")
                .is_empty(),
            "an empty fan-out asks the database nothing"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn clearing_a_token_leaves_the_preference_alone(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        settings(&pool, ada(), true, true, Some("device-token")).await;

        repo.clear_device_token(ada())
            .await
            .expect("the update runs");

        assert_eq!(repo.device_token(ada()).await.expect("the read runs"), None);
        let push_on = sqlx::query_scalar::<_, bool>(
            "SELECT push_notifications FROM settings WHERE user_id = $1",
        )
        .bind(ada())
        .fetch_one(&pool)
        .await
        .expect("the preference is read");

        assert!(
            push_on,
            "deleting a dead token must not turn the user's push setting off"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn in_app_notifications_are_off_without_a_settings_row(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        accounts(&pool).await;
        let repo = PgNotificationsRepo::new(pool.clone());

        assert!(
            !repo
                .app_notifications_enabled(ada())
                .await
                .expect("the read runs"),
            "a user who has never opened settings has not opted in"
        );

        settings(&pool, adam(), false, true, None).await;
        assert!(
            repo.app_notifications_enabled(adam())
                .await
                .expect("the read runs")
        );
    });
}
