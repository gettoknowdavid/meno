//! §10 **unit** layer: the settings service — and the one behaviour that crosses
//! module boundaries, the invalidation of `/users/me`'s cached copy.
//!
//! # The cross-module test is the point
//!
//! Settings is three thin methods on its own; what makes it worth a suite is that
//! `GET /users/me` embeds its data *behind a cache*. A user who saves a setting and
//! then reloads their profile must see it — which means `settings::update` failing to
//! invalidate `profile`'s cache is a real, user-visible bug that neither module's own
//! tests would catch. The fixture below wires both services to one shared cache for
//! exactly that reason.
//!
//! # What this cannot prove
//!
//! The upsert's `COALESCE` branches are SQL. The in-memory double mirrors them
//! (`Settings::new` for the INSERT branch, stored values for the UPDATE branch), so
//! these tests describe what the SQL must do — `profile_repository_pg.rs` then runs
//! the real statement against a real database and checks that it does.

// Panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml. The allow-*-in-tests keys cover #[test] functions; the helpers below
// are not #[test] functions, so the allowance does not reach them.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use meno_api::infrastructure::storage::InMemoryObjectStore;
use meno_api::modules::profile::cache::{InMemoryProfileCache, ProfileCache};
use meno_api::modules::profile::model::Profile;
use meno_api::modules::profile::repository::{InMemoryProfileRepo, ProfileRepo};
use meno_api::modules::profile::service::{ProfileDeps, ProfileService};
use meno_api::modules::profile::storage::ProfileStorage;
use meno_api::modules::settings::dto::UpdateSettingsRequest;
use meno_api::modules::settings::model::Display;
use meno_api::modules::settings::repository::{InMemorySettingsRepo, SettingsRepo};
use meno_api::modules::settings::service::SettingsService;
use time::OffsetDateTime;
use uuid::Uuid;

// ── fixtures ───────────────────────────────────────────────────────────────

/// Both services, one cache — the wiring `MenoState::build` produces.
struct Harness {
    settings: SettingsService,
    profile: ProfileService,
    rows: Arc<InMemorySettingsRepo>,
    cache: Arc<InMemoryProfileCache>,
}

fn harness() -> Harness {
    let rows = Arc::new(InMemorySettingsRepo::new());
    let profiles = Arc::new(InMemoryProfileRepo::new());
    let cache = Arc::new(InMemoryProfileCache::new());

    let settings = SettingsService::new(
        Arc::clone(&rows) as Arc<dyn SettingsRepo>,
        Arc::clone(&cache) as Arc<dyn ProfileCache>,
    );
    let profile = ProfileService::new(ProfileDeps {
        repo: Arc::clone(&profiles) as Arc<dyn ProfileRepo>,
        settings_repo: Arc::clone(&rows) as Arc<dyn SettingsRepo>,
        cache: Arc::clone(&cache) as Arc<dyn ProfileCache>,
        storage: ProfileStorage::new(Arc::new(InMemoryObjectStore::new("https://cdn.test"))),
    });

    // Seed the account `/users/me` will read.
    profiles.insert_user(Profile {
        id: ada(),
        full_name: "Ada Lovelace".to_owned(),
        email: "ada@example.com".to_owned(),
        verified: true,
        bio: None,
        avatar_id: None,
        avatar_url: None,
        followers: 0,
        following: 0,
        broadcasts: 0,
        created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000)
            .expect("a plausible fixture instant"),
    });

    Harness {
        settings,
        profile,
        rows,
        cache,
    }
}

fn ada() -> Uuid {
    Uuid::from_u128(1)
}

/// A patch that sets `language` — and nothing else.
fn language(tag: &str) -> UpdateSettingsRequest {
    UpdateSettingsRequest {
        push_notifications: None,
        app_notifications: None,
        email_notifications: None,
        push_notification_token: None,
        display: None,
        language: Some(tag.to_owned()),
        notification_preferences: None,
    }
}

fn token(value: &str) -> UpdateSettingsRequest {
    UpdateSettingsRequest {
        push_notifications: None,
        app_notifications: None,
        email_notifications: None,
        push_notification_token: Some(value.to_owned()),
        display: None,
        language: None,
        notification_preferences: None,
    }
}

// ── GET /settings ──────────────────────────────────────────────────────────

#[tokio::test]
async fn get_answers_with_the_defaults_when_no_row_exists() {
    // `master` answered `SETTINGS_NOT_FOUND` here — for every user who had never
    // saved a setting, i.e. every user, because nothing inserts the row at signup.
    let h = harness();

    let settings = h
        .settings
        .get(ada())
        .await
        .expect("a default is not an error");
    assert!(settings.push_notifications);
    assert!(settings.app_notifications);
    assert!(settings.email_notifications);
    assert_eq!(settings.language, "en");
    assert_eq!(settings.display, Display::System);
    assert!(!settings.has_push_token);
    assert!(h.rows.row(ada()).is_none(), "a read does not create a row");
}

#[tokio::test]
async fn a_stored_row_round_trips_and_the_token_never_leaves_the_server() {
    let h = harness();

    h.settings
        .update(ada(), &token("fcm-device-token"))
        .await
        .expect("the save runs");

    let settings = h.settings.get(ada()).await.expect("the read runs");
    assert!(settings.has_push_token, "presence is reported");

    // The raw token is a credential: `SettingsResponse` has no field for it, and
    // this asserts the serialised form rather than the struct — a `#[serde(flatten)]`
    // added later would otherwise smuggle it out silently.
    let json = serde_json::to_value(&settings).expect("the response serialises");
    assert!(
        json.get("push_notification_token").is_none(),
        "the device token must not appear on the wire: {json}"
    );

    h.settings
        .clear_push_token(ada())
        .await
        .expect("the clear runs");
    let settings = h.settings.get(ada()).await.expect("the read runs");
    assert!(!settings.has_push_token);
}

// ── PATCH /settings ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_first_save_persists_the_same_defaults_get_had_shown() {
    // The COALESCE-INSERT rule: a first patch that only names `language` must not
    // flip the untouched toggles to the migration's column defaults (`false`) —
    // what the client saw a moment before the save is what it sees after.
    let h = harness();

    let before = h.settings.get(ada()).await.expect("the defaults");
    assert!(before.push_notifications);

    let saved = h
        .settings
        .update(ada(), &language("fr"))
        .await
        .expect("the save runs");
    assert_eq!(saved.language, "fr");
    assert!(
        saved.push_notifications,
        "an unmentioned field keeps its value"
    );
    assert!(saved.app_notifications);
    assert!(saved.email_notifications);

    let row = h.rows.row(ada()).expect("the first save created the row");
    assert_eq!(row.language, "fr");
    assert!(
        row.push_notifications,
        "the INSERT branch defaults to `Settings::new`"
    );
}

#[tokio::test]
async fn a_second_patch_only_touches_the_fields_it_names() {
    let h = harness();
    h.settings
        .update(ada(), &language("fr"))
        .await
        .expect("the first save");

    let mut second = language("de");
    second.push_notifications = Some(false);
    let saved = h
        .settings
        .update(ada(), &second)
        .await
        .expect("the second save");

    assert_eq!(saved.language, "de");
    assert!(
        !saved.push_notifications,
        "the field the patch named changed"
    );
    assert!(saved.app_notifications, "the field it did not name did not");
}

// ── the cross-module contract ──────────────────────────────────────────────

#[tokio::test]
async fn saving_settings_invalidates_the_embedded_block_on_users_me() {
    let h = harness();

    // 1. The profile screen loads: `/users/me` is built and cached with the
    //    defaults embedded.
    let me = h.profile.get_me(ada()).await.expect("the profile loads");
    assert_eq!(me.settings.language, "en");
    assert!(h.cache.get(ada()).is_some(), "the payload is cached");

    // 2. The settings screen saves a change.
    h.settings
        .update(ada(), &language("fr"))
        .await
        .expect("the save runs");
    assert!(
        h.cache.get(ada()).is_none(),
        "the stale copy is dropped — a user must see their own change"
    );

    // 3. The next profile read rebuilds from the database.
    let me = h.profile.get_me(ada()).await.expect("the profile reloads");
    assert_eq!(me.settings.language, "fr");

    // 4. And clearing the push token lands the same way.
    h.settings
        .update(ada(), &token("fcm-device-token"))
        .await
        .expect("the save runs");
    assert!(
        h.profile
            .get_me(ada())
            .await
            .expect("the reload")
            .settings
            .has_push_token
    );

    h.settings
        .clear_push_token(ada())
        .await
        .expect("the clear runs");
    assert!(
        !h.profile
            .get_me(ada())
            .await
            .expect("the reload")
            .settings
            .has_push_token
    );
}

#[tokio::test]
async fn clearing_a_token_that_was_never_set_succeeds_silently() {
    // Idempotent by contract: a client retrying a DELETE after a dropped response
    // must get the same success, not a 404 it cannot distinguish from a bug.
    let h = harness();

    h.settings
        .clear_push_token(ada())
        .await
        .expect("clearing nothing is still the requested state");
    let settings = h.settings.get(ada()).await.expect("the read runs");
    assert!(!settings.has_push_token);
}
