use crate::modules::broadcast::dto::{
    BroadcastListItem, BroadcastQuery, BroadcastSortBy, ParticipantListItem, ParticipantQuery,
    ParticipantSortBy,
};
use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::broadcast::model::{
    Broadcast, BroadcastParticipant, EndReason, ParticipantRole,
};
use crate::shared::pagination::{Order};
use crate::shared::repository::{push_cursor_condition, push_order_and_limit};
use crate::shared::types::dto::UserSummary;
use sqlx::{Postgres, QueryBuilder, Transaction};
use std::collections::HashMap;
use time::OffsetDateTime;
use uuid::Uuid;

// ==================== INPUTS ====================
pub struct CreateBroadcastInput<'a> {
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub image_id: Option<&'a str>,
    pub image_url: Option<&'a str>,
    pub time_zone: Option<&'a str>,
    pub start_time: Option<OffsetDateTime>,
    pub recording_enabled: bool,
    pub creator_id: Uuid,
}

pub struct UpdateBroadcastInput<'a> {
    pub broadcast_id: Uuid,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub image_id: Option<&'a str>,
    pub image_url: Option<&'a str>,
    pub time_zone: Option<&'a str>,
    pub start_time: Option<OffsetDateTime>,
    pub recording_enabled: Option<bool>,
}

pub struct SetActiveInput {
    pub broadcast_id: Uuid,
    pub broadcast_token: String,
    pub start_time: OffsetDateTime,
}

pub struct UpsertParticipantInput<'a> {
    pub broadcast_id: Uuid,
    pub participant_id: Uuid,
    pub role: &'a ParticipantRole,
    pub joined_at: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct BroadcastRepository {
    db: sqlx::PgPool,
}

impl BroadcastRepository {
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }

    pub async fn create<'t>(
        &self,
        input: &CreateBroadcastInput<'_>,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<Broadcast, BroadcastError> {
        sqlx::query_as!(
            Broadcast,
            r#"INSERT INTO broadcasts (title, description, image_id, image_url, time_zone,
                        start_time, recording_enabled, creator_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING *"#,
            input.title,
            input.description,
            input.image_id,
            input.image_url,
            input.time_zone,
            input.start_time,
            input.recording_enabled,
            input.creator_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn update<'t>(
        &self,
        updates: &UpdateBroadcastInput<'_>,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<Broadcast, BroadcastError> {
        let mut query = QueryBuilder::new("UPDATE broadcasts SET updated_at = NOW()");

        if let Some(title) = updates.title {
            query.push(", title = ").push_bind(title);
        }

        if let Some(description) = updates.description {
            query.push(", description = ").push_bind(description);
        }

        if let Some(image_id) = updates.image_id {
            query.push(", image_id = ").push_bind(image_id);
        }

        if let Some(image_url) = updates.image_url {
            query.push(", image_url = ").push_bind(image_url);
        }

        if let Some(time_zone) = updates.time_zone {
            query.push(", time_zone = ").push_bind(time_zone);
        }

        if let Some(start_time) = updates.start_time {
            query.push(", start_time = ").push_bind(start_time);
        }

        if let Some(r_enabled) = updates.recording_enabled {
            query.push(", recording_enabled = ").push_bind(r_enabled);
        }

        query.push(" WHERE id = ").push_bind(updates.broadcast_id);

        query.push(" RETURNING *");

        query
            .build_query_as::<Broadcast>()
            .fetch_one(&mut **tx)
            .await
            .map_err(BroadcastError::Database)
    }

    pub async fn delete(&self, broadcast_id: Uuid) -> Result<(), BroadcastError> {
        let result = sqlx::query!(
            "UPDATE broadcasts SET deleted_at = NOW() WHERE id = $1 AND deleted_at IS NULL",
            broadcast_id,
        )
        .execute(&self.db)
        .await
        .map_err(BroadcastError::Database)?;

        if result.rows_affected() == 0 {
            tracing::warn!("No broadcast found to delete: {}", broadcast_id);
        }

        Ok(())
    }

    /// Fetch a page of broadcasts with creator info and total participant count
    /// joined in a single SQL query.
    ///
    /// ## Why not `sqlx::query_as!` / `query!` macros here?
    /// The dynamic WHERE clause built by `QueryBuilder` is incompatible with
    /// compile-time query verification. We use `build_query_as` instead and
    /// accept runtime type mapping via `FromRow`.
    ///
    /// ## Tracing
    /// A span is opened here, so slow queries surface in the trace tree with the
    /// filter parameters attached. Bind parameters are not logged to avoid
    /// leaking PII (keywords, user IDs) in structured logs.
    #[tracing::instrument(name = "broadcast_repo.find")]
    pub async fn find_broadcasts(
        &self,
        query: &BroadcastQuery,
        requester_id: Option<Uuid>,
    ) -> Result<Vec<BroadcastListItem>, BroadcastError> {
        let order = query.effective_order();
        let sort = query.effective_sort();
        let dir = order.sql();
        let op = order.cursor_op();

        // Decode the cursor, whose shape depends on the sort field.
        // We do this first to display any possible errors before touching the DB
        let cursor_ts1: Option<OffsetDateTime>;
        let cursor_ts2: Option<OffsetDateTime>;
        let cursor_score: Option<i64>;
        let cursor_id: Option<Uuid>;

        match sort {
            BroadcastSortBy::TotalParticipants => {
                cursor_ts1 = None;
                cursor_ts2 = None;
                match query.cursor() {
                    None => {
                        cursor_id = None;
                        cursor_score = None;
                    }
                    Some(c) => {
                        let (score, id) = c.to_score_id().map_err(BroadcastError::Cursor)?;
                        cursor_score = Some(score);
                        cursor_id = Some(id);
                    }
                }
            }
            _ => {
                cursor_score = None;
                match query.cursor() {
                    None => {
                        cursor_ts1 = None;
                        cursor_ts2 = None;
                        cursor_id = None;
                    }
                    Some(c) => {
                        let (ts, id) = c.to_timestamp_id().map_err(BroadcastError::Cursor)?;
                        cursor_ts1 = Some(ts);
                        cursor_ts2 = Some(ts);
                        cursor_id = Some(id);
                    }
                }
            }
        }

        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            r#"
            SELECT
                b.id,
                b.title,
                b.description,
                b.time_zone,
                b.image_url,
                b.image_id,
                b.status,
                b.created_at,
                b.start_time,
                b.end_time,
                b.creator_id,
                (
                    SELECT COUNT(*) FROM broadcast_participants bp
                    WHERE bp.broadcast_id = b.id
                ) AS total_participants,
                COALESCE(u.full_name, 'Unknown') AS creator_name,
                u.avatar_url AS creator_avatar_url,
                u.avatar_id AS creator_avatar_id
            FROM broadcasts b
            LEFT JOIN users u ON  u.id = b.creator_id AND u.deleted_at IS NULL
            WHERE b.deleted_at IS NULL
            "#,
        );

        // Filters
        if let Some(cid) = query.creator_id {
            qb.push(" AND b.creator_id = ").push_bind(cid);
        }
        if let Some(status) = &query.status {
            qb.push(" AND b.status = ").push_bind(status);
        }
        if query.only_subscriptions.unwrap_or(false) {
            if let Some(vid) = requester_id {
                qb.push(
                    " AND b.creator_id IN (
                        SELECT subscription_id
                        FROM user_subscribers
                        WHERE subscriber_id = ",
                )
                .push_bind(vid)
                .push(")");
            }
        }
        if let Some(ref kw) = query.keywords {
            qb.push(
                r#" AND to_tsvector('english', b.title || ' ' || b.description)
                    @@ plainto_tsquery('english', "#,
            )
            .push_bind(kw.trim())
            .push(")");
        }

        if let Some(v) = query.start_time_gt {
            qb.push(" AND b.start_time > ").push_bind(v);
        }
        if let Some(v) = query.start_time_gte {
            qb.push(" AND b.start_time >= ").push_bind(v);
        }
        if let Some(v) = query.start_time_lt {
            qb.push(" AND b.start_time < ").push_bind(v);
        }
        if let Some(v) = query.start_time_lte {
            qb.push(" AND b.start_time <= ").push_bind(v);
        }

        if let Some(v) = query.end_time_gt {
            qb.push(" AND b.end_time > ").push_bind(v);
        }
        if let Some(v) = query.end_time_gte {
            qb.push(" AND b.end_time >= ").push_bind(v);
        }
        if let Some(v) = query.end_time_lt {
            qb.push(" AND b.end_time < ").push_bind(v);
        }
        if let Some(v) = query.end_time_lte {
            qb.push(" AND b.end_time <= ").push_bind(v);
        }

        if let Some(exists) = query.start_time_exists {
            qb.push(if exists {
                " AND b.start_time IS NOT NULL"
            } else {
                " AND b.start_time IS NULL"
            });
        }
        if let Some(exists) = query.end_time_exists {
            qb.push(if exists {
                " AND b.end_time IS NOT NULL"
            } else {
                " AND b.end_time IS NULL"
            });
        }

        // Cursor Condition
        // Each sort field has a corresponding cursor column.  The cursor
        // condition must use the EXACT same expression as ORDER BY.
        match sort {
            BroadcastSortBy::CreatedAt => {
                push_cursor_condition(
                    &mut qb,
                    "b.created_at",
                    "b.id",
                    cursor_ts1,
                    cursor_id,
                    order,
                );
            }
            BroadcastSortBy::Title => {
                push_cursor_condition(
                    &mut qb,
                    "b.created_at",
                    "b.id",
                    cursor_ts1,
                    cursor_id,
                    order,
                );
            }
            BroadcastSortBy::StartTime => {
                if let (Some(ts), Some(id)) = (cursor_ts1, cursor_id) {
                    let op_str = op;
                    qb.push(format!(" AND (b.start_time, b.id) {op_str} ("))
                        .push_bind(ts)
                        .push(", ")
                        .push_bind(id)
                        .push(")");
                }
            }
            BroadcastSortBy::EndTime => {
                if let (Some(ts1), Some(ts2), Some(id)) = (cursor_ts1, cursor_ts2, cursor_id) {
                    let op_str = op;
                    qb.push(format!(
                        " AND (COALESCE(b.end_time, b.created_at), b.id) {op_str} ("
                    ))
                    .push_bind(ts2)
                    .push(", ")
                    .push_bind(ts1)
                    .push(", ")
                    .push_bind(id)
                    .push(")");
                }
            }
            BroadcastSortBy::TotalParticipants => {
                if let (Some(score), Some(id)) = (cursor_score, cursor_id) {
                    let op_str = op;
                    qb.push(format!(
                        " AND (
                            (SELECT COUNT(*)::BIGINT FROM broadcast_participants bp2
                             WHERE bp2.broadcast_id = b.id),
                            b.id
                        ) {op_str} ("
                    ))
                    .push_bind(score)
                    .push(", ")
                    .push_bind(id)
                    .push(")");
                }
            }
        }

        // Order by
        // Always include `b.id` as a tiebreaker so the page boundary is
        // deterministic even when the primary sort column has duplicates.
        match sort {
            BroadcastSortBy::CreatedAt => {
                qb.push(format!(" ORDER BY b.created_at {dir}, b.id {dir}"));
            }
            BroadcastSortBy::Title => {
                qb.push(format!(
                    " ORDER BY b.title {dir}, b.created_at {dir}, b.id {dir}"
                ));
            }
            BroadcastSortBy::StartTime => {
                qb.push(format!(
                    " ORDER BY b.start_time {dir} NULLS LAST, b.id {dir}"
                ));
            }
            BroadcastSortBy::EndTime => {
                qb.push(format!(
                    " ORDER BY COALESCE(b.end_time, b.created_at) {dir}, b.id {dir}"
                ));
            }
            BroadcastSortBy::TotalParticipants => {
                qb.push(format!(" ORDER BY total_participants {dir}, b.id {dir}"));
            }
        }

        // Fetch one extra row to determine has_next_page.
        qb.push(" LIMIT ").push_bind(query.limit_plus_one());

        let rows = qb
            .build_query_as::<BroadcastListItem>()
            .fetch_all(&self.db)
            .await?;

        tracing::debug!(returned = rows.len(), "broadcast list query complete");

        Ok(rows)
    }

    pub async fn find_participants(
        &self,
        broadcast_id: Uuid,
        query: &ParticipantQuery,
    ) -> Result<Vec<ParticipantListItem>, BroadcastError> {
        let cursor = query.cursor();
        let sort = query.sort_by.unwrap_or_default();
        let order = query.order.unwrap_or(Order::Asc);
        let dir = order.sql();
        let op = order.cursor_op();

        let (cursor_ts, cursor_id, cursor_name, cursor_role) = match (cursor, sort) {
            (None, _) => (None, None, None, None),
            (Some(c), ParticipantSortBy::JoinedAt) => {
                let (ts, id) = c.to_timestamp_id().map_err(BroadcastError::Cursor)?;
                (Some(ts), Some(id), None, None)
            }
            (Some(c), ParticipantSortBy::Name) => {
                // For name sorting, cursor is (name, id)
                let (name, id) = c.to_name_id().map_err(BroadcastError::Cursor)?;
                (None, Some(id), Some(name), None)
            }
            (Some(c), ParticipantSortBy::Role) => {
                let (priority, id) = c.to_score_id().map_err(BroadcastError::Cursor)?;
                let role = match priority {
                    0 => Some(ParticipantRole::Host),
                    1 => Some(ParticipantRole::Cohost),
                    2 => Some(ParticipantRole::Participant),
                    _ => None,
                };
                (None, Some(id), None, role)
            }
        };

        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            r#"
            SELECT
                u.id,
                u.full_name,
                u.avatar_id,
                u.avatar_url,
                bp.role,
                bp.joined_at
            FROM broadcast_participants bp
            INNER JOIN users u
                ON u.id = bp.participant_id
                AND u.deleted_at IS NULL
            WHERE bp.broadcast_id =
            "#,
        );
        qb.push_bind(broadcast_id);

        if let Some(role) = &query.role {
            qb.push(" AND bp.role = ").push_bind(role);
        }
        if let Some(ref kw) = query.keywords {
            if !kw.is_empty() {
                qb.push(" AND to_tsvector('english', u.full_name) @@ plainto_tsquery('english', ")
                    .push_bind(kw.clone())
                    .push(")");
            }
        }

        // Cursor condition based on the sort field
        match sort {
            ParticipantSortBy::Role => {
                if let (Some(r), Some(id)) = (cursor_role, cursor_id) {
                    qb.push(format!(" AND (bp.role, bp.participant_id) {} (", op))
                        .push_bind(r)
                        .push(", ")
                        .push_bind(id)
                        .push(")");
                }
            }
            ParticipantSortBy::JoinedAt => {
                if let (Some(ts), Some(id)) = (cursor_ts, cursor_id) {
                    qb.push(format!(" AND (bp.joined_at, bp.participant_id) {} (", op))
                        .push_bind(ts)
                        .push(", ")
                        .push_bind(id)
                        .push(")");
                }
            }
            ParticipantSortBy::Name => {
                if let (Some(n), Some(id)) = (cursor_name, cursor_id) {
                    qb.push(format!(" AND (u.full_name, bp.participant_id) {} (", op))
                        .push_bind(n)
                        .push(", ")
                        .push_bind(id)
                        .push(")");
                }
            }
        }

        // Order by
        match sort {
            ParticipantSortBy::Role => {
                qb.push(format!(
                    " ORDER BY \
                 CASE bp.role \
                     WHEN 'host' THEN 0 \
                     WHEN 'cohost' THEN 1 \
                     WHEN 'participant' THEN 2 \
                     ELSE 3 \
                 END {}, \
                 bp.participant_id {}",
                    dir, dir
                ));
            }
            ParticipantSortBy::JoinedAt => {
                qb.push(format!(
                    " ORDER BY bp.joined_at {}, bp.participant_id {}",
                    dir, dir
                ));
            }
            ParticipantSortBy::Name => {
                qb.push(format!(
                    " ORDER BY u.full_name {}, bp.participant_id {}",
                    dir, dir
                ));
            }
        }

        qb.push(" LIMIT ").push_bind(query.limit_plus_one());

        let rows = qb
            .build_query_as::<ParticipantListItem>()
            .fetch_all(&self.db)
            .await?;

        Ok(rows)
    }

    pub async fn find_live_participants(
        &self,
        broadcast_id: Uuid,
        query: &ParticipantQuery,
    ) -> Result<Vec<ParticipantListItem>, BroadcastError> {
        let (cursor_ts, cursor_id) = match query.cursor() {
            None => (None, None),
            Some(c) => {
                let (ts, id) = c.to_timestamp_id().map_err(|_| sqlx::Error::RowNotFound)?;
                (Some(ts), Some(id))
            }
        };

        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            r#"
            SELECT
                u.id,
                u.full_name,
                u.avatar_id,
                u.avatar_url,
                bp.role,
                bp.joined_at
            FROM broadcast_participants bp
            INNER JOIN users u
                ON u.id = bp.participant_id
                AND u.deleted_at IS NULL
            INNER JOIN broadcasts b
                ON b.id = bp.broadcast_id
                AND status = 'active'
                AND end_time IS NULL
                AND deleted_at IS NULL
            WHERE bp.broadcast_id =
            "#,
        );
        qb.push_bind(broadcast_id);

        if let Some(role) = &query.role {
            qb.push(" AND bp.role = ").push_bind(role);
        }
        if let Some(ref kw) = query.keywords {
            if !kw.is_empty() {
                qb.push(" AND to_tsvector('english', u.full_name) @@ plainto_tsquery('english', ")
                    .push_bind(kw.clone())
                    .push(")");
            }
        }

        push_cursor_condition(
            &mut qb,
            "bp.joined_at",
            "bp.participant_id",
            cursor_ts,
            cursor_id,
            Order::Asc,
        );

        push_order_and_limit(
            &mut qb,
            "bp.joined_at",
            "bp.participant_id",
            Order::Asc,
            query.limit_plus_one(),
        );

        let rows = qb
            .build_query_as::<ParticipantListItem>()
            .fetch_all(&self.db)
            .await?;

        Ok(rows)
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Broadcast>, BroadcastError> {
        sqlx::query_as!(
            Broadcast,
            r#"SELECT * FROM broadcasts WHERE id = $1 AND deleted_at IS NULL"#,
            id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn find_by_id_or_error(&self, id: Uuid) -> Result<Broadcast, BroadcastError> {
        self.find_by_id(id).await?.ok_or(BroadcastError::NotFound)
    }

    /// Find an active broadcast where the given user with `user_id` is the host (creator)
    /// Returns the broadcast if found, otherwise None
    /// An "active" broadcast has status = 'active' and is not deleted
    pub async fn find_active_broadcast_hosted_by_id(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Broadcast>, BroadcastError> {
        sqlx::query_as!(
            Broadcast,
            r#"SELECT * FROM broadcasts
               WHERE creator_id = $1 AND status = 'active' AND deleted_at IS NULL"#,
            user_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    /// Check if a user is hosting ANY active broadcast (returns bool)
    pub async fn is_active_host(&self, user_id: Uuid) -> Result<bool, BroadcastError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS (
                    SELECT 1
                    FROM broadcasts
                    WHERE creator_id = $1 AND status = 'active' AND deleted_at IS NULL
            ) AS "exists!""#,
            user_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn add_cohosts<'t>(
        &self,
        broadcast_id: Uuid,
        cohost_ids: &[Uuid],
        invited_by: Uuid,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<(), BroadcastError> {
        if cohost_ids.is_empty() {
            return Ok(());
        }

        sqlx::query!(
            r#"INSERT INTO broadcast_cohosts (broadcast_id, cohost_id, invited_by)
               SELECT $1, unnest_id, $2
               FROM UNNEST($3::uuid[]) AS unnest_id
               ON CONFLICT (broadcast_id, cohost_id) DO NOTHING"#,
            broadcast_id,
            invited_by,
            cohost_ids,
        )
        .execute(&mut **tx)
        .await
        .map_err(BroadcastError::Database)?;

        Ok(())
    }

    pub async fn is_cohost(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, BroadcastError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS (
                    SELECT 1 FROM broadcast_cohosts
                    WHERE broadcast_id = $1 AND cohost_id = $2
            ) AS "exists!""#,
            broadcast_id,
            user_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn find_users_batch(&self, ids: &[Uuid]) -> Result<Vec<UserSummary>, BroadcastError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as!(
            UserSummary,
            r#"SELECT id, full_name, bio,  avatar_id, avatar_url
               FROM users
               WHERE id = ANY($1) AND deleted_at IS NULL"#,
            ids
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn find_participant(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<Option<BroadcastParticipant>, BroadcastError> {
        sqlx::query_as!(
            BroadcastParticipant,
            r#"SELECT * FROM broadcast_participants
               WHERE broadcast_id = $1 AND participant_id = $2"#,
            broadcast_id,
            participant_id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    /// Find an active participant (not left) by user ID across any broadcast
    /// Returns the first active broadcast participant found for the user
    pub async fn find_active_participant(
        &self,
        user_id: Uuid,
    ) -> Result<Option<BroadcastParticipant>, BroadcastError> {
        sqlx::query_as!(
            BroadcastParticipant,
            r#"SELECT * FROM broadcast_participants
               WHERE participant_id = $1 AND left_at IS NULL
               LIMIT 1"#,
            user_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn upsert_participant_tx<'t>(
        &self,
        input: &UpsertParticipantInput<'_>,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<(), BroadcastError> {
        sqlx::query!(
            r#"INSERT INTO broadcast_participants (broadcast_id, participant_id, role, joined_at, left_at)
               VALUES ($1, $2, $3::text, $4, NULL)
               ON CONFLICT (broadcast_id, participant_id)
               DO UPDATE SET role = EXCLUDED.role, joined_at = EXCLUDED.joined_at"#,
            input.broadcast_id,
            input.participant_id,
            input.role.to_string(),
            input.joined_at,
        )
            .execute(&mut **tx)
            .await
            .map_err(BroadcastError::Database)?;
        Ok(())
    }

    pub async fn remove_participant(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), BroadcastError> {
        sqlx::query!(
            "DELETE FROM broadcast_participants WHERE broadcast_id = $1 and participant_id = $2",
            broadcast_id,
            user_id
        )
        .execute(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(())
    }

    pub async fn remove_participant_tx<'t>(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<(), BroadcastError> {
        sqlx::query!(
            "DELETE FROM broadcast_participants WHERE broadcast_id = $1 and participant_id = $2",
            broadcast_id,
            user_id
        )
        .execute(&mut **tx)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(())
    }

    pub async fn get_participant_ids(
        &self,
        broadcast_id: Uuid,
    ) -> Result<Vec<Uuid>, BroadcastError> {
        sqlx::query_scalar!(
            r#"SELECT participant_id FROM broadcast_participants
               WHERE broadcast_id = $1 AND left_at IS NULL"#,
            broadcast_id
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn get_participant_ids_and_clear<'t>(
        &self,
        broadcast_id: Uuid,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<Vec<Uuid>, BroadcastError> {
        sqlx::query_scalar!(
            r#"WITH deleted_participants AS (
                    UPDATE broadcast_participants
                    SET left_at = NOW()
                    WHERE broadcast_id = $1 AND left_at IS NULL
                    RETURNING *
               )
               SELECT participant_id
               FROM deleted_participants
               ORDER BY joined_at"#,
            broadcast_id
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn get_total_participants(&self, broadcast_id: Uuid) -> Result<i64, BroadcastError> {
        sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM broadcast_participants WHERE broadcast_id = $1"#,
            broadcast_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn find_user_summary(&self, id: Uuid) -> Result<Option<UserSummary>, BroadcastError> {
        sqlx::query_as!(
            UserSummary,
            r#"SELECT id, full_name, bio, avatar_id, avatar_url
               FROM users WHERE id = $1 AND deleted_at IS NULL"#,
            id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn is_subscribed(
        &self,
        subscriber_id: Uuid,
        creator_id: Uuid,
    ) -> Result<bool, BroadcastError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS(
               SELECT 1 FROM user_subscribers
               WHERE subscriber_id = $1 AND subscription_id = $2
            ) AS "exists!""#,
            subscriber_id,
            creator_id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn is_bookmarked(
        &self,
        user_id: Uuid,
        broadcast_id: Uuid,
    ) -> Result<bool, BroadcastError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS(
               SELECT 1 FROM broadcast_bookmarks
               WHERE user_id = $1 AND broadcast_id = $2
            ) AS "exists!""#,
            user_id,
            broadcast_id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn set_active<'t>(
        &self,
        input: &SetActiveInput,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<Broadcast, BroadcastError> {
        sqlx::query_as!(
            Broadcast,
            r#"UPDATE broadcasts
               SET status = 'active', broadcast_token = $1, start_time = $2, updated_at = NOW()
               WHERE id = $3
               RETURNING *"#,
            input.broadcast_token,
            input.start_time,
            input.broadcast_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn set_inactive<'t>(
        &self,
        broadcast_id: Uuid,
        reason: &EndReason,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<(), BroadcastError> {
        let results = sqlx::query!(
            r#"UPDATE broadcasts
               SET status = 'inactive', end_time   = NOW(), end_reason = $2, updated_at = NOW()
               WHERE id = $1"#,
            broadcast_id,
            reason.to_string(),
        )
        .execute(&mut **tx)
        .await
        .map_err(BroadcastError::Database)?;

        if results.rows_affected() == 0 {
            tracing::warn!(
                broadcast_id = %broadcast_id,
                "set_inactive affected 0 rows — broadcast may not exist or already inactive"
            );
        }

        Ok(())
    }

    pub async fn get_subscriber_ids(
        &self,
        subscription_id: Uuid,
    ) -> Result<Vec<Uuid>, BroadcastError> {
        let rows = sqlx::query!(
            r#"SELECT subscriber_id FROM user_subscribers WHERE subscription_id = $1"#,
            subscription_id
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(rows.into_iter().map(|r| r.subscriber_id).collect())
    }

    pub async fn remove_cohost_tx<'t>(
        &self,
        broadcast_id: Uuid,
        cohost_id: Uuid,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<(), BroadcastError> {
        sqlx::query!(
            "DELETE FROM broadcast_cohosts WHERE cohost_id = $1 AND broadcast_id = $2",
            cohost_id,
            broadcast_id,
        )
        .execute(&mut **tx)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(())
    }

    pub async fn get_cohosts(
        &self,
        broadcast_id: Uuid,
    ) -> Result<Vec<UserSummary>, BroadcastError> {
        sqlx::query_as!(
            UserSummary,
            r#"SELECT u.id, u.full_name, u.bio, u.avatar_id, u.avatar_url
               FROM broadcast_cohosts bc
               JOIN users u ON u.id = bc.cohost_id
               WHERE bc.broadcast_id = $1 AND bc.removed_at IS NULL AND deleted_at IS NUll"#,
            broadcast_id,
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    /// Fetch all current participant roles for a broadcast in one query.
    pub async fn get_participant_roles_batch(
        &self,
        broadcast_id: Uuid,
    ) -> Result<HashMap<Uuid, ParticipantRole>, BroadcastError> {
        let rows = sqlx::query!(
            r#"SELECT participant_id, role
               FROM broadcast_participants
               WHERE broadcast_id = $1 AND left_at IS NULL"#,
            broadcast_id
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| (r.participant_id, ParticipantRole::from(r.role)))
            .collect())
    }
}
