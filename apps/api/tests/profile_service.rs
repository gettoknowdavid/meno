//! §10 **unit** layer: the profile service, against in-memory doubles.
//!
//! # Why this suite exists
//!
//! `profile_router.rs` asserts on the *wire*; these assert on the *rules*. The two
//! cannot stand in for each other: a handler can render the right envelope around a
//! service that never invalidates its cache, and a service can be correct while the
//! router answers 200 where the contract says 404. What this file proves is the
//! orchestration — read-through caching, write-repairs-itself invalidation, the
//! object-before-row ordering of an avatar upload, and the cursor actually advancing
//! a page — none of which is visible from a single request.
//!
//! # What this cannot prove
//!
//! No Postgres and no Redis run here: `repository/pg.rs` and `cache.rs`'s Redis
//! adapter are covered by `profile_repository_pg.rs` and by the double's contract.
//! The in-memory repository deliberately mirrors the SQL's semantics (COALESCE on
//! update, row-value cursor comparison), so a green run here describes something the
//! SQL does — the pg suite then checks that the SQL is that something.

// Panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml. The allow-*-in-tests keys cover #[test] functions; the helpers below
// are not #[test] functions, so the allowance does not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use meno_api::infrastructure::storage::{
    InMemoryObjectStore, NoopObjectStore, ObjectKey, ObjectStore,
};
use meno_api::modules::profile::cache::{InMemoryProfileCache, ProfileCache};
use meno_api::modules::profile::dto::{ProfileSearchQuery, UpdateProfileRequest};
use meno_api::modules::profile::model::Profile;
use meno_api::modules::profile::repository::{InMemoryProfileRepo, ProfileRepo};
use meno_api::modules::profile::service::{ProfileDeps, ProfileService};
use meno_api::modules::profile::storage::ProfileStorage;
use meno_api::modules::settings::repository::{InMemorySettingsRepo, SettingsRepo};
use meno_core::{CursorParams, Error as MenoError, ErrorCode};
use time::OffsetDateTime;
use uuid::Uuid;

// ── fixtures ───────────────────────────────────────────────────────────────

/// The service with its doubles still concrete, so tests can assert underneath it.
struct Harness {
    service: ProfileService,
    repo: Arc<InMemoryProfileRepo>,
    cache: Arc<InMemoryProfileCache>,
}

fn harness() -> Harness {
    harness_with_store(Arc::new(InMemoryObjectStore::new("https://cdn.test")))
}

fn harness_with_store(store: Arc<dyn ObjectStore>) -> Harness {
    let repo = Arc::new(InMemoryProfileRepo::new());
    let settings = Arc::new(InMemorySettingsRepo::new());
    let cache = Arc::new(InMemoryProfileCache::new());

    let service = ProfileService::new(ProfileDeps {
        repo: Arc::clone(&repo) as Arc<dyn ProfileRepo>,
        settings_repo: Arc::clone(&settings) as Arc<dyn SettingsRepo>,
        cache: Arc::clone(&cache) as Arc<dyn ProfileCache>,
        storage: ProfileStorage::new(store),
    });

    Harness {
        service,
        repo,
        cache,
    }
}

/// A deterministic instant for pagination fixtures — far enough in the past that no
/// clock arithmetic in the code can move a row across a page boundary.
fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(seconds).expect("a plausible fixture instant")
}

fn profile(id: Uuid, full_name: &str, created_at: OffsetDateTime) -> Profile {
    Profile {
        id,
        full_name: full_name.to_owned(),
        email: format!("{full_name}@example.com"),
        verified: true,
        bio: Some("Notes on the analytical engine".to_owned()),
        avatar_id: None,
        avatar_url: None,
        followers: 3,
        following: 1,
        broadcasts: 2,
        created_at,
    }
}

fn ada() -> Uuid {
    Uuid::from_u128(1)
}
fn adam() -> Uuid {
    Uuid::from_u128(2)
}

fn patch(full_name: Option<&str>, bio: Option<&str>) -> UpdateProfileRequest {
    UpdateProfileRequest {
        full_name: full_name.map(str::to_owned),
        bio: bio.map(str::to_owned),
    }
}

fn query(q: &str, limit: i64, cursor: Option<meno_core::Cursor>) -> ProfileSearchQuery {
    ProfileSearchQuery {
        q: q.to_owned(),
        pagination: CursorParams {
            cursor,
            limit: Some(limit),
        },
    }
}

/// A real PNG header — what `sniff` accepts.
fn png() -> Vec<u8> {
    [
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01, 0x42,
    ]
    .to_vec()
}

/// A real JPEG header.
fn jpg() -> Vec<u8> {
    [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10].to_vec()
}

// ── GET /users/me ──────────────────────────────────────────────────────────

#[tokio::test]
async fn me_embeds_defaults_for_an_account_that_never_saved_settings() {
    let h = harness();
    h.repo
        .insert_user(profile(ada(), "Ada Lovelace", at(1_700_000_000)));
    h.repo.set_providers(
        ada(),
        vec![meno_api::middleware::auth::AuthProvider::Password],
    );

    let me = h
        .service
        .get_me(ada())
        .await
        .expect("the fixture profile loads");

    assert_eq!(me.full_name, "Ada Lovelace");
    assert!(
        me.email_verified,
        "`users.verified` maps to `email_verified`"
    );
    assert_eq!(
        me.followers, 3,
        "counters come from the columns, not from zeros"
    );
    assert_eq!(me.following, 1);
    assert_eq!(me.broadcasts, 2);

    // The embedded block is the defaults, not a 404 and not an error.
    assert!(me.settings.push_notifications);
    assert!(me.settings.app_notifications);
    assert!(me.settings.email_notifications);
    assert_eq!(me.settings.language, "en");
    assert_eq!(
        me.settings.display,
        meno_api::modules::settings::model::Display::System
    );
    assert!(!me.settings.has_push_token);

    assert_eq!(
        me.providers,
        vec![meno_api::middleware::auth::AuthProvider::Password]
    );
}

#[tokio::test]
async fn me_is_cached_and_a_write_repairs_the_cache_in_the_same_breath() {
    let h = harness();
    h.repo
        .insert_user(profile(ada(), "Ada Lovelace", at(1_700_000_000)));

    let first = h.service.get_me(ada()).await.expect("the first read runs");
    assert_eq!(first.full_name, "Ada Lovelace");
    assert!(h.cache.get(ada()).is_some(), "a cold read fills the cache");

    // The repository now says something else. A read-through cache that is working
    // still answers the old value — that is what a cache *is*.
    h.repo
        .insert_user(profile(ada(), "Ada Byron", at(1_700_000_000)));
    let stale = h.service.get_me(ada()).await.expect("the second read runs");
    assert_eq!(stale.full_name, "Ada Lovelace", "still the cached copy");

    // The write path does not merely invalidate: it re-reads and overwrites, so the
    // answer to the *write's* own response is fresh and so is the next read.
    let updated = h
        .service
        .update_me(ada(), &patch(Some("Ada King"), None))
        .await
        .expect("the update runs");
    assert_eq!(updated.full_name, "Ada King");

    let after = h.service.get_me(ada()).await.expect("the third read runs");
    assert_eq!(
        after.full_name, "Ada King",
        "the cache was repaired, not just dropped"
    );
}

#[tokio::test]
async fn me_404s_for_an_unknown_or_deleted_account() {
    let h = harness();

    let error = h
        .service
        .get_me(Uuid::from_u128(99))
        .await
        .expect_err("there is no such profile");
    assert_eq!(error.code(), ErrorCode::NotFound, "{error}");
    assert_eq!(meno_core::to_body(&error).http_status, 404);
}

#[tokio::test]
async fn a_failing_cache_falls_back_to_the_database() {
    // The policy under test: a cache *read* error is a miss, not an outage.
    struct BrokenCache;

    #[async_trait::async_trait]
    impl ProfileCache for BrokenCache {
        async fn cache_me(
            &self,
            _value: meno_api::modules::profile::dto::MeResponse,
        ) -> Result<(), MenoError> {
            Err(MenoError::Internal {
                context: "broken_cache",
                detail: "redis is down".to_owned(),
            })
        }
        async fn get_cached_me(
            &self,
            _user_id: Uuid,
        ) -> Result<Option<meno_api::modules::profile::dto::MeResponse>, MenoError> {
            Err(MenoError::Internal {
                context: "broken_cache",
                detail: "redis is down".to_owned(),
            })
        }
        async fn invalidate_cached_profile(&self, _user_id: Uuid) -> Result<(), MenoError> {
            Err(MenoError::Internal {
                context: "broken_cache",
                detail: "redis is down".to_owned(),
            })
        }
    }

    let repo = Arc::new(InMemoryProfileRepo::new());
    let settings = Arc::new(InMemorySettingsRepo::new());
    repo.insert_user(profile(ada(), "Ada Lovelace", at(1_700_000_000)));

    let service = ProfileService::new(ProfileDeps {
        repo: Arc::clone(&repo) as Arc<dyn ProfileRepo>,
        settings_repo: Arc::clone(&settings) as Arc<dyn SettingsRepo>,
        cache: Arc::new(BrokenCache),
        storage: ProfileStorage::new(Arc::new(InMemoryObjectStore::new("https://cdn.test"))),
    });

    let me = service
        .get_me(ada())
        .await
        .expect("availability wins over freshness");
    assert_eq!(me.full_name, "Ada Lovelace");

    // And a write still succeeds: the cache failing to invalidate is logged, not
    // raised — the database is already right.
    let updated = service
        .update_me(ada(), &patch(Some("Ada King"), None))
        .await
        .expect("the write does not depend on Redis");
    assert_eq!(updated.full_name, "Ada King");
}

// ── PATCH /users/me ────────────────────────────────────────────────────────

#[tokio::test]
async fn a_patch_only_touches_the_fields_it_names() {
    let h = harness();
    h.repo
        .insert_user(profile(ada(), "Ada Lovelace", at(1_700_000_000)));
    let original_bio = h.repo.user(ada()).expect("the fixture row").bio;

    // `full_name` only: the bio must survive — the COALESCE rule, asserted through
    // the service so a future repository cannot quietly blank omitted fields.
    let me = h
        .service
        .update_me(ada(), &patch(Some("Ada Byron"), None))
        .await
        .expect("the update runs");
    assert_eq!(me.full_name, "Ada Byron");
    assert_eq!(
        me.bio, original_bio,
        "an omitted field is not a cleared field"
    );

    // An explicit empty string *is* a clear: the wire's one way to say "none".
    let me = h
        .service
        .update_me(ada(), &patch(None, Some("")))
        .await
        .expect("the update runs");
    assert_eq!(
        me.bio.as_deref(),
        Some(""),
        "an empty string clears the bio"
    );

    let row = h.repo.user(ada()).expect("the row is still there");
    assert_eq!(row.full_name, "Ada Byron");
    assert_eq!(row.bio.as_deref(), Some(""));
}

#[tokio::test]
async fn updating_a_missing_profile_is_a_404() {
    let h = harness();

    let error = h
        .service
        .update_me(Uuid::from_u128(99), &patch(Some("Ghost"), None))
        .await
        .expect_err("there is nothing to update");
    assert_eq!(error.code(), ErrorCode::NotFound, "{error}");
}

// ── GET /users/{id} ────────────────────────────────────────────────────────

#[tokio::test]
async fn a_public_profile_reports_whether_the_viewer_follows_and_hides_the_email() {
    let h = harness();
    h.repo
        .insert_user(profile(ada(), "Ada Lovelace", at(1_700_000_000)));
    h.repo
        .insert_user(profile(adam(), "Adam Smith", at(1_700_000_100)));
    h.repo.follow(adam(), ada()); // Adam follows Ada

    let as_adam = h
        .service
        .get_profile(adam(), ada())
        .await
        .expect("the lookup runs");
    assert!(as_adam.is_following);
    assert_eq!(as_adam.followers, 3);

    let as_stranger = h
        .service
        .get_profile(Uuid::from_u128(7), ada())
        .await
        .expect("the lookup runs");
    assert!(!as_stranger.is_following, "a stranger follows nobody");

    // And the type itself carries no email field to leak — this asserts the wire
    // struct, not the row.
    let json = serde_json::to_value(&as_adam).expect("the response serialises");
    assert!(
        json.get("email").is_none(),
        "a public profile never exposes it: {json}"
    );
    assert!(json.get("settings").is_none(), "{json}");
}

#[tokio::test]
async fn a_missing_profile_is_a_404() {
    let h = harness();

    let error = h
        .service
        .get_profile(ada(), Uuid::from_u128(99))
        .await
        .expect_err("there is no such profile");
    assert_eq!(error.code(), ErrorCode::NotFound, "{error}");
}

// ── GET /users?q= ──────────────────────────────────────────────────────────

#[tokio::test]
async fn search_pages_forward_with_a_cursor_that_actually_advances() {
    // The regression this suite exists for: `master`'s search never applied its
    // cursor (`rank > $n` with `$n` unbound), so every page was page one. Here the
    // second request must return a *different*, older row — and no overlap.
    let h = harness();
    h.repo
        .insert_user(profile(ada(), "Ada Lovelace", at(1_700_000_100))); // newer
    h.repo
        .insert_user(profile(adam(), "Adam Smith", at(1_700_000_000))); // older
    h.repo.insert_user(profile(
        Uuid::from_u128(3),
        "Grace Hopper",
        at(1_700_000_200),
    )); // newer still, but does not match `ada`

    let viewer = Uuid::from_u128(9);
    h.repo.follow(viewer, ada());

    let page_one = h
        .service
        .search(viewer, &query("ada", 1, None))
        .await
        .expect("the first page runs");
    assert_eq!(page_one.data.len(), 1, "one row per page");
    assert!(page_one.has_next_page, "another match exists");
    let cursor = page_one.next_cursor.expect("a cursor to the next page");
    assert_eq!(page_one.data[0].full_name, "Ada Lovelace", "newest first");
    assert!(
        page_one.data[0].is_following,
        "the viewer's follow state rides along"
    );

    let page_two = h
        .service
        .search(viewer, &query("ada", 1, Some(cursor)))
        .await
        .expect("the second page runs");
    assert_eq!(page_two.data.len(), 1);
    assert_eq!(
        page_two.data[0].full_name, "Adam Smith",
        "the cursor advanced"
    );
    assert!(!page_two.has_next_page, "that was the last match");
    assert!(
        page_two.data[0].id != page_one.data[0].id,
        "pages must not overlap — a repeated row is the dead-cursor bug in disguise"
    );
}

#[tokio::test]
async fn search_skips_everything_that_does_not_match() {
    let h = harness();
    h.repo
        .insert_user(profile(ada(), "Ada Lovelace", at(1_700_000_100)));
    h.repo.insert_user(profile(
        Uuid::from_u128(3),
        "Grace Hopper",
        at(1_700_000_200),
    ));

    let page = h
        .service
        .search(ada(), &query("lovelace", 20, None))
        .await
        .expect("the search runs");
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].full_name, "Ada Lovelace");
    assert!(!page.has_next_page);

    let none = h
        .service
        .search(ada(), &query("zzzz", 20, None))
        .await
        .expect("an empty result is not an error");
    assert!(none.data.is_empty());
    assert!(!none.has_next_page);
    assert!(none.next_cursor.is_none());
}

// ── POST /users/me/avatar ──────────────────────────────────────────────────

#[tokio::test]
async fn an_avatar_upload_stores_replaces_and_invalidates() {
    let ada = ada();
    let store = Arc::new(InMemoryObjectStore::new("https://cdn.test"));
    let h = harness_with_store(store.clone());
    h.repo
        .insert_user(profile(ada, "Ada Lovelace", at(1_700_000_000)));

    // Warm the cache so the invalidation at the end has something to remove.
    h.service.get_me(ada).await.expect("the warm-up read runs");
    assert!(h.cache.get(ada).is_some());

    let first = h
        .service
        .upload_avatar(ada, &png())
        .await
        .expect("a PNG is accepted");
    let expected_key = format!("avatars/{ada}.png");
    assert_eq!(first.avatar_id, expected_key);
    assert_eq!(
        first.avatar_url.as_deref(),
        Some(format!("https://cdn.test/{expected_key}").as_str()),
        "the URL comes from the store's public base"
    );
    assert_eq!(
        h.repo.user(ada).expect("the row").avatar_id.as_deref(),
        Some(expected_key.as_str())
    );

    // A different extension means a different key: the old object must go, or every
    // avatar replacement leaks a blob.
    let second = h
        .service
        .upload_avatar(ada, &jpg())
        .await
        .expect("a JPEG is accepted");
    assert_eq!(second.avatar_id, format!("avatars/{ada}.jpg"));

    let old = ObjectKey::avatar(ada, "png").expect("the old key");
    let new = ObjectKey::avatar(ada, "jpg").expect("the new key");
    assert!(
        !store.exists(&old).await.expect("the store answers"),
        "the replaced object is deleted"
    );
    assert!(
        store.exists(&new).await.expect("the store answers"),
        "the new object is there"
    );
    assert_eq!(
        h.repo.user(ada).expect("the row").avatar_id.as_deref(),
        Some(second.avatar_id.as_str())
    );
    assert!(
        h.cache.get(ada).is_none(),
        "the cached /users/me still carries the old avatar and must be dropped"
    );
}

#[tokio::test]
async fn an_avatar_is_validated_before_a_byte_is_written() {
    let h = harness();
    h.repo
        .insert_user(profile(ada(), "Ada Lovelace", at(1_700_000_000)));

    let error = h
        .service
        .upload_avatar(ada(), b"<html>sign in to continue</html>")
        .await
        .expect_err("an HTML page is not an avatar");
    let MenoError::Validation { fields } = &error else {
        panic!("a bad file is a field failure, got {error}");
    };
    assert!(fields.contains_key("file"), "{fields:?}");

    // Oversize, but with a valid header — so the *size* rule is what answers, not
    // the sniff. This pins the order of the two checks.
    let mut huge = vec![0u8; meno_api::modules::profile::storage::MAX_AVATAR_BYTES + 1];
    huge[..3].copy_from_slice(&[0xFF, 0xD8, 0xFF]);
    let error = h
        .service
        .upload_avatar(ada(), &huge)
        .await
        .expect_err("five megabytes plus one is too many");
    let MenoError::Validation { fields } = &error else {
        panic!("an oversized file is a field failure, got {error}");
    };
    assert!(
        fields["file"][0].contains("at most"),
        "the size rule fired first: {:?}",
        fields["file"]
    );
    assert!(
        h.repo.user(ada()).expect("the row").avatar_id.is_none(),
        "nothing was written for either refusal"
    );
}

#[tokio::test]
async fn an_avatar_upload_with_storage_disabled_is_a_503() {
    // `STORAGE_ENABLED=false` is a deployment, not a compile error (§4.6): the
    // NoopObjectStore refuses every write, and the client gets `UPSTREAM_UNAVAILABLE`
    // rather than a panic or a success that stored nothing.
    let h = harness_with_store(Arc::new(NoopObjectStore));
    h.repo
        .insert_user(profile(ada(), "Ada Lovelace", at(1_700_000_000)));

    let error = h
        .service
        .upload_avatar(ada(), &png())
        .await
        .expect_err("a disabled store cannot accept an upload");
    assert_eq!(error.code(), ErrorCode::UpstreamUnavailable, "{error}");
    assert_eq!(meno_core::to_body(&error).http_status, 503);
    assert!(
        h.repo.user(ada()).expect("the row").avatar_id.is_none(),
        "the row never points at an object that was not written"
    );
}

#[tokio::test]
async fn an_avatar_upload_for_a_missing_account_is_a_404() {
    let h = harness();

    let error = h
        .service
        .upload_avatar(Uuid::from_u128(99), &png())
        .await
        .expect_err("there is no account to attach it to");
    assert_eq!(error.code(), ErrorCode::NotFound, "{error}");
}
