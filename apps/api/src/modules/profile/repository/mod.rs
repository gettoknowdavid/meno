//! What profile knows about `users` and `user_subscribers` — as a port, not as SQL.
//!
//! The split `broadcast` established: [`pg`] is the Postgres adapter, [`memory`] the
//! §10 double, and [`ProfileRepo`] is the only thing the service names. Choosing
//! between them happens in `state.rs` (§5.6), which is the only file here allowed to
//! mention a concrete adapter.

pub mod memory;
pub mod pg;

use time::OffsetDateTime;
use uuid::Uuid;

use meno_core::Error as MenoError;

use super::dto::ProfileSearchResult;
use super::model::Profile;
use crate::middleware::auth::AuthProvider;

#[cfg(any(test, feature = "test-support"))]
pub use memory::InMemoryProfileRepo;
pub use pg::PgProfileRepo;

/// One page of `GET /users?q=`, as the repository receives it.
///
/// The cursor is already decoded to its parts — a half-decoded cursor bound as
/// `NULL` is how a paginated search silently returns page one forever, which is
/// exactly what `master`'s unbound `rank > $n` did. Decoding belongs to the service
/// and binding belongs here, so this struct cannot carry an opaque string into SQL.
#[derive(Debug, Clone)]
pub struct SearchFilter<'a> {
    /// Who is searching: answered per row by the `is_following` `EXISTS`.
    pub viewer_id: Uuid,
    /// The already-trimmed search term.
    pub query: &'a str,
    /// `(created_at, id)` of the last row of the previous page; `None` is page one.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
    /// `limit + 1`, so one query can say whether another page exists (§9.4).
    pub limit_plus_one: i64,
}

/// The user's own record — how profile reads and writes it.
///
/// # Semantics the service relies on
///
/// - `update_profile` writes each column with `COALESCE($n, column)`: `None` means
///   "leave it alone", so a partial patch never blanks a field the client omitted.
/// - `find_by_id` skips soft-deleted rows (`deleted_at IS NULL`), so a deleted
///   account is a 404 on every endpoint rather than a ghost profile.
/// - `search` orders by `(created_at DESC, id DESC)` — recency, not `ts_rank`. The
///   shared [`meno_core::Cursor`] is a `(timestamp, id)` pair, the same primitive
///   every other list in this API pages with; a rank-ordered feed would need a
///   three-part codec in `crates/core`, which this refactor deliberately does not
///   invent.
#[async_trait::async_trait]
pub trait ProfileRepo: Send + Sync + 'static {
    /// A live profile, or `None` when the id does not exist or is soft-deleted.
    async fn find_by_id(&self, user_id: Uuid) -> Result<Option<Profile>, MenoError>;

    /// The identity providers linked to the account, deduplicated — `password`,
    /// `google`. Unknown `provider_type` values are skipped rather than failing the
    /// whole `/users/me` read.
    async fn find_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, MenoError>;

    /// The current avatar's storage key, so a replacement can delete the old object.
    async fn find_avatar_key(&self, user_id: Uuid) -> Result<Option<String>, MenoError>;

    /// Apply a partial update and return the resulting row.
    ///
    /// # Errors
    ///
    /// [`super::error::not_found`] when the account does not exist or is deleted —
    /// a caller cannot edit a profile it could not have loaded.
    async fn update_profile(
        &self,
        user_id: Uuid,
        full_name: Option<&str>,
        bio: Option<&str>,
    ) -> Result<Profile, MenoError>;

    /// Point the avatar columns at a freshly stored object.
    async fn set_avatar(
        &self,
        user_id: Uuid,
        avatar_id: &str,
        avatar_url: Option<&str>,
    ) -> Result<(), MenoError>;

    /// Whether `viewer_id` follows `user_id` — one `EXISTS` against
    /// `user_subscribers` (`subscriber_id` is the follower).
    async fn is_following(&self, viewer_id: Uuid, user_id: Uuid) -> Result<bool, MenoError>;

    /// One page of search results, newest first, `limit_plus_one` rows.
    async fn search(&self, filter: SearchFilter<'_>)
    -> Result<Vec<ProfileSearchResult>, MenoError>;
}
