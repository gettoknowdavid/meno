//! §10 **Integration** layer: `PgAuthRepo` against a real Postgres.
//!
//! # Why this file exists and why it needs a database
//!
//! Everything else in the test suite runs against `InMemoryAuthRepo`. That is the right
//! default — fast, hermetic, good at proving *behaviour*. It is also incapable of
//! proving the SQL is correct. An in-memory repository cannot catch a column that does
//! not exist, a table nobody created, or a constraint that rejects valid input; only
//! Postgres can, because only Postgres reads the migrations.
//!
//! So this suite is where [0016_auth_sessions_schema.sql] and the statements in
//! `repository/pg.rs` are actually executed. Until it runs against a real database,
//! that migration is unverified — `cargo check` type-checks the SQL strings without
//! ever sending one to a server.
//!
//! Run it with:
//!
//! ```text
//! cargo test -p meno-api --features test-support --test auth_repository_pg -- --ignored
//! ```
//!
//! against a `DATABASE_URL` pointing at an empty Postgres. `#[sqlx::test]` applies every
//! migration to a fresh per-test database and rolls it back, so the tests are
//! independent and need no cleanup.
//!
//! `#[ignore]` rather than absent, for the same reason as the repository's other live
//! tests: a contributor with no database must still get a green `cargo test`, and a
//! suite that fails on a missing Docker daemon is a suite people stop running.
//!
//! # A note on the `with_pool!` macro
//!
//! `#[sqlx::test]` has already created and migrated the database by the time the body
//! runs; the macro only skips the test with a readable message when `DATABASE_URL` is
//! absent. Skipping rather than panicking matters here: a panic would report "the
//! repository is broken", which is the opposite of the truth — nothing was tested.

// The three panicking lints are denied workspace-wide but explicitly *allowed in
// tests* — see `clippy.toml`, whose comment states the policy as "deny in
// apps/api/src, allow in tests/". The `allow-*-in-tests` keys there cover `#[test]`
// functions; the shared helpers in `support/mod.rs` and the small `async fn` helpers
// below are not `#[test]` functions, so the allowance does not reach them.
//
// This is the same policy applied to the same code, not a relaxation of it: the rule
// exists so a *request path* cannot panic, and nothing here serves a request. Every
// `expect` below is a fixture that would make the test meaningless if it failed, and
// its message says so.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use meno_api::modules::auth::model::{
    AuthProvider, DeviceContext, NewSession, NewUser, OtpType, Rotation, SessionLookup, User,
};
use meno_api::modules::auth::repository::{AuthRepo, PgAuthRepo, RepoDeps};
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

/// Skip with a readable message rather than failing opaquely when `DATABASE_URL` is unset.
fn database_url() -> Option<String> {
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
}

/// Run the body against the fresh, migrated database `#[sqlx::test]` created.
///
/// The point of this macro is *only* to skip cleanly when no database is configured.
/// It deliberately does **not** open its own connection: `#[sqlx::test]` hands the test
/// a pool already bound to a per-test database that it created, migrated and will drop
/// again. Reconnecting to `DATABASE_URL` inside the body points every test at the same
/// shared database instead — so the first test to create `ada@example.com` makes every
/// later one fail on a duplicate, and the suite reports collisions rather than the
/// behaviour it is written to check.
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

fn new_user(email: &str) -> NewUser {
    NewUser {
        full_name: "Ada Lovelace".to_owned(),
        email: email.to_owned(),
        // Not a real Argon2id hash: nothing here verifies a password, and hashing one
        // per test would dominate the runtime for no added coverage.
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$fixture$fixture".to_owned(),
    }
}

fn device(label: &str) -> DeviceContext {
    DeviceContext {
        device_label: Some(label.to_owned()),
        user_agent: Some("integration-test".to_owned()),
        ip: None,
    }
}

/// Create an account and return the stored row.
///
/// The *returned* row, not the input: `create_user` mints the primary key, so anything
/// keyed on the input struct would be keyed on nothing.
async fn user(repo: &PgAuthRepo, email: &str) -> User {
    repo.create_user(new_user(email))
        .await
        .expect("the fixture account is created")
}

/// A session bound to `user_id`, with a fresh `jti`.
fn new_session(user_id: Uuid, label: &str) -> NewSession {
    NewSession {
        user_id,
        refresh_jti: Uuid::new_v4(),
        token_hash: format!("hash-{label}"),
        expires_at: OffsetDateTime::now_utc() + Duration::days(1),
        device: device(label),
    }
}

/// Create a session and return `(session_id, refresh_jti)`.
async fn session(repo: &PgAuthRepo, user_id: Uuid, label: &str) -> (Uuid, Uuid) {
    let new = new_session(user_id, label);
    let jti = new.refresh_jti;
    let created = repo
        .create_session(new)
        .await
        .expect("the fixture session is created");
    (created.id, jti)
}

// ── users ──────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_created_user_round_trips_through_the_database(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });

        let created = user(&repo, "ada@example.com").await;
        assert_eq!(created.email, "ada@example.com");
        assert!(!created.verified, "a new account starts unverified");

        let found = repo
            .find_user_by_email("ada@example.com")
            .await
            .expect("the lookup runs")
            .expect("the account is there");
        assert_eq!(found.id, created.id);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_duplicate_address_is_refused_by_the_database_constraint(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });

        user(&repo, "ada@example.com").await;

        let error = repo
            .create_user(new_user("ada@example.com"))
            .await
            .expect_err("the unique index must reject the second row");

        // `EMAIL_TAKEN`, not the generic `CONFLICT`: §4.2's taxonomy exists so a client
        // can branch on a *specific* code, and "this address is registered" is a
        // different fix from "this conflicts with something". Asserting the narrow code
        // also proves the mapping is not over-broad — a `NOT NULL` violation must not
        // become this error.
        assert_eq!(error.code(), meno_core::ErrorCode::EmailTaken, "{error}");
        assert_eq!(meno_core::to_body(&error).http_status, 409);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_soft_deleted_user_is_invisible_to_both_lookups(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });

        let ada = user(&repo, "ada@example.com").await;
        assert!(repo.soft_delete(ada.id).await.expect("the delete runs"));

        // `deleted_at IS NULL` is a WHERE clause, which no in-memory double can get
        // wrong by accident — which is exactly why it needs real SQL to be worth
        // asserting.
        assert!(
            repo.find_user_by_email("ada@example.com")
                .await
                .expect("the lookup runs")
                .is_none()
        );
        assert!(
            repo.find_user_by_id(ada.id)
                .await
                .expect("the lookup runs")
                .is_none()
        );

        assert!(
            !repo.soft_delete(ada.id).await.expect("the delete runs"),
            "a second delete changed nothing and must say so"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_password_hash_is_reachable_only_by_address(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        user(&repo, "ada@example.com").await;

        assert!(
            repo.find_password_hash("ada@example.com")
                .await
                .expect("the lookup runs")
                .is_some()
        );
        assert!(
            repo.find_password_hash("nobody@example.com")
                .await
                .expect("the lookup runs")
                .is_none()
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn setting_a_password_hash_revokes_sessions_in_the_same_call(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let _ = session(&repo, ada.id, "laptop").await;

        repo.set_password_hash(ada.id, "$argon2id$v=19$fixture$new")
            .await
            .expect("the update runs");

        // §4.7 item 3's reset flow: changing the password ends every session, so a
        // stolen refresh token cannot outlive the credential it was issued against.
        assert!(
            repo.list_sessions(ada.id)
                .await
                .expect("the listing runs")
                .is_empty()
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn setting_the_email_verified_twice_reports_false_the_second_time(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;

        assert!(
            repo.set_email_verified(ada.id)
                .await
                .expect("the update runs")
        );
        assert!(
            !repo
                .set_email_verified(ada.id)
                .await
                .expect("the update runs"),
            "a second call changed nothing, and must say so"
        );
    });
}

// ── auth_sessions: the §4.7 rotation rules against real SQL ────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_freshly_created_session_can_be_rotated_exactly_once(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let (_, jti) = session(&repo, ada.id, "laptop").await;

        let first = repo
            .rotate_session(ada.id, jti, new_session(ada.id, "laptop"))
            .await
            .expect("the first rotation runs");
        assert!(matches!(first, Rotation::Rotated(_)), "{first:?}");

        // The core of §4.7 item 2. A rotated jti presented again is reuse, not an
        // unknown token, and the difference is what the service acts on.
        let second = repo
            .rotate_session(ada.id, jti, new_session(ada.id, "laptop"))
            .await
            .expect("the second rotation is answered, not an error");
        assert_eq!(second, Rotation::Replayed);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn rotating_marks_the_old_session_revoked_rather_than_deleting_it(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        // Cloned because this is the one test that also queries the pool directly, to
        // count rows the repository API deliberately does not expose.
        let repo = PgAuthRepo::new(RepoDeps { pool: pool.clone() });
        let ada = user(&repo, "ada@example.com").await;
        let (_, jti) = session(&repo, ada.id, "laptop").await;

        repo.rotate_session(ada.id, jti, new_session(ada.id, "laptop"))
            .await
            .expect("the rotation runs");

        // `list_sessions` filters `revoked_at IS NULL`, so the superseded row must be
        // *marked*. The in-memory double once deleted it instead, which lost the
        // `rotated_from` chain and made every replay read as `Unknown`; this asserts
        // the Postgres path does not repeat that mistake.
        let live = repo.list_sessions(ada.id).await.expect("the listing runs");
        assert_eq!(live.len(), 1, "one live session: the replacement");
        assert!(live.iter().all(|s| s.revoked_at.is_none()), "{live:?}");

        let total: i64 =
            sqlx::query_scalar("SELECT count(*) FROM public.auth_sessions WHERE user_id = $1")
                .bind(ada.id)
                .fetch_one(&pool)
                .await
                .expect("the count runs");
        assert_eq!(total, 2, "both rows are retained: one live, one superseded");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_jti_from_another_user_is_unknown_rather_than_replayed(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let mallory = user(&repo, "mallory@example.com").await;
        let (_, mallory_jti) = session(&repo, mallory.id, "tablet").await;

        // Someone else's live token is not evidence of theft. Reading it as `Replayed`
        // would let anyone sign *themselves* out by guessing a `jti`.
        let outcome = repo
            .rotate_session(ada.id, mallory_jti, new_session(ada.id, "laptop"))
            .await
            .expect("the rotation runs");
        assert_eq!(outcome, Rotation::Unknown);
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn revoking_all_sessions_ends_only_that_users_sessions(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let grace = user(&repo, "grace@example.com").await;

        let _ = session(&repo, ada.id, "laptop").await;
        let _ = session(&repo, ada.id, "phone").await;
        let _ = session(&repo, grace.id, "tablet").await;

        assert_eq!(
            repo.revoke_all_sessions(ada.id)
                .await
                .expect("the revoke runs"),
            2,
            "both of Ada's sessions, and no more"
        );

        assert!(
            repo.list_sessions(ada.id)
                .await
                .expect("the listing runs")
                .is_empty()
        );
        assert_eq!(
            repo.list_sessions(grace.id)
                .await
                .expect("the listing runs")
                .len(),
            1,
            "another user's sessions are not collateral damage"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn revoking_a_session_only_works_for_its_owner(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let mallory = user(&repo, "mallory@example.com").await;
        let (session_id, _) = session(&repo, ada.id, "laptop").await;

        // NotFound for both "no such session" and "not yours" — one variant, so the
        // endpoint cannot be used to discover which session ids exist.
        assert_eq!(
            repo.revoke_session(ada.id, session_id)
                .await
                .expect("the revoke runs"),
            SessionLookup::Found
        );
        assert_eq!(
            repo.revoke_session(mallory.id, session_id)
                .await
                .expect("the revoke runs"),
            SessionLookup::NotFound
        );
        assert_eq!(
            repo.revoke_session(ada.id, Uuid::new_v4())
                .await
                .expect("the revoke runs"),
            SessionLookup::NotFound
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn revoking_by_jti_ends_the_device_behind_a_refresh_token(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let (_, laptop_jti) = session(&repo, ada.id, "laptop").await;
        let _ = session(&repo, ada.id, "phone").await;

        // `POST /auth/logout` is presented a token, not a session id, so this is the
        // lookup it depends on.
        assert!(
            repo.revoke_session_for_jti(laptop_jti, ada.id)
                .await
                .expect("the revoke runs")
        );

        let live = repo.list_sessions(ada.id).await.expect("the listing runs");
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].device_label.as_deref(), Some("phone"));
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_device_label_round_trips_as_written(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let _ = session(&repo, ada.id, "Ada's Laptop").await;

        let listed = repo.list_sessions(ada.id).await.expect("the listing runs");
        assert_eq!(listed[0].device_label.as_deref(), Some("Ada's Laptop"));
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn sessions_are_listed_newest_first(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let _ = session(&repo, ada.id, "laptop").await;
        let _ = session(&repo, ada.id, "phone").await;

        let listed = repo.list_sessions(ada.id).await.expect("the listing runs");
        let timestamps: Vec<_> = listed.iter().map(|s| s.created_at).collect();
        let mut descending = timestamps.clone();
        descending.sort_by(|a, b| b.cmp(a));
        assert_eq!(timestamps, descending, "§9.4 orders by recency, not by id");
    });
}

// ── refresh_tokens.jti: the column 0016 added ──────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_refresh_token_is_found_by_its_jti(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let jti = Uuid::new_v4();

        repo.store_refresh_token(
            jti,
            ada.id,
            "hash-value",
            OffsetDateTime::now_utc() + Duration::days(1),
        )
        .await
        .expect("the token is stored");

        // The whole point of adding `jti`: before it, every refresh had to re-hash the
        // entire token to find its own row.
        assert!(
            repo.find_refresh_token(jti, ada.id)
                .await
                .expect("the lookup runs")
                .is_some()
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_refresh_token_from_another_user_is_not_found(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let mallory = user(&repo, "mallory@example.com").await;
        let jti = Uuid::new_v4();

        repo.store_refresh_token(
            jti,
            ada.id,
            "hash-value",
            OffsetDateTime::now_utc() + Duration::days(1),
        )
        .await
        .expect("the token is stored");

        assert!(
            repo.find_refresh_token(jti, mallory.id)
                .await
                .expect("the lookup runs")
                .is_none(),
            "`user_id` is in the predicate so a guessed jti cannot reach another account"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_consumed_refresh_token_is_not_returned_twice(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let jti = Uuid::new_v4();

        repo.store_refresh_token(
            jti,
            ada.id,
            "hash-value",
            OffsetDateTime::now_utc() + Duration::days(1),
        )
        .await
        .expect("the token is stored");

        assert!(
            repo.consume_refresh_token(jti, ada.id)
                .await
                .expect("the consume runs")
        );
        assert!(
            !repo
                .consume_refresh_token(jti, ada.id)
                .await
                .expect("the second consume runs"),
            "a refresh token is single-use"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn revoking_a_refresh_token_is_idempotent(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;
        let jti = Uuid::new_v4();

        repo.store_refresh_token(
            jti,
            ada.id,
            "hash-value",
            OffsetDateTime::now_utc() + Duration::days(1),
        )
        .await
        .expect("the token is stored");

        assert!(
            repo.revoke_refresh_token(jti, ada.id)
                .await
                .expect("the revoke runs")
        );
        assert!(
            !repo
                .revoke_refresh_token(jti, ada.id)
                .await
                .expect("the second revoke runs"),
            "logout must tolerate a client retry, which is why revoke is not consume"
        );
    });
}

// ── otps ───────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_one_time_code_can_be_spent_only_once(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let email = "ada@example.com";

        repo.save_otp(
            email,
            OtpType::VerifyEmail,
            "123456",
            OffsetDateTime::now_utc() + Duration::minutes(15),
        )
        .await
        .expect("the code is saved");

        assert!(
            repo.consume_otp(
                email,
                OtpType::VerifyEmail,
                "123456",
                OffsetDateTime::now_utc()
            )
            .await
            .expect("the consume runs")
        );
        assert!(
            !repo
                .consume_otp(
                    email,
                    OtpType::VerifyEmail,
                    "123456",
                    OffsetDateTime::now_utc()
                )
                .await
                .expect("the second consume runs"),
            "a spent code must not work twice"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_code_of_the_wrong_kind_is_refused(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let email = "ada@example.com";

        repo.save_otp(
            email,
            OtpType::ResetPassword,
            "654321",
            OffsetDateTime::now_utc() + Duration::minutes(15),
        )
        .await
        .expect("the code is saved");

        assert!(
            !repo
                .consume_otp(
                    email,
                    OtpType::VerifyEmail,
                    "654321",
                    OffsetDateTime::now_utc()
                )
                .await
                .expect("the consume runs"),
            "a reset code must not verify an email"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn an_expired_code_is_refused(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let email = "ada@example.com";

        repo.save_otp(
            email,
            OtpType::VerifyEmail,
            "999999",
            OffsetDateTime::now_utc() - Duration::minutes(1),
        )
        .await
        .expect("the code is saved, already expired");

        assert!(
            !repo
                .consume_otp(
                    email,
                    OtpType::VerifyEmail,
                    "999999",
                    OffsetDateTime::now_utc()
                )
                .await
                .expect("the consume runs")
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn issuing_a_second_code_replaces_the_first(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let email = "ada@example.com";
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);

        repo.save_otp(email, OtpType::VerifyEmail, "111111", expires)
            .await
            .expect("the first code is saved");
        repo.save_otp(email, OtpType::VerifyEmail, "222222", expires)
            .await
            .expect("the second code is saved");

        // The unique index on `(email, type)` is what makes this true: two live codes
        // for one address would double the guessing surface.
        assert!(
            !repo
                .consume_otp(
                    email,
                    OtpType::VerifyEmail,
                    "111111",
                    OffsetDateTime::now_utc()
                )
                .await
                .expect("the consume runs"),
            "the superseded code is dead"
        );
        assert!(
            repo.consume_otp(
                email,
                OtpType::VerifyEmail,
                "222222",
                OffsetDateTime::now_utc()
            )
            .await
            .expect("the consume runs")
        );
    });
}

// ── provider linking ───────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_provider_account_can_be_linked_and_found_again(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });
        let ada = user(&repo, "ada@example.com").await;

        repo.link_provider(ada.id, AuthProvider::Google, "google-sub-1")
            .await
            .expect("the link is created");

        assert_eq!(
            repo.find_user_by_provider(AuthProvider::Google, "google-sub-1")
                .await
                .expect("the lookup runs")
                .map(|found| found.id),
            Some(ada.id)
        );

        let providers = repo.list_providers(ada.id).await.expect("the listing runs");
        assert!(providers.contains(&AuthProvider::Password));
        assert!(providers.contains(&AuthProvider::Google), "{providers:?}");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_provider_only_account_has_no_password(#[allow(unused_variables)] pool: PgPool) {
    with_pool!(|pool| {
        let repo = PgAuthRepo::new(RepoDeps { pool });

        let created = repo
            .create_user_from_provider(
                new_user("ada@example.com"),
                AuthProvider::Google,
                "google-sub-2",
            )
            .await
            .expect("the account is created");

        assert!(
            repo.find_password_hash("ada@example.com")
                .await
                .expect("the lookup runs")
                .is_none(),
            "there is no password to verify; reporting one would invite a login attempt \
             that can only fail"
        );
        assert_eq!(
            repo.list_providers(created.id)
                .await
                .expect("the listing runs"),
            vec![AuthProvider::Google]
        );
    });
}

// ── the migration itself ───────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn the_auth_sessions_schema_is_what_the_repository_expects(
    #[allow(unused_variables)] pool: PgPool,
) {
    with_pool!(|pool| {
        // Column by column, because `pg.rs` names these in its statements and a typo
        // would otherwise surface only as a runtime error against a live database.
        for column in [
            "id",
            "user_id",
            "refresh_jti",
            "device_label",
            "user_agent",
            "ip",
            "created_at",
            "last_used_at",
            "revoked_at",
            "rotated_from",
        ] {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
                 WHERE table_name = 'auth_sessions' AND column_name = $1)",
            )
            .bind(column)
            .fetch_one(&pool)
            .await
            .expect("the introspection query runs");
            assert!(exists, "auth_sessions.{column} must exist (migration 0016)");
        }

        let jti_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'refresh_tokens' AND column_name = 'jti')",
        )
        .fetch_one(&pool)
        .await
        .expect("the introspection query runs");
        assert!(jti_exists, "refresh_tokens.jti must exist (migration 0016)");
    });
}
