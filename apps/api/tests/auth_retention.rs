//! §10 **Contract** layer: auth retention (gap 3 / gap 9).
//!
//! # Why this suite exists
//!
//! Expired auth rows used to accumulate forever, because nothing deleted them: a login
//! only *reads* expiry to refuse an old credential. The job runner's auth sweep is only
//! useful if the repository method behind it removes exactly the rows that can no longer
//! be used and keeps exactly the ones that can — a sweep that deleted a live refresh
//! token would log every user out, and one that deleted nothing would look identical in
//! the logs.
//!
//! # What this cannot prove
//!
//! The Postgres SQL. This drives [`InMemoryAuthRepo`], which mirrors the predicate; the
//! statements in `repository/pg.rs` need the Postgres-backed tier. What it does pin is
//! the boundary the SQL must implement: expiry is strict, and a row on the live side of
//! it survives.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use meno_api::modules::auth::model::OtpType;
use meno_api::modules::auth::repository::{AuthRepo, InMemoryAuthRepo};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn user() -> Uuid {
    Uuid::from_u128(1)
}

#[tokio::test]
async fn expired_rows_are_removed_and_live_rows_survive() {
    let repo = InMemoryAuthRepo::new();
    let now = OffsetDateTime::now_utc();

    let expired_jti = Uuid::from_u128(10);
    let live_jti = Uuid::from_u128(11);
    repo.store_refresh_token(
        expired_jti,
        user(),
        "expired-hash",
        now - Duration::minutes(1),
    )
    .await
    .expect("storing the expired token");
    repo.store_refresh_token(live_jti, user(), "live-hash", now + Duration::hours(1))
        .await
        .expect("storing the live token");

    repo.save_otp(
        "ada@example.com",
        OtpType::VerifyEmail,
        "expired-digest",
        now - Duration::seconds(1),
    )
    .await
    .expect("storing the expired code");
    repo.save_otp(
        "grace@example.com",
        OtpType::VerifyEmail,
        "live-digest",
        now + Duration::minutes(10),
    )
    .await
    .expect("storing the live code");

    let report = repo
        .purge_expired(now, now - Duration::days(7))
        .await
        .expect("the sweep runs");

    assert_eq!(report.refresh_tokens, 1, "only the expired token goes");
    assert_eq!(report.otps, 1, "only the expired code goes");

    assert!(
        repo.find_refresh_token(expired_jti, user())
            .await
            .expect("reading")
            .is_none(),
        "an expired token must not survive the sweep"
    );
    assert!(
        repo.find_refresh_token(live_jti, user())
            .await
            .expect("reading")
            .is_some(),
        "a live token must be untouched"
    );

    // The live code is still spendable, which is the half a count alone would not prove.
    assert!(
        repo.consume_otp(
            "grace@example.com",
            OtpType::VerifyEmail,
            "live-digest",
            now
        )
        .await
        .expect("spending"),
        "a live code must still verify"
    );
}

#[tokio::test]
async fn a_second_pass_finds_nothing_left_to_remove() {
    // Idempotence matters because the sweep runs on a timer: if it reported work on
    // every tick, a real leak would be invisible in the logs.
    let repo = InMemoryAuthRepo::new();
    let now = OffsetDateTime::now_utc();

    repo.save_otp(
        "ada@example.com",
        OtpType::ResetPassword,
        "expired-digest",
        now - Duration::minutes(1),
    )
    .await
    .expect("storing the expired code");

    let first = repo
        .purge_expired(now, now - Duration::days(7))
        .await
        .expect("first pass");
    assert_eq!(first.total(), 1);

    let second = repo
        .purge_expired(now, now - Duration::days(7))
        .await
        .expect("second pass");
    assert_eq!(second.total(), 0, "nothing expired since the first pass");
}

#[tokio::test]
async fn a_live_repository_is_usable_through_a_trait_object() {
    // The runner holds the service, not the concrete double; this asserts the seam the
    // job actually depends on compiles and answers through `dyn AuthRepo`.
    let repo: Arc<dyn AuthRepo> = Arc::new(InMemoryAuthRepo::new());
    let now = OffsetDateTime::now_utc();

    let report = repo
        .purge_expired(now, now - Duration::days(7))
        .await
        .expect("the sweep runs");

    assert_eq!(report.total(), 0);
}
