//! The Postgres adapter for [`NotificationsRepo`].
//!
//! Plain `sqlx` runtime queries, not `query!` macros — the same trade as the rest of
//! the refactored tree: `cargo check` works with no database standing, and the SQL is
//! exercised by `tests/notifications_repository_pg.rs` against a live Postgres and by
//! `tests/notifications_service.rs` against the in-memory double that mirrors it.
//!
//! # Injection
//!
//! Every value a request contributes is a bind parameter. The only interpolated text
//! is the pair of column names the page query's cursor helpers are given, and they
//! are `const`s in this file — [`ColumnName::literal`], the infallible constructor the
//! query helpers document for names written in this crate's own source.
//!
//! # The indexes these queries ride
//!
//! Migration `0012` indexes `(owner_id, created_at DESC, id DESC)` twice — once for
//! the whole inbox and once partial on `read = false` — plus a partial index per
//! foreign key. The page query's `WHERE owner_id` / `archived_at IS NULL` / optional
//! `read = false` and its `ORDER BY` match one of them exactly, so a page is one
//! index scan and the keyset cursor is a bound comparison on that same order.
//!
//! # Rendering is not here
//!
//! The page returns [`NotificationRow`]: the template's text with placeholders
//! intact. Substitution is the service's job, so a template edit and a rendering
//! change are one file apart, and the storage layer never decides what a sentence
//! says.

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use super::{NotificationFilter, NotificationsRepo};
use crate::infrastructure::push::model::PushTarget;
use crate::infrastructure::query::{ColumnName, push_cursor_condition, push_order_and_limit};
use crate::modules::notifications::error;
use crate::modules::notifications::model::{NotificationRow, NotificationTemplate};
use meno_core::{Error as MenoError, Order};

/// The page's keyset timestamp column, qualified for the joins.
const CREATED_AT: ColumnName<'static> = ColumnName::literal("n.created_at");

/// The page's tiebreaker. `notifications.id` is a UUID, so it orders the row the
/// same way the index does.
const ID: ColumnName<'static> = ColumnName::literal("n.id");

/// The template lookup. One active template per type in practice; `LIMIT 1` makes a
/// stray duplicate a deterministic pick rather than a driver error.
const FIND_TEMPLATE: &str = "\
     SELECT id, type, title, body, image_url \
     FROM notification_templates \
     WHERE type = $1 AND is_active = true AND deleted_at IS NULL \
     ORDER BY created_at DESC LIMIT 1";

/// Every active template, for the cache.
const FIND_ALL_TEMPLATES: &str = "\
     SELECT id, type, title, body, image_url \
     FROM notification_templates \
     WHERE is_active = true AND deleted_at IS NULL \
     ORDER BY created_at DESC";

/// One notification. `entity_type` / `entity_id` / `custom_metadata` are left to
/// the column defaults: the module does not use them, and binding nulls over
/// defaults is how a column quietly stops being defaulted.
const CREATE_ONE: &str = "\
     INSERT INTO notifications (owner_id, template_id, actor_id, broadcast_id) \
     VALUES ($1, $2, $3, $4) \
     RETURNING id";

/// One notification per recipient, in one statement.
const CREATE_BULK: &str = "\
     INSERT INTO notifications (owner_id, template_id, actor_id, broadcast_id) \
     SELECT unnest($1::uuid[]), $2, $3, $4 \
     ON CONFLICT DO NOTHING";

/// Mark one row read. `read_at` is stamped only on the transition, which is why the
/// `read = false` predicate is part of the statement rather than an `if` above it.
const MARK_READ: &str = "\
     UPDATE notifications SET read = true, read_at = now() \
     WHERE id = $1 AND owner_id = $2 AND read = false AND archived_at IS NULL \
     RETURNING id";

/// Mark every unread row read, and count what moved.
const MARK_ALL_READ: &str = "\
     UPDATE notifications SET read = true, read_at = now() \
     WHERE owner_id = $1 AND read = false AND archived_at IS NULL";

/// Hard-delete one row, scoped to its owner.
const DELETE_ONE: &str = "\
     DELETE FROM notifications \
     WHERE id = $1 AND owner_id = $2 AND archived_at IS NULL";

/// Whether a live row carries this id for this owner.
const EXISTS: &str = "\
     SELECT EXISTS ( \
       SELECT 1 FROM notifications \
       WHERE id = $1 AND owner_id = $2 AND archived_at IS NULL \
     )";

/// Whether the row is unread, or absent. Absent answers `false`, which is what makes
/// the delete path's counter adjustment correct for a row that is not there.
const IS_UNREAD: &str = "\
     SELECT read FROM notifications \
     WHERE id = $1 AND owner_id = $2 AND archived_at IS NULL";

/// The database's unread count — the counter's cold path.
const COUNT_UNREAD: &str = "\
     SELECT COUNT(*) FROM notifications \
     WHERE owner_id = $1 AND read = false AND archived_at IS NULL";

/// A user's registered device token, when push is switched on and a token is set.
const DEVICE_TOKEN: &str = "\
     SELECT push_notification_token FROM settings \
     WHERE user_id = $1 AND push_notifications = true \
       AND push_notification_token IS NOT NULL";

/// Device tokens for a fan-out. Only users with push enabled, for the reason the
/// single lookup only reads one: a token for a user who turned push off is a send
/// they asked not to receive.
const PUSH_TARGETS: &str = "\
     SELECT user_id, push_notification_token FROM settings \
     WHERE user_id = ANY($1) AND push_notification_token IS NOT NULL \
       AND push_notifications = true";

/// Forget a token FCM rejected.
const CLEAR_TOKEN: &str = "UPDATE settings SET push_notification_token = NULL WHERE user_id = $1";

/// Whether the recipient wants in-app notifications. No settings row reads as `false`,
/// matching `master`: a user who has never opened settings has not opted in.
const APP_NOTIFICATIONS: &str = "SELECT app_notifications FROM settings WHERE user_id = $1";

/// Postgres-backed storage for the notifications module.
#[derive(Debug, Clone)]
pub struct PgNotificationsRepo {
    pool: PgPool,
}

impl PgNotificationsRepo {
    /// Read and write through `pool`.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Log the driver's text and return a 500 that does not contain it.
fn database(context: &'static str, failure: sqlx::Error) -> MenoError {
    tracing::error!(error = %failure, context, "notifications repository call failed");
    error::internal(context, failure)
}

/// The page statement, before it is executed.
///
/// Split out from the trait method so the SQL text can be asserted on without a
/// database (see the tests below): whether the `unread_only` predicate and the
/// cursor land in the right order is exactly what a double cannot check, because the
/// double reasons about fields rather than about SQL.
fn page_query(filter: &NotificationFilter) -> QueryBuilder<Postgres> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT n.id, n.read, n.created_at, \
         ntpl.type AS type_code, \
         ntpl.title AS title_template, \
         ntpl.body AS body_template, \
         ntpl.image_url AS image_url_template, \
         n.broadcast_id, b.title AS broadcast_title, \
         n.actor_id, u.full_name AS actor_name, u.bio AS actor_bio, \
         u.avatar_url AS actor_avatar_url, u.avatar_id AS actor_avatar_id \
         FROM notifications n \
         JOIN notification_templates ntpl ON ntpl.id = n.template_id \
         LEFT JOIN broadcasts b ON b.id = n.broadcast_id \
         LEFT JOIN users u ON u.id = n.actor_id AND u.deleted_at IS NULL \
         WHERE n.owner_id = ",
    );
    qb.push_bind(filter.owner_id)
        .push(" AND n.archived_at IS NULL");

    if filter.unread_only {
        qb.push(" AND n.read = false");
    }

    let (cursor_ts, cursor_id) = match filter.cursor {
        None => (None, None),
        Some((ts, id)) => (Some(ts), Some(id)),
    };
    push_cursor_condition(&mut qb, CREATED_AT, ID, cursor_ts, cursor_id, Order::Desc);
    push_order_and_limit(&mut qb, CREATED_AT, ID, Order::Desc, filter.limit_plus_one);
    qb
}

#[async_trait]
impl NotificationsRepo for PgNotificationsRepo {
    async fn find_template_by_code(
        &self,
        type_code: &str,
    ) -> Result<Option<NotificationTemplate>, MenoError> {
        sqlx::query_as::<_, NotificationTemplate>(FIND_TEMPLATE)
            .bind(type_code)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| database("find_template_by_code", e))
    }

    async fn find_all_templates(&self) -> Result<Vec<NotificationTemplate>, MenoError> {
        sqlx::query_as::<_, NotificationTemplate>(FIND_ALL_TEMPLATES)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("find_all_templates", e))
    }

    async fn create(
        &self,
        owner_id: Uuid,
        template_id: Uuid,
        actor_id: Option<Uuid>,
        broadcast_id: Option<Uuid>,
    ) -> Result<Uuid, MenoError> {
        sqlx::query_scalar::<_, Uuid>(CREATE_ONE)
            .bind(owner_id)
            .bind(template_id)
            .bind(actor_id)
            .bind(broadcast_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| database("create_notification", e))
    }

    async fn create_bulk(
        &self,
        owner_ids: &[Uuid],
        template_id: Uuid,
        actor_id: Option<Uuid>,
        broadcast_id: Option<Uuid>,
    ) -> Result<u64, MenoError> {
        if owner_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(CREATE_BULK)
            .bind(owner_ids)
            .bind(template_id)
            .bind(actor_id)
            .bind(broadcast_id)
            .execute(&self.pool)
            .await
            .map_err(|e| database("create_notifications_bulk", e))?;

        Ok(result.rows_affected())
    }

    async fn mark_read(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError> {
        let result = sqlx::query(MARK_READ)
            .bind(notification_id)
            .bind(owner_id)
            .execute(&self.pool)
            .await
            .map_err(|e| database("mark_notification_read", e))?;

        Ok(result.rows_affected() == 1)
    }

    async fn mark_all_read(&self, owner_id: Uuid) -> Result<u64, MenoError> {
        let result = sqlx::query(MARK_ALL_READ)
            .bind(owner_id)
            .execute(&self.pool)
            .await
            .map_err(|e| database("mark_all_notifications_read", e))?;

        Ok(result.rows_affected())
    }

    async fn delete(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError> {
        let result = sqlx::query(DELETE_ONE)
            .bind(notification_id)
            .bind(owner_id)
            .execute(&self.pool)
            .await
            .map_err(|e| database("delete_notification", e))?;

        Ok(result.rows_affected() == 1)
    }

    async fn find_page(
        &self,
        filter: &NotificationFilter,
    ) -> Result<Vec<NotificationRow>, MenoError> {
        page_query(filter)
            .build_query_as::<NotificationRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("find_notifications_page", e))
    }

    async fn exists(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError> {
        sqlx::query_scalar::<_, bool>(EXISTS)
            .bind(notification_id)
            .bind(owner_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| database("notification_exists", e))
    }

    async fn is_unread(&self, notification_id: Uuid, owner_id: Uuid) -> Result<bool, MenoError> {
        let read = sqlx::query_scalar::<_, bool>(IS_UNREAD)
            .bind(notification_id)
            .bind(owner_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| database("notification_is_unread", e))?;

        Ok(read == Some(false))
    }

    async fn count_unread(&self, owner_id: Uuid) -> Result<i64, MenoError> {
        sqlx::query_scalar::<_, i64>(COUNT_UNREAD)
            .bind(owner_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| database("count_unread_notifications", e))
    }

    async fn device_token(&self, user_id: Uuid) -> Result<Option<String>, MenoError> {
        sqlx::query_scalar::<_, String>(DEVICE_TOKEN)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| database("notification_device_token", e))
    }

    async fn push_targets(&self, user_ids: &[Uuid]) -> Result<Vec<PushTarget>, MenoError> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, (Uuid, String)>(PUSH_TARGETS)
            .bind(user_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("notification_push_targets", e))?;

        Ok(rows
            .into_iter()
            .map(|(user_id, token)| PushTarget::new(user_id, token))
            .collect())
    }

    async fn clear_device_token(&self, user_id: Uuid) -> Result<(), MenoError> {
        sqlx::query(CLEAR_TOKEN)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| database("clear_device_token", e))?;

        Ok(())
    }

    async fn app_notifications_enabled(&self, user_id: Uuid) -> Result<bool, MenoError> {
        let enabled = sqlx::query_scalar::<_, bool>(APP_NOTIFICATIONS)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| database("app_notifications_enabled", e))?;

        Ok(enabled.unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    //! The generated page SQL's shape, asserted without a database.

    use super::*;
    use time::OffsetDateTime;

    fn filter(unread_only: bool, cursor: Option<(OffsetDateTime, Uuid)>) -> NotificationFilter {
        NotificationFilter {
            owner_id: Uuid::from_u128(1),
            unread_only,
            limit_plus_one: 21,
            cursor,
        }
    }

    fn sql_of(filter: &NotificationFilter) -> String {
        let rendered = page_query(filter).sql();
        rendered.as_str().to_owned()
    }

    #[test]
    fn a_page_is_owned_and_excludes_archived_rows() {
        let sql = sql_of(&filter(false, None));

        assert!(sql.contains("WHERE n.owner_id = $1"), "{sql}");
        assert!(sql.contains("AND n.archived_at IS NULL"), "{sql}");
        assert!(
            !sql.contains("n.read = false"),
            "the whole inbox is not the unread one: {sql}"
        );
    }

    #[test]
    fn the_unread_filter_is_an_addition_not_a_replacement() {
        let sql = sql_of(&filter(true, None));

        assert!(sql.contains("WHERE n.owner_id = $1"), "{sql}");
        assert!(sql.contains("AND n.read = false"), "{sql}");
    }

    #[test]
    fn the_page_renders_its_own_sentence_inputs() {
        // The fix `master` needed: the broadcast's title is joined, so `{title}` in a
        // template has something to be replaced with on the list path.
        let sql = sql_of(&filter(false, None));

        assert!(sql.contains("b.title AS broadcast_title"), "{sql}");
        assert!(sql.contains("ntpl.title AS title_template"), "{sql}");
        assert!(sql.contains("u.full_name AS actor_name"), "{sql}");
        assert!(
            sql.contains("u.deleted_at IS NULL"),
            "a deleted actor reads as no actor: {sql}"
        );
    }

    #[test]
    fn a_cursor_sits_before_the_order_clause() {
        let cursor = (
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("a plausible instant"),
            Uuid::from_u128(9),
        );
        let sql = sql_of(&filter(false, Some(cursor)));

        let keyset = sql
            .find("AND (n.created_at, n.id) < (")
            .expect("the row-value comparison");
        let order = sql.find("ORDER BY").expect("the order clause");
        assert!(keyset < order, "{sql}");
        assert!(
            sql.contains("ORDER BY n.created_at DESC NULLS LAST, n.id DESC NULLS LAST"),
            "the index in migration 0012 is built for this order: {sql}"
        );
    }
}
