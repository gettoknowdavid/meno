//! §10 **Integration** layer: `PgProfileRepo` and `PgSettingsRepo` against a real
//! Postgres.
//!
//! # Why this file exists and why it needs a database
//!
//! Everything else in the profile/settings suites runs against the in-memory
//! doubles, which mirror the SQL's *semantics* and can therefore prove the rules —
//! but cannot prove the SQL. An in-memory repository cannot catch a column that does
//! not exist, a `search_vector` that matches differently from a lowercase substring,
//! a `COALESCE` written against the wrong branch, or a row-value comparison Postgres
//! parses but answers wrongly. Only Postgres reads the migrations, and only this
//! suite executes the statements in `repository/pg.rs`.
//!
//! Two statements here are worth their weight on their own:
//!
//! - the **search cursor**, which `master` shipped as dead code (`rank > $n` with
//!   `$n` never bound), so every page was page one — the in-memory double asserts
//!   the *behaviour*, this file proves the `QueryBuilder` binds both halves of the
//!   row comparison into real SQL;
//! - the **settings upsert**, whose INSERT branch must default to `Settings::new`'s
//!   `TRUE`s rather than the migration's column defaults of `FALSE` — a disagreement
//!   that would silently flip a user's notification toggles on their first save.
//!
//! Run it with:
//!
//! ```text
//! cargo test -p meno-api --features test-support --test profile_repository_pg -- --ignored
//! ```
//!
//! against a `DATABASE_URL` pointing at an empty Postgres. `#[sqlx::test]` applies
//! every migration to a fresh per-test database and rolls it back, so the tests are
//! independent and need no cleanup. `with_pool!` skips rather than fails when
//! `DATABASE_URL` is absent: a contributor with no database must still get a green
//! `cargo test`, and a suite that fails on a missing Docker daemon is a suite people
//! stop running.

// The three panicking lints are denied workspace-wide but explicitly *allowed in
// tests* — see `clippy.toml`. The allow-*-in-tests keys cover `#[test]` functions;
// the helpers below are not `#[test]` functions, so the allowance does not reach
// them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use meno_api::modules::profile::dto::ProfileSearchQuery;
use meno_api::modules::profile::model::PROFILE_COLUMNS;
use meno_api::modules::profile::repository::{PgProfileRepo, ProfileRepo, SearchFilter};
use meno_api::modules::settings::model::Settings;
use meno_api::modules::settings::repository::{PgSettingsRepo, SettingsInput, SettingsRepo};
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
///
/// Same macro as `auth_repository_pg.rs`, and for the same reason: `#[sqlx::test]`
/// hands the test a pool already bound to a per-test database it created, migrated
/// and will drop again. Reopening `DATABASE_URL` inside the body would point every
/// test at one shared database, where the first fixture's rows collide with the
/// second test's expectations.
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
async fn user(pool: &PgPool, id: Uuid, full_name: &str, email: &str, created_at: OffsetDateTime) {
    sqlx::query(
        "INSERT INTO users (id, full_name, email, verified, created_at) \
         VALUES ($1, $2, $3, true, $4)",
    )
    .bind(id)
    .bind(full_name)
    .bind(email)
    .bind(created_at)
    .execute(pool)
    .await
    .expect("the fixture account is inserted");
}

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(seconds).expect("a plausible fixture instant")
}

fn ada() -> Uuid {
    Uuid::from_u128(1)
}
fn adam() -> Uuid {
    Uuid::from_u128(2)
}
fn grace() -> Uuid {
    Uuid::from_u128(3)
}

fn search(
    q: &str,
    limit: i64,
    cursor: Option<(OffsetDateTime, Uuid)>,
    viewer: Uuid,
) -> SearchFilter<'_> {
    SearchFilter {
        viewer_id: viewer,
        query: q,
        cursor,
        limit_plus_one: limit + 1,
    }
}

// ── profile: the columns `PROFILE_COLUMNS` names ───────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_profile_row_round_trips_every_column_the_dto_reads(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_000),
        )
        .await;
        let repo = PgProfileRepo::new(pool.clone());

        let found = repo
            .find_by_id(ada())
            .await
            .expect("the lookup runs")
            .expect("the fixture row is there");

        // Every field `read_me` copies into `MeResponse` — a typo in
        // `PROFILE_COLUMNS` surfaces here as a compile-time-free runtime error, which
        // is exactly what `cargo check` cannot catch.
        assert_eq!(found.id, ada());
        assert_eq!(found.full_name, "Ada Lovelace");
        assert_eq!(found.email, "ada@example.com");
        assert!(found.verified);
        assert_eq!(found.followers, 0, "the trigger-managed counters read back");
        assert_eq!(found.created_at, at(1_700_000_000));
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_soft_deleted_profile_is_invisible_to_every_lookup(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_000),
        )
        .await;
        let repo = PgProfileRepo::new(pool.clone());

        sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
            .bind(ada())
            .execute(&pool)
            .await
            .expect("the soft delete runs");

        // `deleted_at IS NULL` is a WHERE clause the in-memory double cannot get
        // wrong by accident — which is precisely why it needs real SQL.
        assert!(
            repo.find_by_id(ada())
                .await
                .expect("the lookup runs")
                .is_none()
        );
        assert!(
            repo.find_avatar_key(ada())
                .await
                .expect("the lookup runs")
                .is_none()
        );

        let error = repo
            .update_profile(ada(), Some("Ghost"), None)
            .await
            .expect_err("a deleted account cannot be edited");
        assert_eq!(error.code(), meno_core::ErrorCode::NotFound, "{error}");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn update_profile_coalesces_and_returns_the_new_row(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_000),
        )
        .await;
        let repo = PgProfileRepo::new(pool.clone());

        // `None` must keep the stored value — the COALESCE rule, against the real
        // statement rather than against the double's imitation of it.
        let updated = repo
            .update_profile(ada(), Some("Ada Byron"), None)
            .await
            .expect("the update runs");
        assert_eq!(updated.full_name, "Ada Byron");
        assert_eq!(
            updated.email, "ada@example.com",
            "untouched columns come back"
        );

        // And an explicit empty string clears, because it is not NULL.
        let updated = repo
            .update_profile(ada(), None, Some(""))
            .await
            .expect("the update runs");
        assert_eq!(updated.bio.as_deref(), Some(""));
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn set_avatar_points_the_row_at_the_object(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_000),
        )
        .await;
        let repo = PgProfileRepo::new(pool.clone());

        let key = format!("avatars/{}.png", ada());
        repo.set_avatar(ada(), &key, Some("https://cdn.test/avatars/ada.png"))
            .await
            .expect("the update runs");
        assert_eq!(
            repo.find_avatar_key(ada())
                .await
                .expect("the lookup runs")
                .as_deref(),
            Some(key.as_str())
        );

        let error = repo
            .set_avatar(Uuid::from_u128(99), &key, None)
            .await
            .expect_err("an unknown account has no row to update");
        assert_eq!(error.code(), meno_core::ErrorCode::NotFound, "{error}");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn is_following_reads_the_real_join_table(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_000),
        )
        .await;
        user(
            &pool,
            adam(),
            "Adam Smith",
            "adam@example.com",
            at(1_700_000_001),
        )
        .await;
        let repo = PgProfileRepo::new(pool.clone());

        assert!(
            !repo
                .is_following(adam(), ada())
                .await
                .expect("the query runs")
        );

        sqlx::query(
            "INSERT INTO user_subscribers (subscriber_id, subscription_id) VALUES ($1, $2)",
        )
        .bind(adam())
        .bind(ada())
        .execute(&pool)
        .await
        .expect("the follow is inserted");

        assert!(
            repo.is_following(adam(), ada())
                .await
                .expect("the query runs")
        );
        assert!(
            !repo
                .is_following(ada(), adam())
                .await
                .expect("the query runs"),
            "the direction matters: subscriber_id is the follower"
        );

        // And the trigger kept the counters in step while we were looking.
        let followers: i64 = sqlx::query_scalar("SELECT followers FROM users WHERE id = $1")
            .bind(ada())
            .fetch_one(&pool)
            .await
            .expect("the count runs");
        assert_eq!(
            followers, 1,
            "migration 0009's trigger increments on insert"
        );
    });
}

// ── search: the cursor `master` shipped dead ───────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn search_matches_by_search_vector_rather_than_by_memory(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_100),
        )
        .await;
        user(
            &pool,
            grace(),
            "Grace Hopper",
            "grace@example.com",
            at(1_700_000_200),
        )
        .await;
        let repo = PgProfileRepo::new(pool.clone());

        // `to_tsvector('english', …)` stems and ignores case; a memory double's
        // substring test would agree here by coincidence, and disagree at the edges.
        let page = repo
            .search(search("lovelace", 20, None, ada()))
            .await
            .expect("the search runs");
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].full_name, "Ada Lovelace");
        assert_eq!(page[0].id, ada());

        let none = repo
            .search(search("zzzz", 20, None, ada()))
            .await
            .expect("an empty result is not an error");
        assert!(none.is_empty());
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_cursor_binds_both_halves_of_the_row_comparison(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        // Three matches, an hour apart — all stemming to the token `ada`, which is
        // what `plainto_tsquery('english', 'ada')` matches. (A substring test would
        // also match "Adam"; Postgres does not, and the fixture must not pretend.)
        user(
            &pool,
            Uuid::from_u128(4),
            "Ada King",
            "king@example.com",
            at(1_700_000_000),
        )
        .await; // oldest
        user(
            &pool,
            adam(),
            "Ada Byron",
            "byron@example.com",
            at(1_700_000_060),
        )
        .await;
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_120),
        )
        .await; // newest
        user(
            &pool,
            grace(),
            "Grace Hopper",
            "grace@example.com",
            at(1_700_000_180),
        )
        .await; // no match
        let repo = PgProfileRepo::new(pool.clone());

        let viewer = Uuid::from_u128(9);

        // Page one: `limit_plus_one = 2` says "more exist" without a COUNT.
        let rows = repo
            .search(search("ada", 1, None, viewer))
            .await
            .expect("the first page runs");
        assert_eq!(rows.len(), 2, "the probe row is present: {rows:?}");
        assert_eq!(rows[0].full_name, "Ada Lovelace", "newest first");
        let last = rows.last().expect("at least one match");
        assert_eq!(last.full_name, "Ada Byron");

        // Page two with the cursor at the last row's (created_at, id): the condition
        // must actually filter — `master`'s unbound `$n` returned page one again.
        let page_two = repo
            .search(search("ada", 1, Some((last.created_at, last.id)), viewer))
            .await
            .expect("the second page runs");
        assert_eq!(
            page_two.len(),
            1,
            "exactly the oldest match remains: {page_two:?}"
        );
        assert_eq!(page_two[0].full_name, "Ada King");
        assert!(
            page_two
                .iter()
                .all(|row| row.id != rows[0].id && row.id != last.id),
            "pages must not overlap — a repeated row is the dead-cursor bug in disguise"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn search_reports_the_viewer_follow_state_per_row(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_100),
        )
        .await;
        // Both names stem to the token `ada` — `plainto_tsquery` matches whole
        // stemmed tokens, and "Adam" would not have matched it.
        user(
            &pool,
            adam(),
            "Ada Byron",
            "byron@example.com",
            at(1_700_000_000),
        )
        .await;
        // The viewer must exist too: `user_subscribers` has a foreign key on both
        // columns, and a fixture that skips it tests the constraint, not the query.
        let viewer = Uuid::from_u128(9);
        user(
            &pool,
            viewer,
            "Margaret Hamilton",
            "margaret@example.com",
            at(1_700_000_001),
        )
        .await;
        let repo = PgProfileRepo::new(pool.clone());

        sqlx::query(
            "INSERT INTO user_subscribers (subscriber_id, subscription_id) VALUES ($1, $2)",
        )
        .bind(viewer)
        .bind(ada())
        .execute(&pool)
        .await
        .expect("the follow is inserted");

        let rows = repo
            .search(search("ada", 20, None, viewer))
            .await
            .expect("the search runs");
        assert_eq!(rows.len(), 2);
        let follows = rows
            .iter()
            .find(|row| row.id == ada())
            .expect("Ada in the results");
        let other = rows
            .iter()
            .find(|row| row.id == adam())
            .expect("the other Ada in the results");
        assert!(follows.is_following);
        assert!(!other.is_following, "the EXISTS is per row and per viewer");
    });
}

// ── settings: the upsert both branches ─────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_first_settings_save_defaults_to_the_model_not_the_column(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_000),
        )
        .await;
        let repo = PgSettingsRepo::new(pool.clone());

        // Nothing has created a row yet — `master`'s plain UPDATE would have hit
        // RowNotFound here and answered 500.
        assert!(
            repo.find_by_user_id(ada())
                .await
                .expect("the lookup runs")
                .is_none()
        );

        // A first save that only names `language`.
        let saved = repo
            .update(&SettingsInput {
                user_id: ada(),
                push_notifications: None,
                app_notifications: None,
                email_notifications: None,
                push_notification_token: None,
                notification_preferences: None,
                display: None,
                language: Some("fr"),
            })
            .await
            .expect("the upsert runs");

        assert_eq!(saved.language, "fr");
        // The heart of it: migration 0013's column defaults are `false`, but the
        // INSERT branch must produce what `Settings::new` shows — `true`. A user
        // whose toggles flipped off on their first save would never find them.
        assert!(
            saved.push_notifications,
            "the INSERT branch defaults to `Settings::new`, not to the column default"
        );
        assert!(saved.app_notifications);
        assert!(saved.email_notifications);
        assert_eq!(saved.display.to_string(), "system");
        assert_eq!(saved.user_id, ada());
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_second_settings_save_keeps_every_field_it_was_not_given(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_000),
        )
        .await;
        let repo = PgSettingsRepo::new(pool.clone());

        repo.update(&SettingsInput {
            user_id: ada(),
            push_notifications: Some(false),
            app_notifications: None,
            email_notifications: None,
            push_notification_token: Some("fcm-token"),
            notification_preferences: None,
            display: None,
            language: Some("fr"),
        })
        .await
        .expect("the first save runs");

        // COALESCE branch: only `language` is named; everything stored survives.
        let saved = repo
            .update(&SettingsInput {
                user_id: ada(),
                push_notifications: None,
                app_notifications: None,
                email_notifications: None,
                push_notification_token: None,
                notification_preferences: None,
                display: Some(&meno_api::modules::settings::model::Display::Dark),
                language: Some("de"),
            })
            .await
            .expect("the second save runs");

        assert_eq!(saved.language, "de");
        assert_eq!(saved.display.to_string(), "dark");
        assert!(
            !saved.push_notifications,
            "an unmentioned field keeps the value a previous save stored"
        );
        assert_eq!(
            saved.push_notification_token.as_deref(),
            Some("fcm-token"),
            "COALESCE keeps the token; only clear_push_token may remove it"
        );

        // One row per user, however many saves.
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM settings WHERE user_id = $1")
            .bind(ada())
            .fetch_one(&pool)
            .await
            .expect("the count runs");
        assert_eq!(count, 1, "the ON CONFLICT updated rather than inserted");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn clearing_the_token_unsets_it_without_touching_the_rest(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_000),
        )
        .await;
        let repo = PgSettingsRepo::new(pool.clone());

        repo.update(&SettingsInput {
            user_id: ada(),
            push_notifications: None,
            app_notifications: None,
            email_notifications: None,
            push_notification_token: Some("fcm-token"),
            notification_preferences: None,
            display: None,
            language: Some("fr"),
        })
        .await
        .expect("the save runs");

        repo.clear_push_token(ada()).await.expect("the clear runs");
        let settings = repo
            .find_by_user_id(ada())
            .await
            .expect("the lookup runs")
            .expect("the row exists");
        assert!(settings.push_notification_token.is_none());
        assert_eq!(settings.language, "fr", "the clear touches one column");

        // Clearing again — and clearing for an account with no row — are both
        // no-op successes: a client retrying a DELETE cannot be told it failed.
        repo.clear_push_token(ada()).await.expect("idempotent");
        repo.clear_push_token(Uuid::from_u128(99))
            .await
            .expect("a zero-row UPDATE is success");
    });
}

// ── the shapes the repository promises ─────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn every_column_profile_selects_exists_in_the_schema(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        // Column by column, because `PROFILE_COLUMNS` is a string that `cargo check`
        // type-checks as a string. A typo here is a runtime error in production and
        // nothing else.
        for column in PROFILE_COLUMNS.split(", ") {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
                 WHERE table_name = 'users' AND column_name = $1)",
            )
            .bind(column)
            .fetch_one(&pool)
            .await
            .expect("the introspection query runs");
            assert!(exists, "users.{column} must exist (migration 0003)");
        }

        let dto: ProfileSearchQuery = serde_json::from_str(r#"{"q":"ada"}"#)
            .expect("the search query parses without a cursor");
        assert_eq!(dto.limit(), 20, "the default page size");
        assert!(dto.cursor().is_none());
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_settings_table_is_what_the_repository_expects(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        for column in [
            "id",
            "user_id",
            "push_notifications",
            "app_notifications",
            "email_notifications",
            "push_notification_token",
            "notification_preferences",
            "display",
            "language",
        ] {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
                 WHERE table_name = 'settings' AND column_name = $1)",
            )
            .bind(column)
            .fetch_one(&pool)
            .await
            .expect("the introspection query runs");
            assert!(exists, "settings.{column} must exist (migration 0013)");
        }

        // `Settings` and the row agree: this is the decode path `find_by_user_id`
        // takes, exercised once directly.
        user(
            &pool,
            ada(),
            "Ada Lovelace",
            "ada@example.com",
            at(1_700_000_000),
        )
        .await;
        let repo = PgSettingsRepo::new(pool.clone());
        let created = repo
            .update(&SettingsInput {
                user_id: ada(),
                push_notifications: Some(true),
                app_notifications: Some(false),
                email_notifications: Some(true),
                push_notification_token: None,
                notification_preferences: Some(&serde_json::json!({"quiet": true})),
                display: None,
                language: None,
            })
            .await
            .expect("the upsert runs");
        let decoded: Settings = sqlx::query_as("SELECT * FROM settings WHERE user_id = $1")
            .bind(ada())
            .fetch_one(&pool)
            .await
            .expect("the row decodes");
        assert_eq!(decoded.id, created.id);
        assert!(!decoded.app_notifications);
        assert_eq!(decoded.notification_preferences["quiet"], true);
    });
}
