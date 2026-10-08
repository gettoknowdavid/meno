//! The settings module's state: where concrete adapters are chosen (§5.6).
//!
//! The whole file's job: pool → `PgSettingsRepo`, Redis → `ProfileRedisCache`. Every
//! consumer above sees `Arc<dyn …>` and the module's tests assemble it from the
//! in-memory doubles instead.

use std::sync::Arc;

use sqlx::PgPool;

use super::repository::{PgSettingsRepo, SettingsRepo};
use super::service::SettingsService;
use crate::infrastructure::redis::Redis;
use crate::modules::profile::cache::{ProfileCache, ProfileRedisCache};

/// The settings module as the router sees it.
#[derive(Clone)]
pub struct SettingsState {
    /// The rules. A handler reaches through this and nothing else.
    pub service: Arc<SettingsService>,
}

/// The raw resources [`SettingsState::new`] builds adapters from.
///
/// No object store and no mailer: settings touches one table and one cache key, and
/// the `Wiring` struct says so — an absent integration is a type that cannot be
/// requested, not an `Option` someone forgets to check.
pub struct Wiring {
    /// The application's pool.
    pub pool: PgPool,
    /// The application's Redis, for invalidating `/users/me`'s cached copy.
    pub redis: Redis,
}

impl SettingsState {
    /// Build the module from raw resources.
    #[must_use]
    pub fn new(wiring: Wiring) -> Self {
        let repo: Arc<dyn SettingsRepo> = Arc::new(PgSettingsRepo::new(wiring.pool));
        let profile_cache: Arc<dyn ProfileCache> = Arc::new(ProfileRedisCache::new(wiring.redis));
        Self::from_parts(repo, profile_cache)
    }

    /// Build the module from its trait objects — what `new` funnels into, and what
    /// tests call with the doubles.
    #[must_use]
    pub fn from_parts(repo: Arc<dyn SettingsRepo>, profile_cache: Arc<dyn ProfileCache>) -> Self {
        Self {
            service: Arc::new(SettingsService::new(repo, profile_cache)),
        }
    }
}

impl std::fmt::Debug for SettingsState {
    /// Names the field and stops — the service holds a pool and a Redis handle, whose
    /// URLs carry passwords, so a derived `{:?}` is out of the question (same rule as
    /// `MenoState`'s own `Debug`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsState")
            .field("service", &"[settings rules]")
            .finish()
    }
}
