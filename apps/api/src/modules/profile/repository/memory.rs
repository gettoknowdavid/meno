//! The in-memory double of [`ProfileRepo`] (§10).
//!
//! Mirrors the SQL semantics rather than merely storing values, because a double that
//! is *more* permissive than the database hides exactly the bugs it exists to catch:
//! `update_profile` ignores `None` the way `COALESCE` does, `find_by_id` never
//! returns a row the adapter would have filtered, and `search` applies the cursor as
//! the row-value comparison would.
//!
//! One deliberate approximation: matching is a case-insensitive substring test,
//! where Postgres uses `to_tsvector`/`plainto_tsquery` (stemming, whole tokens). The
//! substring is a *superset* — everything the SQL matches, this matches — so a
//! fixture that pages correctly here pages correctly there; the reverse direction,
//! and the token semantics themselves, are `profile_repository_pg.rs`'s job.
//!
//! Locking follows `broadcast`'s double: a `std::sync::Mutex` poisoned only by a
//! panic while holding it, recovered with `into_inner` — the guard is never held
//! across an `.await`, so a synchronous lock is correct here and cheaper.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

use uuid::Uuid;

use super::{ProfileRepo, SearchFilter};
use crate::middleware::auth::AuthProvider;
use crate::modules::profile::dto::ProfileSearchResult;
use crate::modules::profile::error;
use crate::modules::profile::model::Profile;
use meno_core::Error as MenoError;

#[derive(Debug, Default)]
struct State {
    users: HashMap<Uuid, Profile>,
    providers: HashMap<Uuid, Vec<AuthProvider>>,
    /// `(subscriber_id, subscription_id)` — the `user_subscribers` key columns.
    following: HashSet<(Uuid, Uuid)>,
}

/// The double the profile suite (and `profile_router.rs`) runs against.
#[derive(Debug, Default)]
pub struct InMemoryProfileRepo {
    state: Mutex<State>,
}

impl InMemoryProfileRepo {
    /// An empty repository.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a profile — the fixture's way of seeding a user.
    pub fn insert_user(&self, profile: Profile) {
        self.lock().users.insert(profile.id, profile);
    }

    /// Replace the account's linked providers.
    pub fn set_providers(&self, user_id: Uuid, providers: Vec<AuthProvider>) {
        self.lock().providers.insert(user_id, providers);
    }

    /// Record that `subscriber_id` follows `subscription_id`.
    pub fn follow(&self, subscriber_id: Uuid, subscription_id: Uuid) {
        self.lock()
            .following
            .insert((subscriber_id, subscription_id));
    }

    /// A stored profile, for assertions.
    #[must_use]
    pub fn user(&self, user_id: Uuid) -> Option<Profile> {
        self.lock().users.get(&user_id).cloned()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait::async_trait]
impl ProfileRepo for InMemoryProfileRepo {
    async fn find_by_id(&self, user_id: Uuid) -> Result<Option<Profile>, MenoError> {
        Ok(self.lock().users.get(&user_id).cloned())
    }

    async fn find_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, MenoError> {
        Ok(self
            .lock()
            .providers
            .get(&user_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn find_avatar_key(&self, user_id: Uuid) -> Result<Option<String>, MenoError> {
        Ok(self
            .lock()
            .users
            .get(&user_id)
            .and_then(|p| p.avatar_id.clone()))
    }

    async fn update_profile(
        &self,
        user_id: Uuid,
        full_name: Option<&str>,
        bio: Option<&str>,
    ) -> Result<Profile, MenoError> {
        let mut state = self.lock();
        let profile = state.users.get_mut(&user_id).ok_or_else(error::not_found)?;

        // COALESCE semantics: `None` leaves the column alone.
        if let Some(name) = full_name {
            profile.full_name = name.to_owned();
        }
        if let Some(bio) = bio {
            profile.bio = Some(bio.to_owned());
        }
        Ok(profile.clone())
    }

    async fn set_avatar(
        &self,
        user_id: Uuid,
        avatar_id: &str,
        avatar_url: Option<&str>,
    ) -> Result<(), MenoError> {
        let mut state = self.lock();
        let profile = state.users.get_mut(&user_id).ok_or_else(error::not_found)?;
        profile.avatar_id = Some(avatar_id.to_owned());
        profile.avatar_url = avatar_url.map(str::to_owned);
        Ok(())
    }

    async fn is_following(&self, viewer_id: Uuid, user_id: Uuid) -> Result<bool, MenoError> {
        Ok(self.lock().following.contains(&(viewer_id, user_id)))
    }

    async fn search(
        &self,
        filter: SearchFilter<'_>,
    ) -> Result<Vec<ProfileSearchResult>, MenoError> {
        let state = self.lock();
        let needle = filter.query.trim().to_lowercase();

        let mut rows: Vec<ProfileSearchResult> = state
            .users
            .values()
            .filter(|p| {
                p.full_name.to_lowercase().contains(&needle)
                    || p.bio
                        .as_deref()
                        .is_some_and(|bio| bio.to_lowercase().contains(&needle))
            })
            // The row-value comparison, in tuple form: strictly older than the
            // cursor under a DESC page, i.e. `(created_at, id) < (ts, id)`.
            .filter(|p| match filter.cursor {
                None => true,
                Some((ts, id)) => (p.created_at, p.id) < (ts, id),
            })
            .map(|p| ProfileSearchResult {
                id: p.id,
                full_name: p.full_name.clone(),
                bio: p.bio.clone(),
                avatar_url: p.avatar_url.clone(),
                is_following: state.following.contains(&(filter.viewer_id, p.id)),
                followers: p.followers,
                following: p.following,
                broadcasts: p.broadcasts,
                created_at: p.created_at,
            })
            .collect();

        // Same total order as the SQL: created_at DESC, then id DESC.
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
        rows.truncate(filter.limit_plus_one.max(0) as usize);
        Ok(rows)
    }
}
