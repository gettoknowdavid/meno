use crate::modules::notifications::dto::{NotificationListItem, NotificationQuery};
use crate::modules::notifications::error::NotificationError;
use crate::modules::notifications::model::{
    Notification, NotificationTemplate, NotificationViewRow,
};
use crate::shared::types::dto::UserSummary;
use serde_json::Value;
use sqlx::QueryBuilder;
use std::collections::HashMap;
use tracing::instrument;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct NotificationRepository {
    db: sqlx::PgPool,
}
impl NotificationRepository {
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }

    /// Insert a single notification row and return it.
    #[instrument(skip(self), fields(owner_id = %owner_id, template_id = %template_id))]
    pub async fn create(
        &self,
        owner_id: Uuid,
        template_id: Uuid,
        actor_id: Option<Uuid>,
        broadcast_id: Option<Uuid>,
        metadata: Option<Value>,
    ) -> Result<Notification, NotificationError> {
        sqlx::query_as!(
            Notification,
            r#"INSERT INTO notifications (owner_id, template_id, actor_id, broadcast_id, custom_metadata)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING *"#,
            owner_id,
            template_id,
            actor_id,
            broadcast_id,
            metadata,
        )
        .fetch_one(&self.db)
        .await
        .map_err(NotificationError::Database)
    }

    /// Fan-out: insert one row per `owner_id` in a single statement.
    /// `ON CONFLICT DO NOTHING` makes this safe to retry.
    /// Returns the number of rows actually inserted.
    #[instrument(skip(self), fields(count = owner_ids.len(), template_id = %template_id))]
    pub async fn create_bulk(
        &self,
        owner_ids: &[Uuid],
        template_id: Uuid,
        actor_id: Option<Uuid>,
        broadcast_id: Option<Uuid>,
    ) -> Result<u64, NotificationError> {
        if owner_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query!(
            r#"INSERT INTO notifications (owner_id, template_id, actor_id, broadcast_id)
               SELECT unnest($1::uuid[]), $2, $3, $4
               ON CONFLICT DO NOTHING"#,
            owner_ids,
            template_id,
            actor_id,
            broadcast_id,
        )
        .execute(&self.db)
        .await
        .map_err(NotificationError::Database)?;

        Ok(result.rows_affected())
    }

    /// Cursor-paginated list for `GET /notifications`.
    /// Joins template + actor in one query; template variable substitution
    /// is done in the service layer after fetch.
    #[instrument(
        skip(self, query),
        fields(
            owner_id = %owner_id,
            unread_only = query.unread_only,
            limit = query.limit()
        )
    )]
    pub async fn find_notifications(
        &self,
        query: &NotificationQuery,
        owner_id: Uuid,
    ) -> Result<Vec<NotificationListItem>, NotificationError> {
        let (cursor_ts, cursor_id) = match query.cursor() {
            None => (None, None),
            Some(c) => {
                let (ts, id) = c.to_timestamp_id()?;
                (Some(ts), Some(id))
            }
        };

        let mut qb = QueryBuilder::new(
            r#"SELECT
                    n.id,
                    n.read,
                    n.created_at,
                    nt.code                 AS type_code,
                    ntpl.title              AS title_template,
                    ntpl.body               AS body_template,
                    ntpl.image_url          AS image_url_template,
                    n.actor_id,
                    u.full_name             AS actor_name,
                    u.bio                   AS actor_bio,
                    u.avatar_url            AS actor_avatar_url,
                    u.avatar_id             AS actor_avatar_id,
                    n.broadcast_id
               FROM notifications n
               JOIN  notification_templates ntpl ON ntpl.id = n.template_id
               JOIN  notification_types     nt   ON nt.code = ntpl.type
               LEFT  JOIN users u ON u.id = n.actor_id AND u.deleted_at IS NULL
               WHERE n.owner_id = "#,
        );

        qb.push_bind(owner_id);
        qb.push(" AND n.archived_at IS NULL");

        if query.unread_only {
            qb.push(" AND n.read = false");
        }

        if let (Some(ts), Some(id)) = (cursor_ts, cursor_id) {
            qb.push(" AND (n.created_at, n.id) < (")
                .push_bind(ts)
                .push(", ")
                .push_bind(id)
                .push(")");
        }

        qb.push(" ORDER BY n.created_at DESC, n.id DESC")
            .push(" LIMIT ")
            .push_bind(query.limit_plus_one());

        let rows = qb
            .build_query_as::<NotificationViewRow>()
            .fetch_all(&self.db)
            .await?;

        // Assemble DTOs from the flat rows.
        let items = rows
            .into_iter()
            .map(|row| {
                let actor = row.actor_id.map(|id| UserSummary {
                    id,
                    full_name: row.actor_name.clone().unwrap_or_default(),
                    bio: row.actor_bio.clone(),
                    avatar_id: row.actor_avatar_id.clone(),
                    avatar_url: row.actor_avatar_url.clone(),
                });

                NotificationListItem {
                    id: row.id,
                    type_code: row.type_code,
                    title: row.title_template,
                    body: row.body_template,
                    image_url: row.image_url_template,
                    read: row.read,
                    broadcast_id: row.broadcast_id,
                    actor,
                    created_at: row.created_at,
                    deep_link: String::new(),
                }
            })
            .collect();

        Ok(items)
    }

    /// Fast unread count. Used as Redis-fallback when the key is missing.
    #[instrument(skip(self), fields(owner_id = %owner_id))]
    pub async fn count_unread(&self, owner_id: Uuid) -> Result<i64, NotificationError> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS count
               FROM notifications
               WHERE owner_id = $1 AND read = false AND archived_at IS NULL"#,
            owner_id,
        )
        .fetch_one(&self.db)
        .await?;

        Ok(count.unwrap_or(0))
    }

    /// Returns `true` if the notification exists, belongs to `owner_id`, and is unread.
    /// Used before `delete` to decide whether to decrement the Redis unread counter.
    #[instrument(skip(self))]
    pub async fn is_unread(
        &self,
        notification_id: Uuid,
        owner_id: Uuid,
    ) -> Result<bool, NotificationError> {
        let result = sqlx::query_scalar!(
            r#"SELECT read FROM notifications WHERE id = $1 AND owner_id = $2"#,
            notification_id,
            owner_id,
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(result == Some(false))
    }

    /// Marks a single notification as read. Returns `true` if a row was updated.
    #[instrument(skip(self))]
    pub async fn mark_read(
        &self,
        notification_id: Uuid,
        owner_id: Uuid,
    ) -> Result<bool, NotificationError> {
        let result = sqlx::query!(
            r#"UPDATE notifications
               SET read = true, read_at = NOW()
               WHERE id = $1 AND owner_id = $2 AND read = false
               RETURNING id"#,
            notification_id,
            owner_id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(NotificationError::Database)?;

        Ok(result.is_some())
    }

    /// Marks every unread notification for `owner_id` as read.
    /// Returns the count of rows updated.
    #[instrument(skip(self), fields(owner_id = %owner_id))]
    pub async fn mark_all_read(&self, owner_id: Uuid) -> Result<u64, NotificationError> {
        let result = sqlx::query!(
            r#"UPDATE notifications
               SET read = true, read_at = NOW()
               WHERE owner_id = $1 AND read = false AND archived_at IS NULL
               RETURNING id"#,
            owner_id,
        )
        .fetch_all(&self.db)
        .await?;

        Ok(result.len() as u64)
    }

    /// Hard-deletes a notification. Only the owner can delete their own notifications.
    #[instrument(skip(self))]
    pub async fn delete(
        &self,
        notification_id: Uuid,
        owner_id: Uuid,
    ) -> Result<(), NotificationError> {
        sqlx::query!(
            r#"DELETE FROM notifications WHERE id = $1 AND owner_id = $2"#,
            notification_id,
            owner_id,
        )
        .execute(&self.db)
        .await
        .map_err(NotificationError::Database)?;

        Ok(())
    }

    /// Look up a template by its type code.
    /// The service caches these at startup; this is the cold-path fallback.
    #[instrument(skip(self), fields(code_type = %code_type))]
    pub async fn find_template_by_code(
        &self,
        code_type: &str,
    ) -> Result<Option<NotificationTemplate>, NotificationError> {
        sqlx::query_as!(
            NotificationTemplate,
            r#"SELECT *
               FROM notification_templates
               WHERE type = $1 AND is_active = true AND deleted_at IS NULL"#,
            code_type,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(NotificationError::Database)
    }

    /// Load every active template — called once at startup to warm the in-memory cache.
    #[instrument(skip(self))]
    pub async fn find_all_templates(&self) -> Result<Vec<NotificationTemplate>, NotificationError> {
        sqlx::query_as!(
            NotificationTemplate,
            r#"SELECT * FROM notification_templates WHERE is_active = true ORDER BY type"#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(NotificationError::Database)
    }

    /// Get the FCM push token for a single user (used in single-notification paths).
    #[instrument(skip(self), fields(user_id = %user_id))]
    pub async fn get_push_token(&self, user_id: Uuid) -> Result<Option<String>, NotificationError> {
        sqlx::query_scalar!(
            r#"SELECT push_notification_token FROM general_settings WHERE user_id = $1"#,
            user_id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(NotificationError::Database)
    }

    /// Batch-fetch FCM tokens for fan-out sends.
    /// Only returns tokens for users who have `push_notifications = true`.
    #[instrument(skip(self), fields(count = user_ids.len()))]
    pub async fn get_push_tokens_batch(
        &self,
        user_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, String>, NotificationError> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query!(
            r#"SELECT user_id, push_notification_token
               FROM general_settings
               WHERE user_id = ANY($1)
                 AND push_notification_token IS NOT NULL
                 AND push_notifications = true"#,
            user_ids,
        )
        .fetch_all(&self.db)
        .await?;

        let tokens = rows
            .into_iter()
            .filter_map(|r| r.push_notification_token.map(|t| (r.user_id, t)))
            .collect();

        Ok(tokens)
    }

    /// Clears a stale FCM token (called when FCM returns a 404 / TOKEN_NOT_REGISTERED).
    pub async fn clear_push_token(&self, user_id: Uuid) -> Result<(), NotificationError> {
        sqlx::query!(
            r#"UPDATE general_settings SET push_notification_token = NULL WHERE user_id = $1"#,
            user_id,
        )
        .execute(&self.db)
        .await
        .map_err(NotificationError::Database)?;

        Ok(())
    }

    /// Check whether a user has `app_notifications` enabled.
    pub async fn has_app_notifications_enabled(
        &self,
        user_id: Uuid,
    ) -> Result<bool, NotificationError> {
        let result = sqlx::query_scalar!(
            r#"SELECT app_notifications FROM general_settings WHERE user_id = $1"#,
            user_id,
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(result.unwrap_or(false))
    }
}
