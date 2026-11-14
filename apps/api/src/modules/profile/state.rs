//! The profile module's state: where concrete adapters are chosen (§5.6).
//!
//! This file — and only this file — knows that the repository is Postgres, that the
//! cache is Redis, and that the object store came from `MenoState`. Everything above
//! it sees `Arc<dyn …>` traits, so a test can assemble the whole module from
//! in-memory doubles without a Docker in sight.

use std::sync::Arc;

use sqlx::PgPool;

use super::cache::{ProfileCache, ProfileRedisCache};
use super::repository::{PgProfileRepo, ProfileRepo};
use super::service::{ProfileDeps, ProfileService};
use super::storage::ProfileStorage;
use crate::infrastructure::redis::Redis;
use crate::infrastructure::storage::ObjectStore;
use crate::modules::settings::repository::{PgSettingsRepo, SettingsRepo};

/// The profile module as the router sees it.
#[derive(Clone)]
pub struct ProfileState {
    /// The rules. A handler reaches through this and nothing else.
    pub service: Arc<ProfileService>,
}

/// The raw resources [`ProfileState::new`] builds adapters from.
///
/// The same shape as `broadcast::state::Wiring`: a pool, a Redis and the optional
/// integration (here the object store, §4.6) — so wiring the module is choosing
/// adapters, not passing one in.
pub struct Wiring {
    /// The application's pool.
    pub pool: PgPool,
    /// The application's Redis, for the `/users/me` cache.
    pub redis: Redis,
    /// The object store — S3 when enabled, `NoopObjectStore` when not (§4.6).
    pub storage: Arc<dyn ObjectStore>,
}

impl ProfileState {
    /// Build the module from raw resources: Postgres, Redis and the object store.
    ///
    /// The `SettingsRepository` is built here as well because `/users/me` embeds the
    /// settings block, and reaching for `settings`' state from this file keeps the
    /// dependency a *construction* detail rather than a runtime one.
    #[must_use]
    pub fn new(wiring: Wiring) -> Self {
        let repo: Arc<dyn ProfileRepo> = Arc::new(PgProfileRepo::new(wiring.pool.clone()));
        let settings_repo: Arc<dyn SettingsRepo> = Arc::new(PgSettingsRepo::new(wiring.pool));
        let cache: Arc<dyn ProfileCache> = Arc::new(ProfileRedisCache::new(wiring.redis));
        let storage = ProfileStorage::new(wiring.storage);

        Self::from_parts(repo, settings_repo, cache, storage)
    }

    /// Build the module from its trait objects — what `new` funnels into, and what
    /// tests call with the in-memory doubles.
    #[must_use]
    pub fn from_parts(
        repo: Arc<dyn ProfileRepo>,
        settings_repo: Arc<dyn SettingsRepo>,
        cache: Arc<dyn ProfileCache>,
        storage: ProfileStorage,
    ) -> Self {
        Self {
            service: Arc::new(ProfileService::new(ProfileDeps {
                repo,
                settings_repo,
                cache,
                storage,
            })),
        }
    }
}

impl std::fmt::Debug for ProfileState {
    /// Names the field and stops.
    ///
    /// The service holds a pool and a Redis handle — URLs with passwords in them —
    /// and a derived `{:?}` would print them. `MenoState`'s `Debug` embeds this one,
    /// so the rule is the same as it is there: shape, never contents.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfileState")
            .field("service", &"[profile rules]")
            .finish()
    }
}
