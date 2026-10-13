//! §10 **service** layer: the subscribers rules, against the in-memory double.
//!
//! # What this suite proves, and what it cannot
//!
//! Every assertion runs through a real [`SubscribersService`] with
//! [`InMemorySubscribersRepo`] behind it, so it proves the *rules*: that a repeated follow
//! is one row, that a follow of a nonexistent account is a 404 rather than a silent write,
//! that unfollowing what is not there is a success, and — the part `master` could not have
//! had — that a page is annotated for the viewer and paginates without repeating or skipping
//! a row.
//!
//! It does **not** run a line of SQL. `repository::pg` is covered by
//! `subscribers_repository_pg.rs`, which needs a live Postgres; in particular the two
//! counter columns and the `deleted_at` predicate are only assertable there.

// The panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use std::sync::Arc;

use meno_api::modules::subscribers::dto::{FollowDirection, SubscriberDto};
use meno_api::modules::subscribers::repository::{InMemorySubscribersRepo, SubscribersRepo};
use meno_api::modules::subscribers::service::SubscribersService;
use meno_core::{CursorParams, ErrorCode};
use time::OffsetDateTime;
use uuid::Uuid;

// ── fixtures ──────────────────────────────────────────────────────────────

fn ada() -> Uuid {
    Uuid::from_u128(0x1)
}

fn grace() -> Uuid {
    Uuid::from_u128(0x2)
}

fn adam() -> Uuid {
    Uuid::from_u128(0x3)
}

fn instant(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds).expect("a plausible instant")
}

struct Harness {
    service: SubscribersService,
    repo: Arc<InMemorySubscribersRepo>,
}

/// Three live accounts — Ada, Grace and Adam — and no follows yet.
fn harness() -> Harness {
    let repo = Arc::new(InMemorySubscribersRepo::new());
    for (id, name) in [
        (ada(), "Ada Lovelace"),
        (grace(), "Grace Hopper"),
        (adam(), "Adam Smith"),
    ] {
        repo.seed_user(id, name, None, None);
    }

    let service = SubscribersService::new(Arc::clone(&repo) as Arc<dyn SubscribersRepo>);
    Harness { service, repo }
}

fn page(limit: i64) -> CursorParams {
    CursorParams {
        cursor: None,
        limit: Some(limit),
    }
}

/// The row ids of a page, in the order the page carries them.
fn ids(items: &[SubscriberDto]) -> Vec<Uuid> {
    items.iter().map(|item| item.id).collect()
}

// ── following and unfollowing ─────────────────────────────────────────────

#[tokio::test]
async fn following_a_user_records_one_row() {
    let h = harness();

    h.service
        .subscribe(ada(), grace())
        .await
        .expect("the follow is recorded");

    assert!(h.repo.follows(ada(), grace()));
    assert_eq!(h.repo.stored_follow_count(), 1);
}

#[tokio::test]
async fn following_twice_is_idempotent_rather_than_an_error() {
    // A double tap, or a client retrying after a timeout. `master` relied on the same
    // `ON CONFLICT DO NOTHING`, and the double mirrors it — which matters beyond tidiness:
    // a second row would move the counters the trigger maintains twice.
    let h = harness();

    h.service.subscribe(ada(), grace()).await.expect("first");
    h.service.subscribe(ada(), grace()).await.expect("second");

    assert_eq!(h.repo.stored_follow_count(), 1);
}

#[tokio::test]
async fn following_yourself_is_a_400_and_writes_nothing() {
    let h = harness();

    let error = h
        .service
        .subscribe(ada(), ada())
        .await
        .expect_err("a self-follow is refused");

    assert_eq!(error.code(), ErrorCode::BadRequest);
    assert_eq!(h.repo.stored_follow_count(), 0);
}

#[tokio::test]
async fn following_a_user_who_does_not_exist_is_a_404() {
    let h = harness();

    let error = h
        .service
        .subscribe(ada(), Uuid::from_u128(0xff))
        .await
        .expect_err("there is nobody to follow");

    assert_eq!(error.code(), ErrorCode::NotFound);
    assert_eq!(
        h.repo.stored_follow_count(),
        0,
        "a refused follow must not leave a row behind"
    );
}

#[tokio::test]
async fn unfollowing_removes_the_row() {
    let h = harness();
    h.service.subscribe(ada(), grace()).await.expect("follow");

    h.service
        .unsubscribe(ada(), grace())
        .await
        .expect("the follow is removed");

    assert!(!h.repo.follows(ada(), grace()));
    assert_eq!(h.repo.stored_follow_count(), 0);
}

#[tokio::test]
async fn unfollowing_somebody_you_do_not_follow_is_a_success() {
    // `DELETE` is idempotent: a retried tap must not be told it failed, and the
    // postcondition the caller asked for already holds.
    let h = harness();

    h.service
        .unsubscribe(ada(), grace())
        .await
        .expect("nothing to remove is still success");
    h.service
        .unsubscribe(ada(), grace())
        .await
        .expect("and still success the second time");

    assert_eq!(h.repo.stored_follow_count(), 0);
}

#[tokio::test]
async fn unfollowing_yourself_is_a_no_op_rather_than_a_refusal() {
    // `master` answered the subscribe guard here too, with a message about subscribing.
    // There is no row to delete — the migration's CHECK makes one impossible — so the
    // requested postcondition already holds and the answer is success.
    let h = harness();

    h.service
        .unsubscribe(ada(), ada())
        .await
        .expect("a no-op is not a failure");

    assert_eq!(h.repo.stored_follow_count(), 0);
}

#[tokio::test]
async fn unfollowing_a_user_who_does_not_exist_is_a_404() {
    let h = harness();

    let error = h
        .service
        .unsubscribe(ada(), Uuid::from_u128(0xff))
        .await
        .expect_err("there is nobody to unfollow");

    assert_eq!(error.code(), ErrorCode::NotFound);
}

// ── the two lists ─────────────────────────────────────────────────────────

/// Ada follows Grace and Adam; Grace follows Adam.
///
/// Every follow has its own instant so the ordering assertions are deterministic.
async fn a_small_graph(h: &Harness) {
    h.repo.seed_follow(ada(), grace(), instant(10));
    h.repo.seed_follow(grace(), adam(), instant(20));
    h.repo.seed_follow(ada(), adam(), instant(30));
}

#[tokio::test]
async fn a_followers_page_is_annotated_for_the_viewer_not_the_owner() {
    // Adam's followers are Ada and Grace. Ada is looking, so the flag answers "does *Ada*
    // follow this person": true for Grace (she is followed by Ada), false for Ada herself.
    // Annotating for the owner instead would make every flag meaningless on one's own list.
    let h = harness();
    a_small_graph(&h).await;

    let result = h
        .service
        .page(FollowDirection::Followers, adam(), ada(), &page(10))
        .await
        .expect("the page is read");

    assert_eq!(ids(&result.data), vec![ada(), grace()], "newest first");
    assert!(!result.data[0].is_following, "Ada does not follow herself");
    assert!(result.data[1].is_following, "Ada follows Grace");
    assert!(!result.has_next_page);
    assert!(result.next_cursor.is_none());
}

#[tokio::test]
async fn a_following_page_lists_the_people_the_owner_follows() {
    let h = harness();
    a_small_graph(&h).await;

    let result = h
        .service
        .page(FollowDirection::Following, ada(), ada(), &page(10))
        .await
        .expect("the page is read");

    assert_eq!(ids(&result.data), vec![adam(), grace()], "newest first");
    assert!(
        result.data.iter().all(|item| item.is_following),
        "on one's own following list every row is somebody the caller follows"
    );
}

#[tokio::test]
async fn the_row_carries_the_users_public_fields() {
    let h = harness();
    h.repo.seed_user(
        grace(),
        "Grace Hopper",
        Some("compilers"),
        Some("https://cdn/g.png"),
    );
    h.repo.seed_follow(grace(), ada(), instant(1));

    let result = h
        .service
        .page(FollowDirection::Followers, ada(), ada(), &page(10))
        .await
        .expect("the page is read");

    let row = &result.data[0];
    assert_eq!(row.id, grace());
    assert_eq!(row.full_name, "Grace Hopper");
    assert_eq!(row.bio.as_deref(), Some("compilers"));
    assert_eq!(row.avatar_url.as_deref(), Some("https://cdn/g.png"));
    assert_eq!(row.subscribed_at, instant(1));
}

#[tokio::test]
async fn a_user_with_no_followers_is_an_empty_page_not_a_404() {
    // The distinction the folded existence check has to keep: an empty page is the end of a
    // list for a user who exists, and only a user who does not exist is a 404.
    let h = harness();

    let result = h
        .service
        .page(FollowDirection::Followers, grace(), ada(), &page(10))
        .await
        .expect("an existing user with no followers is a page");

    assert!(result.data.is_empty());
    assert!(!result.has_next_page);
}

#[tokio::test]
async fn a_page_for_a_user_who_does_not_exist_is_a_404() {
    let h = harness();

    let error = h
        .service
        .page(
            FollowDirection::Followers,
            Uuid::from_u128(0xff),
            ada(),
            &page(10),
        )
        .await
        .expect_err("there is no such list");

    assert_eq!(error.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn the_owner_is_not_required_to_be_the_viewer() {
    // A stranger's view of somebody else's followers must work, and must annotate the
    // stranger: this is the case `master`'s four methods covered by passing `auth.id` as the
    // viewer and the path's id as the owner. Adam is viewing Adam's own list here, while
    // following Grace — so the two rows of the same page get two different flags.
    let h = harness();
    a_small_graph(&h).await;
    h.repo.seed_follow(adam(), grace(), instant(40));

    let as_owner = h
        .service
        .page(FollowDirection::Followers, adam(), adam(), &page(10))
        .await
        .expect("the page is read");

    let as_stranger = h
        .service
        .page(FollowDirection::Followers, adam(), grace(), &page(10))
        .await
        .expect("the page is read");

    assert_eq!(ids(&as_owner.data), vec![ada(), grace()]);
    assert_eq!(ids(&as_stranger.data), ids(&as_owner.data));
    assert!(!as_owner.data[0].is_following, "Adam does not follow Ada");
    assert!(as_owner.data[1].is_following, "Adam follows Grace");
    assert!(
        as_stranger.data.iter().all(|row| !row.is_following),
        "Grace follows Adam, not Ada or herself"
    );
}

// ── the cursor ────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_cursor_walks_the_whole_list_without_repeating_or_skipping_a_row() {
    // The regression this pins: `master` built its page SQL with the shared helpers but the
    // annotation pass and the keyset both had to agree about which id the cursor carried.
    // A cursor keyed on the wrong side of the join returns page one forever, which looks
    // like a list with two items in it.
    let h = harness();
    for i in 0..5u128 {
        let follower = Uuid::from_u128(0x10 + i);
        h.repo.seed_user(follower, "Follower", None, None);
        h.repo
            .seed_follow(follower, grace(), instant(i64::try_from(i).expect("small")));
    }

    let mut seen: Vec<Uuid> = Vec::new();
    let mut cursor = None;
    let mut pages = 0;
    loop {
        let result = h
            .service
            .page(
                FollowDirection::Followers,
                grace(),
                ada(),
                &CursorParams {
                    cursor: cursor.clone(),
                    limit: Some(2),
                },
            )
            .await
            .expect("a page of the walk");
        pages += 1;
        seen.extend(ids(&result.data));
        if !result.has_next_page {
            assert!(result.next_cursor.is_none(), "no cursor past the last page");
            break;
        }
        cursor = result.next_cursor;
        assert!(pages < 10, "the walk must terminate");
    }

    assert_eq!(pages, 3, "five rows at two per page");
    let unique: std::collections::HashSet<Uuid> = seen.iter().copied().collect();
    assert_eq!(seen.len(), 5, "no row was repeated: {seen:?}");
    assert_eq!(unique.len(), 5, "no row was skipped: {seen:?}");
}

#[tokio::test]
async fn an_undecodable_cursor_is_a_400_invalid_cursor() {
    let h = harness();

    let error = h
        .service
        .page(
            FollowDirection::Followers,
            grace(),
            ada(),
            &CursorParams {
                cursor: Some(meno_core::Cursor("not-a-cursor".to_owned())),
                limit: Some(10),
            },
        )
        .await
        .expect_err("the cursor cannot be decoded");

    assert_eq!(error.code(), ErrorCode::InvalidCursor);
}

#[tokio::test]
async fn a_page_never_carries_more_than_the_requested_size() {
    let h = harness();
    for i in 0..5u128 {
        let follower = Uuid::from_u128(0x10 + i);
        h.repo.seed_user(follower, "Follower", None, None);
        h.repo
            .seed_follow(follower, grace(), instant(i64::try_from(i).expect("small")));
    }

    let result = h
        .service
        .page(FollowDirection::Followers, grace(), ada(), &page(2))
        .await
        .expect("the page is read");

    assert_eq!(result.data.len(), 2, "the probe row is not returned");
    assert!(result.has_next_page);
}
