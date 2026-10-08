//! The Postgres adapter for [`ProfileRepo`].
//!
//! Plain `sqlx` runtime queries, not `query!` macros: the macros prepare against a
//! live database at compile time, which is what the refactor plan trades away in
//! order that `cargo check` works without Docker (§15's "works without Postgres
//! standing"). The cursor clause comes from [`crate::infrastructure::query`], the
//! same `push_cursor_condition`/`push_order_and_limit` pair `broadcast`'s list uses,
//! so the row-value comparison is written once for the whole API.

use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use super::{ProfileRepo, SearchFilter};
use crate::infrastructure::query::{ColumnName, push_cursor_condition, push_order_and_limit};
use crate::middleware::auth::AuthProvider;
use crate::modules::profile::dto::ProfileSearchResult;
use crate::modules::profile::error;
use crate::modules::profile::model::{PROFILE_COLUMNS, Profile};
use meno_core::Error as MenoError;

/// The profile module's Postgres adapter. One pool, no state.
#[derive(Clone)]
pub struct PgProfileRepo {
    pool: PgPool,
}

impl PgProfileRepo {
    /// Wrap the application's pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Map a `user_identities.provider_type` value onto [`AuthProvider`].
///
/// `user_identities` admits `email`, `google`, `apple` and `facebook` (migration
/// 0003's check), while `AuthProvider` currently has two variants; the enum is
/// `#[non_exhaustive]` so the third variant lands without an API break, and until
/// then an unknown type is logged and skipped rather than failing the read.
fn provider_from_db(raw: &str) -> Option<AuthProvider> {
    match raw {
        "email" => Some(AuthProvider::Password),
        "google" => Some(AuthProvider::Google),
        other => {
            tracing::warn!(provider_type = other, "unknown provider_type; skipping it");
            None
        }
    }
}

#[async_trait::async_trait]
impl ProfileRepo for PgProfileRepo {
    async fn find_by_id(&self, user_id: Uuid) -> Result<Option<Profile>, MenoError> {
        // `QueryBuilder` rather than `query_as(format!(…))`: sqlx 0.9 refuses a
        // runtime `String` at the `query_as` entry point (`SqlSafeStr`) so that a
        // dynamic string has to be a deliberate choice — the same choice `broadcast`'
        //s `list` makes. The only interpolation is the `&'static str` column const.
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!(
            "SELECT {PROFILE_COLUMNS} FROM users WHERE deleted_at IS NULL AND id = "
        ));
        qb.push_bind(user_id);

        qb.build_query_as::<Profile>()
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| error::internal("profile_find_by_id", e))
    }

    async fn find_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, MenoError> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT provider_type FROM user_identities WHERE user_id = $1 \
             ORDER BY provider_type",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| error::internal("profile_find_providers", e))?;

        Ok(rows
            .iter()
            .filter_map(|raw| provider_from_db(raw))
            .collect())
    }

    async fn find_avatar_key(&self, user_id: Uuid) -> Result<Option<String>, MenoError> {
        let row = sqlx::query_scalar::<_, Option<String>>(
            "SELECT avatar_id FROM users WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| error::internal("profile_find_avatar_key", e))?;

        Ok(row.flatten())
    }

    async fn update_profile(
        &self,
        user_id: Uuid,
        full_name: Option<&str>,
        bio: Option<&str>,
    ) -> Result<Profile, MenoError> {
        // Same `QueryBuilder` route as above; binds land in the order they are
        // pushed, so the SQL reads `COALESCE($1, full_name)`, `COALESCE($2, bio)`,
        // `id = $3`.
        let mut qb: QueryBuilder<Postgres> =
            QueryBuilder::new("UPDATE users SET full_name = COALESCE(");
        qb.push_bind(full_name)
            .push(", full_name), bio = COALESCE(")
            .push_bind(bio)
            .push(", bio) WHERE id = ")
            .push_bind(user_id)
            .push(" AND deleted_at IS NULL RETURNING ")
            .push(PROFILE_COLUMNS);

        qb.build_query_as::<Profile>()
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| error::internal("profile_update", e))?
            .ok_or_else(error::not_found)
    }

    async fn set_avatar(
        &self,
        user_id: Uuid,
        avatar_id: &str,
        avatar_url: Option<&str>,
    ) -> Result<(), MenoError> {
        let result = sqlx::query(
            "UPDATE users SET avatar_id = $2, avatar_url = $3 \
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(user_id)
        .bind(avatar_id)
        .bind(avatar_url)
        .execute(&self.pool)
        .await
        .map_err(|e| error::internal("profile_set_avatar", e))?;

        if result.rows_affected() == 0 {
            return Err(error::not_found());
        }
        Ok(())
    }

    async fn is_following(&self, viewer_id: Uuid, user_id: Uuid) -> Result<bool, MenoError> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM user_subscribers \
                            WHERE subscriber_id = $1 AND subscription_id = $2)",
        )
        .bind(viewer_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| error::internal("profile_is_following", e))
    }

    async fn search(
        &self,
        filter: SearchFilter<'_>,
    ) -> Result<Vec<ProfileSearchResult>, MenoError> {
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            "SELECT u.id, u.full_name, u.bio, u.avatar_url, u.followers, u.following, \
                    u.broadcasts, u.created_at, \
                    EXISTS (SELECT 1 FROM user_subscribers us \
                            WHERE us.subscriber_id = ",
        );
        qb.push_bind(filter.viewer_id);
        qb.push(
            " AND us.subscription_id = u.id) AS is_following \
             FROM users u WHERE u.deleted_at IS NULL \
             AND u.search_vector @@ plainto_tsquery('english', ",
        );
        qb.push_bind(filter.query);
        qb.push(")");

        // Keyset pagination over (created_at DESC, id DESC): the same row-value
        // comparison every other list builds, via the shared helper.
        push_cursor_condition(
            &mut qb,
            ColumnName::literal("u.created_at"),
            ColumnName::literal("u.id"),
            filter.cursor.map(|(ts, _)| ts),
            filter.cursor.map(|(_, id)| id),
            meno_core::Order::Desc,
        );
        push_order_and_limit(
            &mut qb,
            ColumnName::literal("u.created_at"),
            ColumnName::literal("u.id"),
            meno_core::Order::Desc,
            filter.limit_plus_one,
        );

        qb.build_query_as::<ProfileSearchResult>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| error::internal("profile_search", e))
    }
}
