//! §10 **Integration** layer: `PgChatRepo` against a real Postgres.
//!
//! # Why this file exists, and why it needs a database
//!
//! `chat_service.rs` runs against the in-memory double, which mirrors the SQL's
//! *semantics* and can therefore prove the rules — but cannot prove the SQL. Only
//! Postgres reads the migrations, and only this suite executes the statements in
//! `repository/pg.rs`.
//!
//! Three statements are worth their weight on their own, and each is a bug that this
//! suite caught or would have caught:
//!
//! - the **guarded insert**, whose entire reason for existing is that the liveness and
//!   membership checks are part of the write. `master` checked first and inserted
//!   after, so a broadcast that ended in between still received the message; the test
//!   `a_broadcast_that_ended_between_two_posts_refuses_the_second` states that
//!   sequence as two statements and asserts the second writes nothing;
//! - the **`WITH … RETURNING` join**, which is how one round trip both writes the row
//!   and returns it with its sender. A column that `MESSAGE_SELECT` names and the
//!   migration never declared is a runtime `ColumnNotFound`, and no unit test can see
//!   it;
//! - the **keyset page**, where a row-value comparison can be written in a way Postgres
//!   parses happily and answers wrongly.
//!
//! Run it with:
//!
//! ```text
//! cargo test -p meno-api --features test-support --test chat_repository_pg -- --ignored
//! ```
//!
//! against a `DATABASE_URL` pointing at an empty Postgres. `#[sqlx::test]` applies every
//! migration to a fresh per-test database and rolls it back, so the tests are independent
//! and need no cleanup. `with_pool!` skips rather than fails when `DATABASE_URL` is
//! absent: a contributor with no database must still get a green `cargo test`.

// The three panicking lints are denied workspace-wide but explicitly *allowed in tests*
// — see `clippy.toml`. The allow-*-in-tests keys cover `#[test]` functions; the helpers
// below are not `#[test]` functions, so the allowance does not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use meno_api::modules::chat::model::{NewMessage, NewReaction};
use meno_api::modules::chat::repository::{ChatRepo, MessageFilter, PgChatRepo};
use meno_core::ErrorCode;
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
    sqlx::query("INSERT INTO users (id, full_name, email, verified) VALUES ($1, $2, $3, true)")
        .bind(id)
        .bind(full_name)
        .bind(email)
        .execute(pool)
        .await
        .expect("the fixture account is inserted");
}

/// Insert a broadcast in the state the tests need.
///
/// `live` is `status = 'active'` with no `end_time`, which is exactly the pair the
/// guarded insert requires; `false` is a broadcast that has already finished.
async fn broadcast(pool: &PgPool, id: Uuid, creator: Uuid, live: bool) {
    let (status, end_time) = if live {
        ("active", None)
    } else {
        ("ended", Some(OffsetDateTime::now_utc()))
    };

    sqlx::query(
        "INSERT INTO broadcasts (id, title, creator_id, status, end_time) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind("Ada's show")
    .bind(creator)
    .bind(status)
    .bind(end_time)
    .execute(pool)
    .await
    .expect("the fixture broadcast is inserted");
}

/// Put an account in a room, or leave them with `left_at` set.
async fn participant(pool: &PgPool, broadcast_id: Uuid, user_id: Uuid, present: bool) {
    let left_at = if present {
        None
    } else {
        Some(OffsetDateTime::now_utc())
    };

    sqlx::query(
        "INSERT INTO broadcast_participants (broadcast_id, participant_id, role, left_at) \
         VALUES ($1, $2, 'participant', $3)",
    )
    .bind(broadcast_id)
    .bind(user_id)
    .bind(left_at)
    .execute(pool)
    .await
    .expect("the fixture participant is inserted");
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

/// The three accounts and the live room most tests use.
async fn fixtures(pool: &PgPool) {
    user(pool, ada(), "Ada Lovelace", "ada@example.com").await;
    user(pool, grace(), "Grace Hopper", "grace@example.com").await;
    broadcast(pool, room(), ada(), true).await;
    participant(pool, room(), ada(), true).await;
}

/// Post `content` as `sender`, returning what the statement produced.
async fn post(
    repo: &PgChatRepo,
    sender: Uuid,
    content: &str,
) -> Result<Option<meno_api::modules::chat::model::ChatMessage>, meno_core::Error> {
    repo.insert_message(NewMessage {
        broadcast_id: room(),
        sender_id: sender,
        content,
    })
    .await
}

// ── the guarded write ──────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_guarded_insert_writes_only_for_a_live_room_and_a_member(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());

        let written = post(&repo, ada(), "hello")
            .await
            .expect("the statement runs")
            .expect("a participant of a live broadcast is admitted");

        // The `WITH … RETURNING` join: the row comes back already carrying its sender,
        // which is the whole reason the write is one round trip.
        assert_eq!(written.sender.id, ada());
        assert_eq!(written.sender.full_name, "Ada Lovelace");
        assert_eq!(written.content, "hello");
        assert!(
            written.updated_at.is_none(),
            "a fresh row has never been edited"
        );
        assert_eq!(written.broadcast_id, room());

        // Registered but not in the room: refused, and nothing is written.
        let refused = post(&repo, grace(), "let me in").await.expect("runs");
        assert!(refused.is_none(), "a non-participant is not admitted");

        let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages")
            .fetch_one(&pool)
            .await
            .expect("the count runs");
        assert_eq!(stored, 1, "only the admitted message exists");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_broadcast_that_ended_between_two_posts_refuses_the_second(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());

        assert!(
            post(&repo, ada(), "while live")
                .await
                .expect("runs")
                .is_some(),
            "the room is live and Ada is in it"
        );

        // The host ends the broadcast — the exact moment `master` had a window in which
        // a message could still land, because it had already read "is this live".
        sqlx::query("UPDATE broadcasts SET status = 'ended', end_time = now() WHERE id = $1")
            .bind(room())
            .execute(&pool)
            .await
            .expect("the broadcast ends");

        let after = post(&repo, ada(), "into the void").await.expect("runs");
        assert!(
            after.is_none(),
            "the guard lives inside the insert, so an ended broadcast writes nothing"
        );

        let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages")
            .fetch_one(&pool)
            .await
            .expect("the count runs");
        assert_eq!(stored, 1);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn leaving_the_room_closes_the_write_path(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());

        assert!(post(&repo, ada(), "one").await.expect("runs").is_some());

        // `left_at` set is the same absence the room list sees.
        sqlx::query("UPDATE broadcast_participants SET left_at = now() WHERE broadcast_id = $1")
            .bind(room())
            .execute(&pool)
            .await
            .expect("the participant leaves");

        assert!(
            post(&repo, ada(), "two").await.expect("runs").is_none(),
            "somebody who left is no longer in the room"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_deleted_broadcast_admits_nothing(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());

        sqlx::query("UPDATE broadcasts SET deleted_at = now() WHERE id = $1")
            .bind(room())
            .execute(&pool)
            .await
            .expect("the broadcast is soft-deleted");

        assert!(
            post(&repo, ada(), "anyone there?")
                .await
                .expect("runs")
                .is_none(),
            "a deleted broadcast is not a live one"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_reaction_is_guarded_exactly_like_a_message(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());

        let reaction = repo
            .insert_reaction(NewReaction {
                broadcast_id: room(),
                sender_id: ada(),
                content: "👏",
            })
            .await
            .expect("the statement runs")
            .expect("a participant of a live broadcast may react");
        assert_eq!(reaction.content, "👏");
        assert_eq!(reaction.sender_id, ada());

        let refused = repo
            .insert_reaction(NewReaction {
                broadcast_id: room(),
                sender_id: grace(),
                content: "👏",
            })
            .await
            .expect("the statement runs");
        assert!(refused.is_none(), "the same guard refuses the same states");

        sqlx::query("UPDATE broadcasts SET status = 'ended', end_time = now() WHERE id = $1")
            .bind(room())
            .execute(&pool)
            .await
            .expect("the broadcast ends");

        let after = repo
            .insert_reaction(NewReaction {
                broadcast_id: room(),
                sender_id: ada(),
                content: "👏",
            })
            .await
            .expect("the statement runs");
        assert!(
            after.is_none(),
            "an ended broadcast accepts no reaction either"
        );
    });
}

// ── editing and deleting ───────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn an_edit_is_the_authors_and_the_triggers_updated_at_follows_it(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());
        let posted = post(&repo, ada(), "hello")
            .await
            .expect("runs")
            .expect("written");

        // Not the author: the `sender_id` in the WHERE matches nothing.
        assert!(
            repo.update_message(posted.id, grace(), "not mine")
                .await
                .expect("the statement runs")
                .is_none(),
            "somebody else cannot edit"
        );

        let edited = repo
            .update_message(posted.id, ada(), "hello, corrected")
            .await
            .expect("the statement runs")
            .expect("the author may edit");

        assert_eq!(edited.content, "hello, corrected");
        // Migration 0010's `set_updated_at` trigger, not an application-written column:
        // the assertion is that the UPDATE was enough to produce it.
        assert!(
            edited.updated_at.is_some(),
            "the trigger must stamp updated_at"
        );
        assert!(edited.is_edited());

        // And the row really changed, rather than the statement returning its own
        // in-memory guess.
        let stored: String = sqlx::query_scalar("SELECT content FROM chat_messages WHERE id = $1")
            .bind(posted.id)
            .fetch_one(&pool)
            .await
            .expect("the read runs");
        assert_eq!(stored, "hello, corrected");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_soft_delete_hides_the_row_from_every_read_and_leaves_it_in_the_table(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());
        let posted = post(&repo, ada(), "regrettable")
            .await
            .expect("runs")
            .expect("written");

        assert!(
            !repo
                .soft_delete_message(posted.id, grace())
                .await
                .expect("runs"),
            "somebody else cannot delete it"
        );
        assert!(
            repo.soft_delete_message(posted.id, ada())
                .await
                .expect("runs"),
            "the author can"
        );
        assert!(
            !repo
                .soft_delete_message(posted.id, ada())
                .await
                .expect("runs"),
            "a second delete affects no rows"
        );

        // Gone from every read…
        assert!(repo.find_message(posted.id).await.expect("runs").is_none());
        let page = repo
            .list_messages(MessageFilter::first_page(room(), 21))
            .await
            .expect("the page runs");
        assert!(page.is_empty(), "a deleted message is not in the history");

        // …and still in the table, with its body, which is what makes it auditable.
        let (content, deleted): (String, bool) = sqlx::query_as(
            "SELECT content, deleted_at IS NOT NULL FROM chat_messages WHERE id = $1",
        )
        .bind(posted.id)
        .fetch_one(&pool)
        .await
        .expect("the read runs");
        assert_eq!(content, "regrettable");
        assert!(deleted);
    });
}

// ── the page ───────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_keyset_page_is_newest_first_and_does_not_repeat_a_row(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());

        // Three messages an hour apart, plus a room they are not part of. The second
        // room belongs to Grace: one account has at most one broadcast on air, and the
        // partial unique index from migration 0017 enforces it even for a fixture.
        let other = Uuid::from_u128(0x200);
        broadcast(&pool, other, grace(), true).await;
        participant(&pool, other, ada(), true).await;

        for (content, seconds) in [
            ("oldest", 0_i64),
            ("middle", 3_600),
            ("newer", 7_200),
            ("newest", 10_800),
        ] {
            sqlx::query(
                "INSERT INTO chat_messages (content, sender_id, broadcast_id, created_at) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(content)
            .bind(ada())
            .bind(room())
            .bind(OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds).expect("instant"))
            .execute(&pool)
            .await
            .expect("the fixture message is inserted");
        }
        sqlx::query(
            "INSERT INTO chat_messages (content, sender_id, broadcast_id) VALUES ($1, $2, $3)",
        )
        .bind("elsewhere")
        .bind(ada())
        .bind(other)
        .execute(&pool)
        .await
        .expect("the other room's message is inserted");

        // A page of 2, fetched as `limit + 1 = 3`: two rows for the client and one
        // probe row that says "more exist" without a COUNT.
        let first = repo
            .list_messages(MessageFilter::first_page(room(), 3))
            .await
            .expect("the first page runs");
        assert_eq!(first.len(), 3, "two rows plus the probe row");
        assert_eq!(first[0].content, "newest", "newest first");
        assert_eq!(first[1].content, "newer");
        assert_eq!(first[2].content, "middle");
        assert!(
            first.iter().all(|row| row.broadcast_id == room()),
            "another room's messages must not leak into this page"
        );

        // The cursor is the last row the client was given.
        let last = first.last().expect("a last row");
        let second = repo
            .list_messages(MessageFilter {
                broadcast_id: room(),
                limit_plus_one: 3,
                cursor: Some((last.created_at, last.id)),
                order: meno_core::Order::Desc,
            })
            .await
            .expect("the second page runs");

        assert!(
            second.iter().all(|row| row.id != last.id),
            "the cursor is exclusive"
        );
        assert_eq!(
            second.len(),
            1,
            "only the oldest row remains below the cursor: {second:?}"
        );
        assert_eq!(second[0].content, "oldest");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_soft_deleted_sender_takes_their_messages_out_of_the_history(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());
        post(&repo, ada(), "hello")
            .await
            .expect("runs")
            .expect("written");

        sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
            .bind(ada())
            .execute(&pool)
            .await
            .expect("the account is soft-deleted");

        // The `JOIN users … AND u.deleted_at IS NULL` is a clause a double can get
        // wrong by accident, which is exactly why it needs real SQL.
        let page = repo
            .list_messages(MessageFilter::first_page(room(), 21))
            .await
            .expect("the page runs");
        assert!(
            page.is_empty(),
            "the history does not carry a deleted account's messages: {page:?}"
        );
    });
}

// ── the refusal path ───────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_two_explanations_a_refused_write_can_give_are_readable(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());

        assert!(repo.is_live_broadcast(room()).await.expect("runs"));
        assert!(repo.is_participant(room(), ada()).await.expect("runs"));
        assert!(!repo.is_participant(room(), grace()).await.expect("runs"));
        assert!(
            !repo
                .is_live_broadcast(Uuid::from_u128(0xdead))
                .await
                .expect("runs"),
            "an unknown broadcast is not live"
        );

        sqlx::query("UPDATE broadcasts SET status = 'ended', end_time = now() WHERE id = $1")
            .bind(room())
            .execute(&pool)
            .await
            .expect("the broadcast ends");
        assert!(
            !repo.is_live_broadcast(room()).await.expect("runs"),
            "the explanation follows the same columns the guard does"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn an_unknown_message_has_no_row_rather_than_an_error(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        fixtures(&pool).await;
        let repo = PgChatRepo::new(pool.clone());

        assert!(
            repo.find_message(Uuid::from_u128(0xdead))
                .await
                .expect("an absent row is not a driver failure")
                .is_none()
        );
        let failure = repo
            .update_message(Uuid::from_u128(0xdead), ada(), "nowhere")
            .await
            .expect("the statement runs");
        assert!(failure.is_none());
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn every_column_the_module_selects_exists_in_the_schema(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        // The unit test in `repository/pg.rs` asserts the projection *mentions* every
        // alias `MessageRow` expects; this one asserts the schema has the columns those
        // aliases read. Between them, a rename in either place fails a test rather than
        // a request.
        fixtures(&pool).await;

        for (table, column) in [
            ("chat_messages", "id"),
            ("chat_messages", "broadcast_id"),
            ("chat_messages", "content"),
            ("chat_messages", "sender_id"),
            ("chat_messages", "created_at"),
            ("chat_messages", "updated_at"),
            ("chat_messages", "deleted_at"),
            ("chat_reactions", "content"),
            ("chat_reactions", "sender_id"),
            ("chat_reactions", "broadcast_id"),
            ("chat_reactions", "created_at"),
        ] {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
                 WHERE table_name = $1 AND column_name = $2)",
            )
            .bind(table)
            .bind(column)
            .fetch_one(&pool)
            .await
            .expect("the catalogue read runs");

            assert!(exists, "{table}.{column} is missing from the schema");
        }

        // A `content` wider than 256 characters must be refused by the column rather
        // than silently truncated — that is what makes `MESSAGE_MAX_CHARS` the boundary
        // the frame validates against instead of a number chosen twice.
        let too_long = repo_overflow_error(&pool).await;
        assert_eq!(
            too_long.code(),
            ErrorCode::Internal,
            "the driver's refusal surfaces as a 500 with the driver's text logged"
        );
    });
}

/// Insert an over-wide message through the repository and return the error.
///
/// Split out so the assertion above is about the *outcome* rather than the statement.
async fn repo_overflow_error(pool: &PgPool) -> meno_core::Error {
    let repo = PgChatRepo::new(pool.clone());
    post(&repo, ada(), &"a".repeat(257))
        .await
        .expect_err("257 characters do not fit VARCHAR(256)")
}
