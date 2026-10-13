//! The Postgres adapter for [`SubscribersRepo`].
//!
//! Plain `sqlx` runtime queries, not `query!` macros — the same trade as the rest of the
//! refactored tree: `cargo check` works with no database standing, and the SQL is exercised
//! by `tests/subscribers_repository_pg.rs` against a live Postgres and by
//! `tests/subscribers_service.rs` against the in-memory double that mirrors it.
//!
//! # Injection
//!
//! Every value a request contributes is a bind parameter. The only interpolated text is
//! the pair of column names `page_query` chooses from [`FollowDirection`], and they are
//! `const`s in this file — [`ColumnName::literal`], the infallible constructor the query
//! helpers document for names written in this crate's own source.
//!
//! # One statement per list, one per write
//!
//! - [`find_follows`](SubscribersRepo::find_follows) is a single query: the viewer's
//!   `is_following` answer is an `EXISTS` in the SELECT list, not a second round trip.
//!   `master` read the page and then asked again for the annotation.
//! - [`create`](SubscribersRepo::create) guards the insert on the target user existing, so
//!   a missing user is `false` rather than a foreign-key violation, and `ON CONFLICT DO
//!   NOTHING` makes a repeated tap change nothing — which is also what keeps the counters
//!   the migration's trigger maintains from double-counting.
//! - [`remove`](SubscribersRepo::remove) is one `DELETE`, and `rows_affected` is the answer
//!   rather than a follow-up `SELECT`: a second delete touches no row, and the caller needs
//!   only to know that it did not.
//!
//! # The indexes these queries ride
//!
//! Migration `0009` indexes `(subscription_id, created_at DESC, subscriber_id DESC)` and
//! `(subscriber_id, created_at DESC, subscription_id DESC)` — one per direction, columns
//! and order matching this file's `WHERE` plus `ORDER BY`. Both list queries are therefore
//! one index scan, and the keyset cursor is a bound comparison on that same order.
//!
//! # What the module does not write
//!
//! `users.followers` and `users.following` are the `user_subscribers` trigger's job
//! (migration `0009`). Nothing here touches them: a counter written by two places is a
//! counter that eventually disagrees with the rows.

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use super::{FollowFilter, SubscribersRepo};
use crate::infrastructure::query::{ColumnName, push_cursor_condition, push_order_and_limit};
use crate::modules::subscribers::dto::{FollowDirection, SubscriberDto};
use crate::modules::subscribers::error;
use meno_core::{Error as MenoError, Order};

/// The join table's keyset timestamp column, qualified for a query that also joins `users`.
const CREATED_AT: ColumnName<'static> = ColumnName::literal("us.created_at");

/// The insert that records a follow, or explains that there is nothing to record.
///
/// The `WHERE EXISTS` is load-bearing twice over: it keeps a follow of a nonexistent (or
/// soft-deleted) user out of the foreign key, so that case is a `false` rather than a 500.
/// `ON CONFLICT DO NOTHING` is the other half — a repeat tap inserts nothing, so the
/// trigger that maintains the counters does not fire twice.
const CREATE_FOLLOW: &str = "\
     INSERT INTO user_subscribers (subscriber_id, subscription_id) \
     SELECT $1, $2 \
     WHERE EXISTS (SELECT 1 FROM users WHERE id = $2 AND deleted_at IS NULL) \
     ON CONFLICT (subscriber_id, subscription_id) DO NOTHING";

/// The delete that removes a follow. Removing nothing is a success (see the trait).
const REMOVE_FOLLOW: &str = "\
     DELETE FROM user_subscribers \
     WHERE subscriber_id = $1 AND subscription_id = $2";

/// Whether a live account carries this id.
const USER_EXISTS: &str =
    "SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND deleted_at IS NULL)";

/// Postgres-backed storage for the subscribers module.
#[derive(Debug, Clone)]
pub struct PgSubscribersRepo {
    pool: PgPool,
}

impl PgSubscribersRepo {
    /// Read and write through `pool`.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Log the driver's text and return a 500 that does not contain it.
fn database(context: &'static str, failure: sqlx::Error) -> MenoError {
    tracing::error!(error = %failure, context, "subscribers repository call failed");
    error::internal(context, failure)
}

/// The owner's column and the described user's column, in that order.
///
/// The only place the direction turns into SQL, and the reason the two list statements are
/// one function: getting these two the wrong way round returns an empty list, which is
/// indistinguishable from a user nobody follows.
fn columns_for(direction: FollowDirection) -> (ColumnName<'static>, ColumnName<'static>) {
    if direction.owner_is_subscription() {
        (
            ColumnName::literal("us.subscription_id"),
            ColumnName::literal("us.subscriber_id"),
        )
    } else {
        (
            ColumnName::literal("us.subscriber_id"),
            ColumnName::literal("us.subscription_id"),
        )
    }
}

/// The page statement, before it is executed.
///
/// Split out from the trait method so the SQL text can be asserted on without a database
/// (see the tests below): the column swap above is exactly the kind of bug an in-memory
/// double cannot catch, because the double reasons about fields rather than about SQL.
///
/// # The pushes are interleaved with the text
///
/// `QueryBuilder` appends each segment in call order and numbers the placeholders in that
/// same order, so a bind pushed after the `WHERE` clause cannot serve a placeholder that
/// appears in the `SELECT` list. The viewer's `EXISTS` comes first in the statement, so the
/// viewer is pushed before the rest of the text — including the owner's `=` and the
/// cursor's row comparison, which the helpers below append in their own order.
fn page_query(filter: &FollowFilter) -> QueryBuilder<Postgres> {
    let (owner, counterpart) = columns_for(filter.direction);

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT u.id, u.full_name, u.bio, u.avatar_url, \
         EXISTS (SELECT 1 FROM user_subscribers mine \
         WHERE mine.subscriber_id = ",
    );
    qb.push_bind(filter.viewer_id)
        .push(
            " AND mine.subscription_id = u.id) AS is_following, \
             us.created_at AS subscribed_at \
             FROM user_subscribers us \
             JOIN users u ON u.id = ",
        )
        .push(counterpart)
        .push(" AND u.deleted_at IS NULL WHERE ")
        .push(owner)
        .push(" = ")
        .push_bind(filter.owner_id);

    let (cursor_ts, cursor_id) = match filter.cursor {
        None => (None, None),
        Some((ts, id)) => (Some(ts), Some(id)),
    };
    push_cursor_condition(
        &mut qb,
        CREATED_AT,
        counterpart,
        cursor_ts,
        cursor_id,
        Order::Desc,
    );
    push_order_and_limit(
        &mut qb,
        CREATED_AT,
        counterpart,
        Order::Desc,
        filter.limit_plus_one,
    );
    qb
}

#[async_trait]
impl SubscribersRepo for PgSubscribersRepo {
    async fn create(&self, follower_id: Uuid, target_id: Uuid) -> Result<bool, MenoError> {
        let result = sqlx::query(CREATE_FOLLOW)
            .bind(follower_id)
            .bind(target_id)
            .execute(&self.pool)
            .await
            .map_err(|e| database("create_follow", e))?;

        Ok(result.rows_affected() == 1)
    }

    async fn remove(&self, follower_id: Uuid, target_id: Uuid) -> Result<bool, MenoError> {
        let result = sqlx::query(REMOVE_FOLLOW)
            .bind(follower_id)
            .bind(target_id)
            .execute(&self.pool)
            .await
            .map_err(|e| database("remove_follow", e))?;

        Ok(result.rows_affected() == 1)
    }

    async fn user_exists(&self, user_id: Uuid) -> Result<bool, MenoError> {
        sqlx::query_scalar::<_, bool>(USER_EXISTS)
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| database("user_exists", e))
    }

    async fn find_follows(&self, filter: &FollowFilter) -> Result<Vec<SubscriberDto>, MenoError> {
        page_query(filter)
            .build_query_as::<SubscriberDto>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("find_follows", e))
    }
}

#[cfg(test)]
mod tests {
    //! The generated SQL's shape, asserted without a database.
    //!
    //! These cannot prove the query *runs* — that is `subscribers_repository_pg.rs`'s job —
    //! but they do pin the two things the double cannot see: which column each direction
    //! filters on, and that the annotation is in the page statement rather than in a second
    //! call.

    use super::*;
    use time::OffsetDateTime;

    fn filter(direction: FollowDirection, cursor: Option<(OffsetDateTime, Uuid)>) -> FollowFilter {
        FollowFilter {
            owner_id: Uuid::from_u128(1),
            viewer_id: Uuid::from_u128(2),
            direction,
            limit_plus_one: 21,
            cursor,
        }
    }

    fn sql_of(filter: &FollowFilter) -> String {
        // The `SqlStr` must be bound to a local: `sql()` returns a temporary whose `&str`
        // would otherwise outlive it.
        let rendered = page_query(filter).sql();
        rendered.as_str().to_owned()
    }

    #[test]
    fn a_followers_page_filters_on_the_subscription_side() {
        let sql = sql_of(&filter(FollowDirection::Followers, None));

        assert!(
            sql.contains("WHERE us.subscription_id = $2"),
            "the owner is the second bind, after the viewer's EXISTS: {sql}"
        );
        assert!(
            sql.contains("JOIN users u ON u.id = us.subscriber_id"),
            "the described user is the follower: {sql}"
        );
    }

    #[test]
    fn a_following_page_filters_on_the_subscriber_side() {
        let sql = sql_of(&filter(FollowDirection::Following, None));

        assert!(
            sql.contains("WHERE us.subscriber_id = $2"),
            "the same placeholders, the other column: {sql}"
        );
        assert!(
            sql.contains("JOIN users u ON u.id = us.subscription_id"),
            "the described user is the followee: {sql}"
        );
    }

    #[test]
    fn the_page_annotates_for_the_viewer_in_the_same_statement() {
        let sql = sql_of(&filter(FollowDirection::Followers, None));

        assert!(sql.contains("AS is_following"), "{sql}");
        assert!(
            sql.contains("mine.subscriber_id = $1"),
            "the viewer is bound first, because the EXISTS comes first: {sql}"
        );
        assert!(
            !sql.contains("batch"),
            "no second statement belongs to a page: {sql}"
        );
    }

    #[test]
    fn a_cursor_sits_before_the_order_clause_and_moves_with_the_counterpart_id() {
        let cursor = (
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("a plausible instant"),
            Uuid::from_u128(9),
        );
        let sql = sql_of(&filter(FollowDirection::Followers, Some(cursor)));

        let keyset = sql
            .find("AND (us.created_at, us.subscriber_id) < (")
            .expect("the row-value comparison");
        let order = sql.find("ORDER BY").expect("the order clause");
        assert!(keyset < order, "{sql}");
        assert!(
            sql.contains(
                "ORDER BY us.created_at DESC NULLS LAST, us.subscriber_id DESC NULLS LAST"
            ),
            "the index in migration 0009 is built for this order: {sql}"
        );
    }
}
