//! The row shapes profile reads from `users`.
//!
//! # What changed from `master`
//!
//! [`Profile`] now carries `verified` (the column's own name) and the three counters,
//! because `master`'s model had neither and its `MeResponse` therefore always reported
//! `followers: 0, following: 0, broadcasts: 0` — the "counters are dead" bug the plan
//! calls out. The columns are maintained by the `user_subscribers` trigger (migration
//! `0009`), so reading them is all this module has to do.

use time::OffsetDateTime;
use uuid::Uuid;

/// The columns the profile module reads, in one place so every `SELECT` agrees.
///
/// `pub` because `profile_repository_pg.rs` asserts each name against
/// `information_schema` — the point of listing them once is that both the SQL and
/// the schema check read the same list.
pub const PROFILE_COLUMNS: &str = "id, full_name, email, verified, bio, avatar_id, avatar_url, followers, following, broadcasts, created_at";

/// A live row of `users`, as `/users/me` and `/users/{id}` see it.
///
/// Deliberately *not* `Serialize`: this is the table's shape, not the wire's. The
/// response structs in [`super::dto`] decide what a client may see — `email`, for
/// instance, exists on this row and appears on no public response.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Profile {
    /// The user's id.
    pub id: Uuid,
    /// The display name.
    pub full_name: String,
    /// The email address. On the wire it appears only on `/users/me`; public
    /// responses are built from this row but never map this field.
    pub email: String,
    /// `users.verified` — whether the email address has been confirmed. Named for the
    /// column here and `email_verified` on the wire, which is what `master` shipped.
    pub verified: bool,
    /// The free-text bio. `VARCHAR(244)`.
    pub bio: Option<String>,
    /// Storage key of the current avatar, e.g. `avatars/<user-id>.png`.
    pub avatar_id: Option<String>,
    /// CDN URL for that object, captured at upload time.
    pub avatar_url: Option<String>,
    /// How many users follow this one. Kept by the `update_user_follow_counts`
    /// trigger, never written by hand.
    pub followers: i64,
    /// How many users this one follows.
    pub following: i64,
    /// How many broadcasts this one has created.
    pub broadcasts: i64,
    /// Account creation, in UTC.
    pub created_at: OffsetDateTime,
}
