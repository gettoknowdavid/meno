//! The profile rules, as orchestration (§5.1's `service` row).
//!
//! # Thin, in one direction
//!
//! Everything here is "in what order things happen": read the cache, on a miss read
//! the row, embed the settings, write back. The decisions that belong to the wire —
//! is this body valid, which HTTP status — are already behind us in `dto` and
//! `handlers`, and the decisions that belong to storage — which SQL, which key — are
//! ahead of us in `repository` and `storage`.
//!
//! # The cache is a read-through, and writes repair it
//!
//! [`Self::get_me`] serves from cache when it can and rebuilds when it cannot. The
//! write paths do not "invalidate and hope": [`Self::update_me`] re-reads and
//! re-writes the entry, so the cache is correct the moment the response is, and a
//! Redis that was down during the `SET` still has the old value bounded by
//! `ttl::PROFILE`. A cache *read* failure is a miss (availability over freshness); a
//! cache *write* failure after a mutation is logged, because at that point the
//! database is right and only the next read's speed is wrong.

use std::sync::Arc;

use meno_core::{Cursor, CursorPage, Error as MenoError};
use uuid::Uuid;

use super::cache::ProfileCache;
use super::dto::{
    MeResponse, ProfileSearchQuery, ProfileSearchResult, PublicProfileResponse,
    UpdateProfileRequest, UploadAvatarResponse,
};
use super::error;
use super::repository::{ProfileRepo, SearchFilter};
use super::storage::ProfileStorage;
use crate::modules::settings::model::Settings;
use crate::modules::settings::repository::SettingsRepo;

/// The collaborators the profile service is built from.
///
/// A struct rather than four positional parameters so a caller cannot swap the cache
/// and the repository — both are `Arc<dyn …>` and would type-check either way.
pub struct ProfileDeps {
    /// What exists in `users` / `user_subscribers`.
    pub repo: Arc<dyn ProfileRepo>,
    /// Settings rows, for the block `/users/me` embeds.
    pub settings_repo: Arc<dyn SettingsRepo>,
    /// The `/users/me` read-through cache.
    pub cache: Arc<dyn ProfileCache>,
    /// The object store, behind its sniffing wrapper.
    pub storage: ProfileStorage,
}

/// The profile module's rules.
#[derive(Clone)]
pub struct ProfileService {
    repo: Arc<dyn ProfileRepo>,
    settings_repo: Arc<dyn SettingsRepo>,
    cache: Arc<dyn ProfileCache>,
    storage: ProfileStorage,
}

impl ProfileService {
    /// Build from its collaborators. `state.rs` is where the concrete ones come from.
    #[must_use]
    pub fn new(deps: ProfileDeps) -> Self {
        Self {
            repo: deps.repo,
            settings_repo: deps.settings_repo,
            cache: deps.cache,
            storage: deps.storage,
        }
    }

    /// `GET /users/me` — cached, with settings and providers embedded.
    ///
    /// # Errors
    ///
    /// [`error::not_found`] when the account does not exist or is soft-deleted; a
    /// repository failure otherwise. A cache failure is *not* an error: it degrades to
    /// a database read.
    pub async fn get_me(&self, user_id: Uuid) -> Result<MeResponse, MenoError> {
        match self.cache.get_cached_me(user_id).await {
            Ok(Some(hit)) => return Ok(hit),
            Ok(None) => {}
            Err(error) => {
                // A cache that cannot answer is a miss, not an outage.
                tracing::warn!(%error, user_id = %user_id, "profile cache read failed; falling back");
            }
        }
        self.refresh_me(user_id).await
    }

    /// Rebuild the `/users/me` payload from the database and repopulate the cache.
    ///
    /// Also what every write path calls: re-reading and overwriting *is* the
    /// invalidation, and unlike a `DEL` it repairs an entry even when the delete would
    /// have raced with another writer.
    async fn refresh_me(&self, user_id: Uuid) -> Result<MeResponse, MenoError> {
        let me = self.read_me(user_id).await?;
        if let Err(error) = self.cache.cache_me(me.clone()).await {
            tracing::warn!(%error, user_id = %user_id, "profile cache write failed; next read re-reads the database");
        }
        Ok(me)
    }

    /// The uncached `/users/me` read: profile row + settings block + providers.
    async fn read_me(&self, user_id: Uuid) -> Result<MeResponse, MenoError> {
        let profile = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(error::not_found)?;

        // No row yet means the user has never saved settings — the defaults are a
        // read-path answer, not a row. `Settings::new` and the upsert's INSERT branch
        // agree on every value, so what GET shows and what the first PATCH persists
        // are the same thing.
        let settings = self
            .settings_repo
            .find_by_user_id(user_id)
            .await?
            .unwrap_or_else(|| Settings::new(user_id));

        let providers = self.repo.find_providers(user_id).await?;

        Ok(MeResponse {
            id: profile.id,
            full_name: profile.full_name,
            email: profile.email,
            email_verified: profile.verified,
            bio: profile.bio,
            avatar_id: profile.avatar_id,
            avatar_url: profile.avatar_url,
            settings: settings.into(),
            providers,
            followers: profile.followers,
            following: profile.following,
            broadcasts: profile.broadcasts,
            created_at: profile.created_at,
        })
    }

    /// `PATCH /users/me` — persist, then answer with the fresh record.
    ///
    /// The boundary already refused an empty patch (`dto::validate`), so anything
    /// that reaches here names at least one field to change.
    ///
    /// # Errors
    ///
    /// [`error::not_found`] if the account vanished between the token check and the
    /// write; a repository failure otherwise.
    pub async fn update_me(
        &self,
        user_id: Uuid,
        update: &UpdateProfileRequest,
    ) -> Result<MeResponse, MenoError> {
        self.repo
            .update_profile(user_id, update.full_name.as_deref(), update.bio.as_deref())
            .await?;
        // The write re-reads and overwrites the cache entry (see the module docs), so
        // the caller sees its own change immediately rather than after a TTL.
        self.refresh_me(user_id).await
    }

    /// `GET /users/{id}` — another account's public profile.
    ///
    /// No email, no settings, no providers: those exist only for the caller themself,
    /// and this function cannot even construct a type that carries them.
    ///
    /// # Errors
    ///
    /// [`error::not_found`] for a missing or soft-deleted account.
    pub async fn get_profile(
        &self,
        viewer_id: Uuid,
        user_id: Uuid,
    ) -> Result<PublicProfileResponse, MenoError> {
        let profile = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(error::not_found)?;
        let is_following = self.repo.is_following(viewer_id, user_id).await?;

        Ok(PublicProfileResponse {
            id: profile.id,
            full_name: profile.full_name,
            bio: profile.bio,
            avatar_url: profile.avatar_url,
            is_following,
            followers: profile.followers,
            following: profile.following,
            broadcasts: profile.broadcasts,
            created_at: profile.created_at,
        })
    }

    /// `GET /users?q=` — one keyset-paginated page, newest first.
    ///
    /// The cursor decodes here, where a bad one can become a 400 through
    /// `From<CursorError>`, and binds in `repository` — a half-decoded cursor never
    /// reaches SQL, which is how `master`'s search silently returned page one forever.
    ///
    /// # Errors
    ///
    /// [`meno_core::Error::BadRequest`] for a malformed cursor; a repository failure
    /// otherwise. Validation of `q` itself happened at the boundary.
    pub async fn search(
        &self,
        viewer_id: Uuid,
        query: &ProfileSearchQuery,
    ) -> Result<CursorPage<ProfileSearchResult>, MenoError> {
        let cursor = query
            .cursor()
            .map(|cursor| cursor.to_timestamp_id())
            .transpose()?;

        let rows = self
            .repo
            .search(SearchFilter {
                viewer_id,
                query: query.q.trim(),
                cursor,
                limit_plus_one: query.limit_plus_one(),
            })
            .await?;

        Ok(CursorPage::from_rows(rows, query.limit(), |row| {
            Cursor::from_timestamp_id(row.created_at, row.id)
        }))
    }

    /// `POST /users/me/avatar` — sniff, store, point the row at it, drop the old one.
    ///
    /// Order matters and is deliberate: the object is written *before* the row, so a
    /// failure between the two leaves an orphaned blob the next upload reuses — the
    /// inverse order would leave `avatar_url` pointing at something that does not
    /// exist, which is a broken image in the client rather than wasted storage.
    ///
    /// # Errors
    ///
    /// [`error::avatar_too_large`] / [`error::avatar_not_an_image`] for the body
    /// (422), [`error::upstream`] for the store (503), [`error::not_found`] if the
    /// account is gone.
    pub async fn upload_avatar(
        &self,
        user_id: Uuid,
        bytes: &[u8],
    ) -> Result<UploadAvatarResponse, MenoError> {
        // Read the old key first: `put_avatar` may overwrite the same key (same
        // extension), and only a *different* old key is worth deleting.
        let previous = self.repo.find_avatar_key(user_id).await?;

        let stored = self.storage.put_avatar(user_id, bytes).await?;
        self.repo
            .set_avatar(user_id, &stored.key, stored.url.as_deref())
            .await?;

        if let Some(previous) = previous.filter(|previous| previous != &stored.key)
            && let Err(error) = self.storage.delete_avatar(&previous).await
        {
            tracing::warn!(%error, avatar_key = %previous, "replaced avatar could not be deleted");
        }

        // The cached /me carries the old avatar; drop it so the next read rebuilds.
        if let Err(error) = self.cache.invalidate_cached_profile(user_id).await {
            tracing::warn!(%error, user_id = %user_id, "profile cache invalidation failed; staleness is bounded by the key's TTL");
        }

        Ok(UploadAvatarResponse {
            avatar_id: stored.key,
            avatar_url: stored.url,
        })
    }
}
