## Repository
```rust
use crate::modules::broadcast::dto::{
    BroadcastOrderBy, BroadcastParams, BroadcastSortBy, CreateBroadcastRequest, ParticipantSummary,
};
use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::broadcast::model::{
    Broadcast, BroadcastCohost, BroadcastParticipant, BroadcastStatus, CohostInvitation,
    EndReason, ParticipantRole,
};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

// ── Input structs ────────────────────────────────────────────────────────────
// Keeping inputs as explicit structs (not re-using request DTOs) means the
// repository stays decoupled from the HTTP layer.

pub struct CreateBroadcastInput<'a> {
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub image_id: Option<&'a str>,
    pub image_url: Option<&'a str>,
    pub time_zone: &'a str,
    pub start_time: Option<OffsetDateTime>,
    pub recording_enabled: bool,
    pub creator_id: Uuid,
}

pub struct SetActiveInput {
    pub broadcast_id: Uuid,
    pub broadcast_token: String,
}

pub struct UpsertParticipantInput {
    pub broadcast_id: Uuid,
    pub participant_id: Uuid,
    pub role: ParticipantRole,
    pub joined_at: OffsetDateTime,
}

pub struct BroadcastListRow {
    pub broadcast: Broadcast,
    pub creator_full_name: String,
    pub creator_avatar_id: Option<String>,
    pub creator_avatar_url: Option<String>,
    pub total_participants: i64,
}

// ── Repository ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct BroadcastRepository {
    db: PgPool,
}

impl BroadcastRepository {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    // ── Broadcast CRUD ───────────────────────────────────────────────────────

    pub async fn create<'t>(
        &self,
        input: &CreateBroadcastInput<'_>,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<Broadcast, BroadcastError> {
        // Always runs inside a transaction — no Option<tx> needed here.
        // The service is responsible for beginning / committing the transaction.
        sqlx::query_as!(
            Broadcast,
            r#"INSERT INTO broadcasts
               (title, description, image_id, image_url, time_zone, start_time,
                recording_enabled, creator_id)
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

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Broadcast>, BroadcastError> {
        sqlx::query_as!(
            Broadcast,
            r#"SELECT * FROM broadcasts WHERE id = $1 AND deleted_at IS NULL"#,
            id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    /// Fetch a broadcast or return `BroadcastError::NotFound`.
    pub async fn find_by_id_or_error(&self, id: Uuid) -> Result<Broadcast, BroadcastError> {
        self.find_by_id(id)
            .await?
            .ok_or(BroadcastError::NotFound)
    }

    pub async fn find_active_hosted_by(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Broadcast>, BroadcastError> {
        sqlx::query_as!(
            Broadcast,
            r#"SELECT * FROM broadcasts
               WHERE creator_id = $1 AND status = 'active' AND deleted_at IS NULL
               LIMIT 1"#,
            user_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    /// Sets status = active and stores the LiveKit room token.
    /// Runs inside the caller's transaction.
    pub async fn set_active<'t>(
        &self,
        input: &SetActiveInput,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<Broadcast, BroadcastError> {
        sqlx::query_as!(
            Broadcast,
            r#"UPDATE broadcasts
               SET status = 'active', broadcast_token = $2, updated_at = NOW()
               WHERE id = $1
               RETURNING *"#,
            input.broadcast_id,
            input.broadcast_token,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(BroadcastError::Database)
    }

    /// Sets status = inactive, clears the token, and stamps end_time.
    pub async fn set_inactive(
        &self,
        broadcast_id: Uuid,
        reason: &EndReason,
    ) -> Result<(), BroadcastError> {
        // end_reason stored as text; serde serialises the snake_case variant name.
        let reason_str = serde_json::to_string(reason)
            .map_err(|e| BroadcastError::Internal(e.into()))?
            .trim_matches('"')
            .to_string();

        sqlx::query!(
            r#"UPDATE broadcasts
               SET status     = 'inactive',
                   broadcast_token = NULL,
                   end_time   = NOW(),
                   end_reason = $2,
                   updated_at = NOW()
               WHERE id = $1"#,
            broadcast_id,
            reason_str,
        )
        .execute(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(())
    }

    pub async fn soft_delete(&self, broadcast_id: Uuid) -> Result<(), BroadcastError> {
        sqlx::query!(
            r#"UPDATE broadcasts SET deleted_at = NOW(), updated_at = NOW() WHERE id = $1"#,
            broadcast_id
        )
        .execute(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(())
    }

    /// Dynamic list query with all filters from `BroadcastParams`.
    /// Returns (rows, total_count) for pagination.
    pub async fn list(
        &self,
        params: &BroadcastParams,
        viewer_id: Option<Uuid>,
    ) -> Result<(Vec<BroadcastListRow>, i64), BroadcastError> {
        // Build the shared WHERE clause separately so we can reuse it for COUNT.
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            r#"SELECT
                 b.*,
                 u.full_name  AS creator_full_name,
                 u.avatar_id  AS creator_avatar_id,
                 u.avatar_url AS creator_avatar_url,
                 (SELECT COUNT(*) FROM broadcast_participants bp
                  WHERE bp.broadcast_id = b.id) AS total_participants
               FROM broadcasts b
               JOIN users u ON u.id = b.creator_id
               WHERE b.deleted_at IS NULL"#,
        );

        Self::apply_filters(&mut qb, params, viewer_id);

        let sort_col = match params.sort_by.unwrap_or_default() {
            BroadcastSortBy::CreatedAt => "b.created_at",
            BroadcastSortBy::Title => "b.title",
            BroadcastSortBy::StartTime => "b.start_time",
            BroadcastSortBy::EndTime => "COALESCE(b.end_time, b.created_at)",
            BroadcastSortBy::TotalParticipants => "total_participants",
        };
        let order_dir = match params.order.unwrap_or_default() {
            BroadcastOrderBy::Asc => "ASC NULLS LAST",
            BroadcastOrderBy::Desc => "DESC NULLS LAST",
        };
        qb.push(format!(" ORDER BY {} {}", sort_col, order_dir));

        let limit = params.limit.unwrap_or(20).clamp(1, 100);
        let offset = (params.page.unwrap_or(1).max(1) - 1) * limit;
        qb.push(" LIMIT ").push_bind(limit).push(" OFFSET ").push_bind(offset);

        // sqlx QueryBuilder doesn't map directly to a struct — use fetch_all on the
        // built query and map manually. For the full version you'd derive FromRow on
        // BroadcastListRow; shown here as a pattern:
        let rows = qb
            .build_query_as::<BroadcastListRow>()
            .fetch_all(&self.db)
            .await
            .map_err(BroadcastError::Database)?;

        // COUNT with same filters (no ORDER BY, no LIMIT)
        let mut count_qb: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM broadcasts b WHERE b.deleted_at IS NULL");
        Self::apply_filters(&mut count_qb, params, viewer_id);
        let total: i64 = count_qb
            .build_query_scalar()
            .fetch_one(&self.db)
            .await
            .map_err(BroadcastError::Database)?;

        Ok((rows, total))
    }

    fn apply_filters(
        qb: &mut QueryBuilder<Postgres>,
        params: &BroadcastParams,
        viewer_id: Option<Uuid>,
    ) {
        if let Some(id) = params.id {
            qb.push(" AND b.id = ").push_bind(id);
        }
        if let Some(ref s) = params.status {
            // BroadcastStatus serialises to "active"/"inactive"
            let s_str = match s {
                crate::modules::broadcast::model::BroadcastStatus::Active => "active",
                crate::modules::broadcast::model::BroadcastStatus::Inactive => "inactive",
            };
            qb.push(" AND b.status = ").push_bind(s_str);
        }
        if let Some(cid) = params.creator_id {
            qb.push(" AND b.creator_id = ").push_bind(cid);
        }
        if let Some(xcid) = params.exclude_creator_id {
            qb.push(" AND b.creator_id <> ").push_bind(xcid);
        }
        if params.only_subscriptions.unwrap_or(false) {
            if let Some(vid) = viewer_id {
                qb.push(
                    " AND b.creator_id IN (SELECT subscription_id FROM user_subscribers WHERE subscriber_id = "
                )
                .push_bind(vid)
                .push(")");
            }
        }
        if let Some(ref kw) = params.keywords {
            qb.push(
                " AND to_tsvector('english', b.title || ' ' || COALESCE(b.description, '')) \
                 @@ plainto_tsquery('english', ",
            )
            .push_bind(kw)
            .push(")");
        }
        if params.recently_ended.unwrap_or(false) {
            qb.push(
                " AND b.status = 'inactive' AND b.end_time > NOW() - INTERVAL '24 hours'",
            );
        }
        if let Some(true) = params.has_recording {
            qb.push(" AND b.recording_url IS NOT NULL AND b.published_at IS NOT NULL");
        }
        // start_time range filters
        if let Some(v) = params.start_time_gt {
            qb.push(" AND b.start_time > ").push_bind(v);
        }
        if let Some(v) = params.start_time_gte {
            qb.push(" AND b.start_time >= ").push_bind(v);
        }
        if let Some(v) = params.start_time_lt {
            qb.push(" AND b.start_time < ").push_bind(v);
        }
        if let Some(v) = params.start_time_lte {
            qb.push(" AND b.start_time <= ").push_bind(v);
        }
        if let Some(exists) = params.start_time_exists {
            if exists {
                qb.push(" AND b.start_time IS NOT NULL");
            } else {
                qb.push(" AND b.start_time IS NULL");
            }
        }
        // end_time range filters
        if let Some(v) = params.end_time_gt {
            qb.push(" AND b.end_time > ").push_bind(v);
        }
        if let Some(v) = params.end_time_gte {
            qb.push(" AND b.end_time >= ").push_bind(v);
        }
        if let Some(v) = params.end_time_lt {
            qb.push(" AND b.end_time < ").push_bind(v);
        }
        if let Some(v) = params.end_time_lte {
            qb.push(" AND b.end_time <= ").push_bind(v);
        }
        if let Some(exists) = params.end_time_exists {
            if exists {
                qb.push(" AND b.end_time IS NOT NULL");
            } else {
                qb.push(" AND b.end_time IS NULL");
            }
        }
    }

    // ── Participant helpers ───────────────────────────────────────────────────

    pub async fn upsert_participant<'t>(
        &self,
        input: &UpsertParticipantInput,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<(), BroadcastError> {
        let role_str = match input.role {
            ParticipantRole::Host => "host",
            ParticipantRole::Cohost => "cohost",
            ParticipantRole::Participant => "participant",
            ParticipantRole::None => return Ok(()), // defensive — never stored
        };
        sqlx::query!(
            r#"INSERT INTO broadcast_participants (broadcast_id, participant_id, role, joined_at)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (broadcast_id, participant_id)
                 DO UPDATE SET role = EXCLUDED.role, joined_at = EXCLUDED.joined_at"#,
            input.broadcast_id,
            input.participant_id,
            role_str,
            input.joined_at,
        )
        .execute(&mut **tx)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(())
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

    pub async fn remove_participant(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<(), BroadcastError> {
        sqlx::query!(
            r#"DELETE FROM broadcast_participants
               WHERE broadcast_id = $1 AND participant_id = $2"#,
            broadcast_id,
            participant_id,
        )
        .execute(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(())
    }

    pub async fn clear_all_participants(
        &self,
        broadcast_id: Uuid,
    ) -> Result<(), BroadcastError> {
        sqlx::query!(
            r#"DELETE FROM broadcast_participants WHERE broadcast_id = $1"#,
            broadcast_id
        )
        .execute(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(())
    }

    pub async fn get_all_participant_ids(
        &self,
        broadcast_id: Uuid,
    ) -> Result<Vec<Uuid>, BroadcastError> {
        let rows = sqlx::query!(
            r#"SELECT participant_id FROM broadcast_participants WHERE broadcast_id = $1"#,
            broadcast_id
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(rows.into_iter().map(|r| r.participant_id).collect())
    }

    pub async fn get_host_and_cohost_ids(
        &self,
        broadcast_id: Uuid,
    ) -> Result<Vec<Uuid>, BroadcastError> {
        let rows = sqlx::query!(
            r#"SELECT participant_id FROM broadcast_participants
               WHERE broadcast_id = $1 AND role IN ('host', 'cohost')"#,
            broadcast_id
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(rows.into_iter().map(|r| r.participant_id).collect())
    }

    pub async fn count_active(&self) -> Result<i64, BroadcastError> {
        let row = sqlx::query!(
            r#"SELECT COUNT(*) AS count FROM broadcasts WHERE status = 'active' AND deleted_at IS NULL"#
        )
        .fetch_one(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(row.count.unwrap_or(0))
    }

    // ── Cohost helpers ───────────────────────────────────────────────────────

    /// Bulk-insert cohosts in a single statement. Uses UNNEST to avoid
    /// per-row round-trips. Safe to call with an empty slice.
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

    pub async fn remove_cohost(
        &self,
        broadcast_id: Uuid,
        cohost_id: Uuid,
    ) -> Result<(), BroadcastError> {
        sqlx::query!(
            r#"UPDATE broadcast_cohosts
               SET removed_at = NOW()
               WHERE broadcast_id = $1 AND cohost_id = $2 AND removed_at IS NULL"#,
            broadcast_id,
            cohost_id,
        )
        .execute(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(())
    }

    pub async fn is_cohost(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, BroadcastError> {
        let row = sqlx::query!(
            r#"SELECT EXISTS(
               SELECT 1 FROM broadcast_cohosts
               WHERE broadcast_id = $1 AND cohost_id = $2 AND removed_at IS NULL
            ) AS "exists!""#,
            broadcast_id,
            user_id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(row.exists)
    }

    pub async fn get_cohosts(
        &self,
        broadcast_id: Uuid,
    ) -> Result<Vec<ParticipantSummary>, BroadcastError> {
        sqlx::query_as!(
            ParticipantSummary,
            r#"SELECT u.id, u.full_name, u.avatar_id, u.avatar_url
               FROM broadcast_cohosts bc
               JOIN users u ON u.id = bc.cohost_id
               WHERE bc.broadcast_id = $1 AND bc.removed_at IS NULL
                 AND u.deleted_at IS NULL"#,
            broadcast_id
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    // ── User helpers ─────────────────────────────────────────────────────────

    /// Fetch multiple user summaries in a single query.
    /// Returns only found users — caller validates length matches input.
    pub async fn find_users_batch(
        &self,
        ids: &[Uuid],
    ) -> Result<Vec<ParticipantSummary>, BroadcastError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        sqlx::query_as!(
            ParticipantSummary,
            r#"SELECT id, full_name, avatar_id, avatar_url
               FROM users
               WHERE id = ANY($1) AND deleted_at IS NULL"#,
            ids,
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    pub async fn find_user_summary(
        &self,
        id: Uuid,
    ) -> Result<Option<ParticipantSummary>, BroadcastError> {
        sqlx::query_as!(
            ParticipantSummary,
            r#"SELECT id, full_name, avatar_id, avatar_url
               FROM users WHERE id = $1 AND deleted_at IS NULL"#,
            id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }

    // ── Subscription / bookmark helpers ──────────────────────────────────────

    pub async fn is_subscribed(
        &self,
        subscriber_id: Uuid,
        creator_id: Uuid,
    ) -> Result<bool, BroadcastError> {
        let row = sqlx::query!(
            r#"SELECT EXISTS(
               SELECT 1 FROM user_subscribers
               WHERE subscriber_id = $1 AND subscription_id = $2
            ) AS "exists!""#,
            subscriber_id,
            creator_id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(row.exists)
    }

    pub async fn is_bookmarked(
        &self,
        user_id: Uuid,
        broadcast_id: Uuid,
    ) -> Result<bool, BroadcastError> {
        let row = sqlx::query!(
            r#"SELECT EXISTS(
               SELECT 1 FROM broadcast_bookmarks
               WHERE user_id = $1 AND broadcast_id = $2
            ) AS "exists!""#,
            user_id,
            broadcast_id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(row.exists)
    }

    pub async fn get_subscriber_ids(
        &self,
        creator_id: Uuid,
    ) -> Result<Vec<Uuid>, BroadcastError> {
        let rows = sqlx::query!(
            r#"SELECT subscriber_id FROM user_subscribers WHERE subscription_id = $1"#,
            creator_id,
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)?;
        Ok(rows.into_iter().map(|r| r.subscriber_id).collect())
    }
}
```

## Service
```rust
use crate::modules::broadcast::dto::{
    BroadcastEndedPayload, BroadcastParams, BroadcastResponse, BroadcastSessionResponse,
    CohostSessionResponse, CreateBroadcastRequest, ParticipantSummary, UpdateBroadcastRequest,
    MAX_COHOSTS,
};
use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::broadcast::model::{
    Broadcast, BroadcastContext, BroadcastState, BroadcastStatus, EndReason, ParticipantRole,
};
use crate::modules::broadcast::repository::{
    BroadcastRepository, CreateBroadcastInput, SetActiveInput, UpsertParticipantInput,
};
use crate::shared::pagination::PaginationResponse;
use crate::shared::services::livekit::LivekitService;
use crate::shared::services::redis::RedisService;
use crate::shared::services::ws::hub::{WsHub, WsPayload};
use crate::state::MenoState;
use serde_json::json;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

// ── Service ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct BroadcastService {
    repo: BroadcastRepository,
    livekit: LivekitService,
    redis: RedisService,
    ws: WsHub,
}

impl BroadcastService {
    pub fn new(
        repo: BroadcastRepository,
        livekit: LivekitService,
        redis: RedisService,
        ws: WsHub,
    ) -> Self {
        Self { repo, livekit, redis, ws }
    }

    // ── Response builder ─────────────────────────────────────────────────────

    /// Central helper that converts a `Broadcast` DB row into a `BroadcastResponse`.
    ///
    /// **Call this from every service function that returns a `BroadcastResponse`.**
    /// It handles all computed fields (state, counts, viewer signals) in one place,
    /// eliminating the copy-paste that was happening in the old service.
    ///
    /// `ctx` carries viewer-specific and Redis-sourced values that cannot come from
    /// the DB row alone. Build it in each service method and pass it here.
    pub async fn broadcast_to_response(
        &self,
        broadcast: Broadcast,
        creator: ParticipantSummary,
        cohosts: Vec<ParticipantSummary>,
        ctx: BroadcastContext,
    ) -> Result<BroadcastResponse, BroadcastError> {
        // ── broadcast_state ──────────────────────────────────────────────────
        let broadcast_state = if broadcast.status == BroadcastStatus::Active {
            if ctx.is_reconnecting {
                BroadcastState::Reconnecting
            } else {
                BroadcastState::Live
            }
        } else {
            broadcast.get_state()
        };

        // ── duration_seconds ─────────────────────────────────────────────────
        let duration_seconds = match (broadcast.start_time, broadcast.end_time) {
            (Some(start), Some(end)) => {
                let diff = end - start;
                Some(diff.whole_seconds())
            }
            _ => None,
        };

        // ── end_reason ───────────────────────────────────────────────────────
        // The DB stores it as text; parse it back to the enum.
        let end_reason = broadcast.end_reason;

        Ok(BroadcastResponse {
            id: broadcast.id,
            title: broadcast.title,
            description: broadcast.description,
            time_zone: broadcast.time_zone,
            image_id: broadcast.image_id,
            image_url: broadcast.image_url,
            created_at: broadcast.created_at,
            start_time: broadcast.start_time,
            end_time: broadcast.end_time,
            published_at: broadcast.published_at,
            duration_seconds,
            status: broadcast.status,
            broadcast_state,
            viewer_role: ctx.viewer_role,
            is_subscribed_to_creator: ctx.is_subscribed_to_creator,
            is_bookmarked: ctx.is_bookmarked,
            live_participants_count: ctx.live_count,
            total_participants: ctx.total_count,
            recording_enabled: broadcast.recording_enabled,
            recording_url: broadcast.recording_url,
            end_reason,
            time_remaining_seconds: ctx.time_remaining_seconds,
            last_listened_at: ctx.last_listened_at,
            creator,
            cohosts,
        })
    }

    /// Gathers all viewer-specific signals for a broadcast in one batch.
    /// Runs the Redis + subscription + bookmark checks concurrently.
    async fn build_context(
        &self,
        broadcast: &Broadcast,
        viewer_id: Option<Uuid>,
        live_count: i64,
        total_count: i64,
    ) -> Result<BroadcastContext, BroadcastError> {
        // Check Redis host_grace key
        let grace_key = RedisService::host_grace_key(broadcast.id);
        let is_reconnecting = self.redis.exists(&grace_key).await.unwrap_or(false);

        let (viewer_role, is_subscribed, is_bookmarked) = match viewer_id {
            None => (ParticipantRole::None, false, false),
            Some(vid) => {
                // Run the three checks concurrently
                let (role_res, sub_res, bm_res) = tokio::join!(
                    self.get_viewer_role(broadcast, vid),
                    self.repo.is_subscribed(vid, broadcast.creator_id),
                    self.repo.is_bookmarked(vid, broadcast.id),
                );
                (role_res?, sub_res?, bm_res?)
            }
        };

        Ok(BroadcastContext {
            viewer_id,
            is_reconnecting,
            live_count,
            total_count,
            viewer_role,
            viewer_is_in_room: false, // populated per-endpoint when relevant
            is_subscribed_to_creator: is_subscribed,
            is_bookmarked,
            time_remaining_seconds: None,
            last_listened_at: None,
        })
    }

    async fn get_viewer_role(
        &self,
        broadcast: &Broadcast,
        viewer_id: Uuid,
    ) -> Result<ParticipantRole, BroadcastError> {
        if broadcast.creator_id == viewer_id {
            return Ok(ParticipantRole::Host);
        }
        match self.repo.find_participant(broadcast.id, viewer_id).await? {
            Some(p) => Ok(p.role),
            None => Ok(ParticipantRole::None),
        }
    }

    async fn get_live_count(&self, broadcast_id: Uuid) -> i64 {
        let key = RedisService::live_count_key(broadcast_id);
        self.redis.get_i64(&key).await.unwrap_or(0)
    }

    // ── Public service methods ────────────────────────────────────────────────

    /// Create a broadcast (draft or scheduled) with optional cohosts.
    ///
    /// The entire creation — broadcast row + cohost rows — runs in a single
    /// DB transaction. If cohost insertion fails, the broadcast is rolled back too.
    pub async fn create(
        &self,
        state: &MenoState,
        req: CreateBroadcastRequest,
        creator_id: Uuid,
    ) -> Result<BroadcastResponse, BroadcastError> {
        // ── Validate start_time ──────────────────────────────────────────────
        if let Some(st) = req.start_time {
            if st <= OffsetDateTime::now_utc() {
                return Err(BroadcastError::StartTimeInPast);
            }
        }

        // ── Validate cohosts ─────────────────────────────────────────────────
        let cohost_ids = req.cohosts.clone().unwrap_or_default();
        let cohosts: Vec<ParticipantSummary> = if cohost_ids.is_empty() {
            vec![]
        } else {
            if cohost_ids.len() > MAX_COHOSTS {
                return Err(BroadcastError::CohostLimitExceeded { max: MAX_COHOSTS });
            }
            if cohost_ids.contains(&creator_id) {
                return Err(BroadcastError::CannotAddSelfAsCohost);
            }
            // Deduplicate: the FE shouldn't send duplicates, but be defensive.
            let mut deduped = cohost_ids.clone();
            deduped.sort_unstable();
            deduped.dedup();

            let users = self.repo.find_users_batch(&deduped).await?;
            if users.len() != deduped.len() {
                return Err(BroadcastError::OneOrMoreUsersNotFound);
            }
            users
        };

        // ── DB transaction ───────────────────────────────────────────────────
        // Transactions belong in the service, not the repository. The repository
        // receives a &mut Transaction so it can participate without owning the
        // transaction boundary. This is the standard Rust/sqlx pattern.
        let mut tx = state.db.begin().await?;

        let broadcast = self
            .repo
            .create(
                &CreateBroadcastInput {
                    title: &req.title,
                    description: req.description.as_deref(),
                    image_id: req.image_id.as_deref(),
                    image_url: req.image_url.as_deref(),
                    time_zone: req.time_zone.as_deref().unwrap_or("Etc/UTC"),
                    start_time: req.start_time,
                    recording_enabled: req.recording_enabled.unwrap_or(false),
                    creator_id,
                },
                &mut tx,
            )
            .await?;

        if !cohost_ids.is_empty() {
            self.repo
                .add_cohosts(broadcast.id, &cohost_ids, creator_id, &mut tx)
                .await?;
        }

        tx.commit().await?;
        // ────────────────────────────────────────────────────────────────────

        // Schedule start notification job (fire-and-forget; not part of the tx).
        if broadcast.start_time.is_some() {
            // TODO: schedule apalis BroadcastStartJob here
        }

        // Fetch creator summary (we need the name + avatar for the response).
        // This could be combined into the INSERT…RETURNING with a CTE if you want
        // to save one round-trip, but the added query complexity isn't worth it yet.
        let creator = self
            .repo
            .find_user_summary(creator_id)
            .await?
            .ok_or(BroadcastError::UserNotFound)?;

        let ctx = BroadcastContext {
            viewer_id: Some(creator_id),
            viewer_role: ParticipantRole::Host,
            is_subscribed_to_creator: false, // creator can't subscribe to themselves
            is_bookmarked: false,
            live_count: 0,
            total_count: 0,
            ..Default::default()
        };

        self.broadcast_to_response(broadcast, creator, cohosts, ctx).await
    }

    /// Atomically start a broadcast: create the LiveKit room, mint the HOST
    /// token, set status=active, and insert the host as a participant.
    /// Fan-out notifications are spawned in the background so the response
    /// returns immediately.
    pub async fn go_live(
        &self,
        state: Arc<MenoState>,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<BroadcastSessionResponse, BroadcastError> {
        let broadcast = self.repo.find_by_id_or_error(broadcast_id).await?;

        if broadcast.creator_id != user_id {
            return Err(BroadcastError::NotCreator);
        }
        if broadcast.status == BroadcastStatus::Active {
            return Err(BroadcastError::AlreadyLive);
        }

        let creator = self
            .repo
            .find_user_summary(user_id)
            .await?
            .ok_or(BroadcastError::UserNotFound)?;

        // ── LiveKit: create room before touching the DB ───────────────────────
        // If LiveKit fails, the DB is untouched — no cleanup needed.
        self.livekit
            .create_room(broadcast_id)
            .await
            .map_err(|_| BroadcastError::LiveKitUnavailable)?;

        let token = self
            .livekit
            .create_token(user_id, &creator.full_name, broadcast_id, ParticipantRole::Host)?;

        // ── DB transaction ───────────────────────────────────────────────────
        let now = OffsetDateTime::now_utc();
        let mut tx = state.db.begin().await?;

        let updated = self
            .repo
            .set_active(
                &SetActiveInput {
                    broadcast_id,
                    broadcast_token: token.clone(),
                },
                &mut tx,
            )
            .await?;

        self.repo
            .upsert_participant(
                &UpsertParticipantInput {
                    broadcast_id,
                    participant_id: user_id,
                    role: ParticipantRole::Host,
                    joined_at: now,
                },
                &mut tx,
            )
            .await?;

        tx.commit().await?;
        // ────────────────────────────────────────────────────────────────────

        // ── Redis: set live count to 1 (host is the first participant) ───────
        let count_key = RedisService::live_count_key(broadcast_id);
        let _ = self.redis.set(&count_key, 1_i64).await;

        // ── Fan-out: notify subscribers (background task) ─────────────────────
        {
            let svc = self.clone();
            let ws = self.ws.clone();
            let broadcast_clone = updated.clone();
            let creator_clone = creator.clone();
            tokio::spawn(async move {
                if let Ok(subscriber_ids) =
                    svc.repo.get_subscriber_ids(broadcast_clone.creator_id).await
                {
                    // TODO: create in-app notifications for each subscriber
                    // notification_service.create_bulk(...)

                    if let Ok(response) = svc
                        .broadcast_to_response(
                            broadcast_clone,
                            creator_clone,
                            vec![],
                            BroadcastContext {
                                viewer_role: ParticipantRole::None,
                                live_count: 1,
                                ..Default::default()
                            },
                        )
                        .await
                    {
                        ws.send_to_users(
                            &subscriber_ids,
                            WsPayload {
                                event: "newBroadcast".into(),
                                data: serde_json::to_value(&response).unwrap_or_default(),
                            },
                        )
                        .await;
                    }
                }
            });
        }

        let cohosts = self.repo.get_cohosts(broadcast_id).await?;
        let ctx = BroadcastContext {
            viewer_id: Some(user_id),
            viewer_role: ParticipantRole::Host,
            live_count: 1,
            total_count: 1,
            ..Default::default()
        };

        let broadcast_response = self
            .broadcast_to_response(updated, creator, cohosts, ctx)
            .await?;

        Ok(BroadcastSessionResponse {
            broadcast: broadcast_response,
            token,
        })
    }

    /// Join a live broadcast. Returns a LiveKit token for the appropriate role.
    pub async fn join(
        &self,
        state: Arc<MenoState>,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<BroadcastSessionResponse, BroadcastError> {
        let broadcast = self.repo.find_by_id_or_error(broadcast_id).await?;
        if broadcast.status != BroadcastStatus::Active {
            return Err(BroadcastError::NotLive);
        }

        let user = self
            .repo
            .find_user_summary(user_id)
            .await?
            .ok_or(BroadcastError::UserNotFound)?;

        let role = if broadcast.creator_id == user_id {
            ParticipantRole::Host
        } else if self.repo.is_cohost(broadcast_id, user_id).await? {
            ParticipantRole::Cohost
        } else {
            ParticipantRole::Participant
        };

        let token =
            self.livekit
                .create_token(user_id, &user.full_name, broadcast_id, role.clone())?;

        let now = OffsetDateTime::now_utc();
        let mut tx = state.db.begin().await?;
        self.repo
            .upsert_participant(
                &UpsertParticipantInput {
                    broadcast_id,
                    participant_id: user_id,
                    role: role.clone(),
                    joined_at: now,
                },
                &mut tx,
            )
            .await?;
        tx.commit().await?;

        // Update Redis live count and notify host/cohosts
        let count_key = RedisService::live_count_key(broadcast_id);
        let new_count = self.redis.incr(&count_key).await.unwrap_or(0);

        {
            let svc = self.clone();
            let ws = self.ws.clone();
            let user_clone = user.clone();
            tokio::spawn(async move {
                if let Ok(host_ids) = svc.repo.get_host_and_cohost_ids(broadcast_id).await {
                    ws.send_to_users(
                        &host_ids,
                        WsPayload {
                            event: "newBroadcastListener".into(),
                            data: serde_json::to_value(&user_clone).unwrap_or_default(),
                        },
                    )
                    .await;
                }
                if let Ok(all_ids) = svc.repo.get_all_participant_ids(broadcast_id).await {
                    ws.send_to_users(
                        &all_ids,
                        WsPayload {
                            event: "numberOfLiveListeners".into(),
                            data: json!({ "broadcastId": broadcast_id, "count": new_count }),
                        },
                    )
                    .await;
                }
            });
        }

        let creator = self
            .repo
            .find_user_summary(broadcast.creator_id)
            .await?
            .ok_or(BroadcastError::UserNotFound)?;
        let cohosts = self.repo.get_cohosts(broadcast_id).await?;

        let ctx = self
            .build_context(&broadcast, Some(user_id), new_count, 0)
            .await?;

        let broadcast_response = self
            .broadcast_to_response(broadcast, creator, cohosts, ctx)
            .await?;

        Ok(BroadcastSessionResponse {
            broadcast: broadcast_response,
            token,
        })
    }

    /// End a broadcast. Idempotent — safe to call twice (e.g. grace-period task
    /// firing concurrently with an explicit endBroadcast WS event).
    pub async fn end_broadcast(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
        reason: EndReason,
    ) -> Result<(), BroadcastError> {
        let broadcast = self.repo.find_by_id_or_error(broadcast_id).await?;
        if broadcast.creator_id != user_id {
            // TODO: also allow admin role
            return Err(BroadcastError::CannotEnd);
        }
        if broadcast.status != BroadcastStatus::Active {
            return Ok(()); // already ended — idempotent
        }

        // Snapshot participant IDs before clearing them
        let participant_ids = self.repo.get_all_participant_ids(broadcast_id).await?;

        // DB: set inactive + clear participants atomically
        // Note: using two separate statements (no transaction) is fine here because
        // the set_inactive call stamps the canonical "ended" state first. A crash
        // between the two leaves orphaned participant rows that are harmless
        // (they won't affect counts since the broadcast is inactive).
        self.repo.set_inactive(broadcast_id, &reason).await?;
        self.repo.clear_all_participants(broadcast_id).await?;

        // LiveKit: delete room (kicks all media participants)
        let _ = self.livekit.delete_room(broadcast_id).await; // log on error, don't fail

        // Redis: clear live count
        let count_key = RedisService::live_count_key(broadcast_id);
        let _ = self.redis.del(&count_key).await;

        // WS: notify all participants
        let payload = WsPayload {
            event: "endedBroadcast".into(),
            data: serde_json::to_value(BroadcastEndedPayload { broadcast_id, reason })
                .unwrap_or_default(),
        };
        self.ws.send_to_users(&participant_ids, payload).await;

        // Update global live broadcast count
        if let Ok(count) = self.repo.count_active().await {
            self.ws
                .broadcast_all(WsPayload {
                    event: "numberOfLiveBroadcasts".into(),
                    data: json!({ "count": count }),
                })
                .await;
        }

        Ok(())
    }

    /// Leave a broadcast as a listener/cohost. Idempotent.
    pub async fn leave_broadcast(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), BroadcastError> {
        self.repo.remove_participant(broadcast_id, user_id).await?;

        let count_key = RedisService::live_count_key(broadcast_id);
        let new_count = {
            let n = self.redis.decr(&count_key).await.unwrap_or(0);
            if n < 0 {
                let _ = self.redis.set(&count_key, 0_i64).await;
                0
            } else {
                n
            }
        };

        let svc = self.clone();
        let ws = self.ws.clone();
        tokio::spawn(async move {
            if let Ok(host_ids) = svc.repo.get_host_and_cohost_ids(broadcast_id).await {
                ws.send_to_users(
                    &host_ids,
                    WsPayload {
                        event: "broadcastListenerLeft".into(),
                        data: json!({ "userId": user_id, "broadcastId": broadcast_id }),
                    },
                )
                .await;
            }
            if let Ok(all_ids) = svc.repo.get_all_participant_ids(broadcast_id).await {
                ws.send_to_users(
                    &all_ids,
                    WsPayload {
                        event: "numberOfLiveListeners".into(),
                        data: json!({ "broadcastId": broadcast_id, "count": new_count }),
                    },
                )
                .await;
            }
        });

        Ok(())
    }

    /// Get a single broadcast with all viewer-specific signals populated.
    pub async fn get_by_id(
        &self,
        broadcast_id: Uuid,
        viewer_id: Option<Uuid>,
    ) -> Result<BroadcastResponse, BroadcastError> {
        let broadcast = self.repo.find_by_id_or_error(broadcast_id).await?;

        let (creator, cohosts) = tokio::try_join!(
            async {
                self.repo
                    .find_user_summary(broadcast.creator_id)
                    .await?
                    .ok_or(BroadcastError::UserNotFound)
            },
            self.repo.get_cohosts(broadcast_id),
        )?;

        let live_count = self.get_live_count(broadcast_id).await;
        let ctx = self
            .build_context(&broadcast, viewer_id, live_count, 0)
            .await?;

        self.broadcast_to_response(broadcast, creator, cohosts, ctx)
            .await
    }

    /// List broadcasts with pagination. Total DB count + Redis live counts
    /// are fetched in parallel.
    pub async fn list(
        &self,
        params: BroadcastParams,
        viewer_id: Option<Uuid>,
    ) -> Result<PaginationResponse<BroadcastResponse>, BroadcastError> {
        let (rows, total) = self.repo.list(&params, viewer_id).await?;

        let page = params.page.unwrap_or(1).max(1);
        let limit = params.limit.unwrap_or(20).clamp(1, 100);

        // Build responses. For list endpoints we skip some of the heavier
        // per-row queries (bookmarked, subscribed) and batch what we can.
        // TODO: batch Redis MGET for live counts in one round-trip.
        let mut responses = Vec::with_capacity(rows.len());
        for row in rows {
            let broadcast = row.broadcast;
            let creator = ParticipantSummary {
                id: broadcast.creator_id,
                full_name: row.creator_full_name,
                avatar_id: row.creator_avatar_id,
                avatar_url: row.creator_avatar_url,
            };
            let live_count = self.get_live_count(broadcast.id).await;
            let ctx = BroadcastContext {
                viewer_id,
                total_count: row.total_participants,
                live_count,
                ..Default::default()
            };
            responses.push(
                self.broadcast_to_response(broadcast, creator, vec![], ctx)
                    .await?,
            );
        }

        Ok(PaginationResponse {
            data: responses,
            total,
            page,
            total_pages: (total as f64 / limit as f64).ceil() as i64,
        })
    }

    // ── Host disconnect / grace period ───────────────────────────────────────

    /// Called from the WS disconnect handler when the host's socket drops.
    /// Starts the tiered grace period and spawns the watcher task.
    pub async fn on_host_disconnected(
        &self,
        broadcast_id: Uuid,
        host_id: Uuid,
    ) -> Result<(), BroadcastError> {
        // Tiered grace: first disconnect is most generous.
        let count_key = format!("host_disconnect_count:{}", broadcast_id);
        let disconnect_count: i64 = self.redis.incr(&count_key).await.unwrap_or(1);
        let _ = self.redis.expire(&count_key, 3600).await;

        let grace_secs: u64 = match disconnect_count {
            1 => 120,
            2 => 90,
            3 => 60,
            _ => 30,
        };

        let grace_key = RedisService::host_grace_key(broadcast_id);
        let grace_started_key = format!("host_grace_started:{}", broadcast_id);

        let _ = self.redis.set_ex(&grace_key, "pending", grace_secs).await;
        let _ = self
            .redis
            .set_ex(
                &grace_started_key,
                OffsetDateTime::now_utc().unix_timestamp().to_string().as_str(),
                grace_secs + 10,
            )
            .await;

        // Notify all participants so the FE can show a countdown.
        if let Ok(participant_ids) = self.repo.get_all_participant_ids(broadcast_id).await {
            self.ws
                .send_to_users(
                    &participant_ids,
                    WsPayload {
                        event: "hostDisconnected".into(),
                        data: json!({
                            "broadcastId": broadcast_id,
                            "gracePeriodSecs": grace_secs,
                            "disconnectCount": disconnect_count,
                        }),
                    },
                )
                .await;
        }

        // Spawn the grace-period watcher.
        let svc = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(grace_secs)).await;
            // If the grace key still exists, the host never reconnected.
            if svc.redis.exists(&grace_key).await.unwrap_or(false) {
                let _ = svc.redis.del(&grace_key).await;
                tracing::info!(
                    broadcast_id = %broadcast_id,
                    "Host grace period expired — ending broadcast"
                );
                let _ = svc
                    .end_broadcast(broadcast_id, host_id, EndReason::HostDisconnected)
                    .await;
            }
        });

        Ok(())
    }

    /// Called when a host's WS reconnects within the grace period.
    pub async fn on_host_reconnected(&self, user_id: Uuid) -> Result<(), BroadcastError> {
        if let Some(broadcast) = self.repo.find_active_hosted_by(user_id).await? {
            let grace_key = RedisService::host_grace_key(broadcast.id);
            let was_in_grace = self.redis.del(&grace_key).await.unwrap_or(0) > 0;

            if was_in_grace {
                if let Ok(participant_ids) =
                    self.repo.get_all_participant_ids(broadcast.id).await
                {
                    self.ws
                        .send_to_users(
                            &participant_ids,
                            WsPayload {
                                event: "hostReconnected".into(),
                                data: json!({ "broadcastId": broadcast.id }),
                            },
                        )
                        .await;
                }
            }
        }
        Ok(())
    }
}
```

## WebSocket Handler
```rust
// shared/websocket/handler.rs

use crate::modules::auth::model::User;
use crate::modules::broadcast::dto::{BroadcastEndedPayload, EndBroadcastResponse};
use crate::modules::broadcast::service::BroadcastService;
use crate::shared::services::redis::RedisService;
use crate::shared::services::ws::service::{WsEvent, WsPayload, WsService};
use crate::state::MenoState;
use axum::{
    Json,
    extract::ws::{Message, WebSocket},
    extract::{Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
};
use futures_util::{SinkExt, stream::StreamExt};
use serde_json::{json, Value};
use std::sync::{Arc, atomic};
use std::time::Duration;
use tokio::{sync::mpsc, time};
use uuid::Uuid;

// ==================== TYPES ====================

#[derive(Debug, serde::Deserialize)]
pub struct WsQuery {
    pub token: String,
}

#[derive(Debug, serde::Deserialize)]
struct ClientMessage {
    event: String,
    data: Value,
}

#[derive(Debug, serde::Deserialize)]
struct EndBroadcastData {
    broadcast_id: Uuid,
    #[serde(default)]
    request_id: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct LeaveBroadcastData {
    broadcast_id: Uuid,
    position_seconds: Option<i32>,
    #[serde(default)]
    request_id: Option<String>,
}

// ==================== HEARTBEAT CONFIG ====================

pub struct HeartbeatConfig {
    pub ping_interval_secs: u64,
    pub host_pong_timeout: u64,
    pub listener_pong_timeout: u64,
    pub max_missed_pings: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            ping_interval_secs: 25,
            host_pong_timeout: 60,
            listener_pong_timeout: 20,
            max_missed_pings: 2,
        }
    }
}

// ==================== WS UPGRADE HANDLER ====================

// GET /ws?token=<access_jwt>
pub async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<Arc<MenoState>>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // Decode and validate token
    let claims = state
        .jwt
        .decode_access(&query.token)
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "Invalid token"))?;

    // Fetch user from database
    let user = state
        .auth
        .find_user_by_id(claims.sub)
        .await
        .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?
        .ok_or(error_response(StatusCode::BAD_REQUEST, "User not found"))?;

    // Upgrade to WebSocket
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, user, state)))
}

// ==================== SOCKET HANDLER ====================

async fn handle_socket(socket: WebSocket, user: User, state: Arc<MenoState>) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let (hub_tx, hub_rx) = mpsc::channel::<Arc<WsPayload>>(128);
    
    // Register connection
    let conn_id = state.ws.register(user.id, hub_tx);
    
    // Check if user is an active host (for heartbeat tuning)
    let is_host = check_is_active_host(&state, user.id).await;
    let heartbeat_config = HeartbeatConfig::default();
    let pong_timeout = if is_host {
        heartbeat_config.host_pong_timeout
    } else {
        heartbeat_config.listener_pong_timeout
    };
    
    // Spawn write task (hub → WebSocket)
    let write_task = tokio::spawn({
        let mut ws_sender = ws_sender;
        async move {
            let mut hub_rx = hub_rx;
            while let Some(payload) = hub_rx.recv().await {
                let json = serde_json::to_string(&payload).unwrap_or_default();
                if ws_sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });
    
    // Heartbeat tracking
    let missed_pongs = Arc::new(atomic::AtomicU32::new(0));
    let missed_clone = missed_pongs.clone();
    let ping_interval = heartbeat_config.ping_interval_secs;
    let max_missed = heartbeat_config.max_missed_pings;
    
    // Heartbeat task (sends ping every interval)
    let heartbeat_task = tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(ping_interval));
        let (mut ws_sender, _) = socket.split(); // Need to handle this differently
        // Note: This is simplified - actual implementation needs access to ws_sender
        loop {
            interval.tick().await;
            if missed_clone.fetch_add(1, atomic::Ordering::Relaxed) >= max_missed {
                tracing::warn!("User {} missed {} pings — disconnecting", user.id, max_missed);
                break;
            }
            // Send ping (would need ws_sender access)
        }
    });
    
    // Main read loop
    loop {
        let timeout_duration = Duration::from_secs(pong_timeout);
        let timeout = time::timeout(timeout_duration, ws_receiver.next()).await;
        
        match timeout {
            Ok(Some(Ok(Message::Text(text)))) => {
                // Reset missed counter on any message
                missed_pongs.store(0, atomic::Ordering::Relaxed);
                handle_client_message(&text, user.id, state.clone()).await;
            }
            Ok(Some(Ok(Message::Ping(_)))) => {
                missed_pongs.store(0, atomic::Ordering::Relaxed);
                // Axum automatically responds with Pong
            }
            Ok(Some(Ok(Message::Close(_))) | None) => {
                tracing::info!("WebSocket closed for user {}", user.id);
                break;
            }
            Ok(Some(Err(e))) => {
                tracing::warn!("WebSocket error for user {}: {}", user.id, e);
                break;
            }
            Err(_timeout) => {
                tracing::warn!("WebSocket timeout for user {} after {}s", user.id, pong_timeout);
                break;
            }
            _ => {}
        }
    }
    
    // Cleanup
    heartbeat_task.abort();
    write_task.abort();
    state.ws.unregister(user.id, conn_id);
    
    // Handle post-disconnect cleanup
    handle_disconnect(&state, user.id).await;
}

// ==================== CLIENT MESSAGE HANDLER ====================

async fn handle_client_message(raw_text: &str, user_id: Uuid, state: Arc<MenoState>) {
    let msg: ClientMessage = match serde_json::from_str(raw_text) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("Invalid WS message from user {}: {}", user_id, e);
            send_error_response(&state, user_id, None, "Invalid message format").await;
            return;
        }
    };
    
    match msg.event.as_str() {
        "endBroadcast" => {
            handle_end_broadcast(&state, user_id, msg.data).await;
        }
        "leaveBroadcast" => {
            handle_leave_broadcast(&state, user_id, msg.data).await;
        }
        _ => {
            tracing::warn!("Unknown WS event from user {}: {}", user_id, msg.event);
            send_error_response(&state, user_id, None, format!("Unknown event: {}", msg.event)).await;
        }
    }
}

// ==================== END BROADCAST HANDLER ====================

async fn handle_end_broadcast(state: &Arc<MenoState>, user_id: Uuid, data: Value) {
    let end_data: EndBroadcastData = match serde_json::from_value(data) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("Invalid endBroadcast data: {}", e);
            send_error_response(state, user_id, None, "Invalid broadcast ID").await;
            return;
        }
    };
    
    // Call the broadcast service to end the broadcast
    let result = state.broadcast.end_broadcast(end_data.broadcast_id, user_id).await;
    
    match result {
        Ok(response) => {
            // Send success response back to the caller
            let response_payload = WsPayload::new(
                WsEvent::EndBroadcastResponse,
                json!({
                    "success": true,
                    "data": response,
                    "requestId": end_data.request_id,
                }),
            );
            state.ws.send_to_user(user_id, response_payload).await;
            
            // Notify all other participants that broadcast ended
            let notification_payload = WsPayload::new(
                WsEvent::EndedBroadcast,
                BroadcastEndedPayload::normal_for(end_data.broadcast_id),
            );
            
            // Get all participants except the host
            if let Ok(participant_ids) = state.broadcast_repo.get_participant_ids(end_data.broadcast_id).await {
                let other_participants: Vec<Uuid> = participant_ids
                    .into_iter()
                    .filter(|&pid| pid != user_id)
                    .collect();
                
                if !other_participants.is_empty() {
                    state.ws.send_to_users(&other_participants, notification_payload).await;
                }
            }
        }
        Err(e) => {
            // Send error response
            send_error_response(state, user_id, end_data.request_id, e.to_string()).await;
        }
    }
}

// ==================== LEAVE BROADCAST HANDLER ====================

async fn handle_leave_broadcast(state: &Arc<MenoState>, user_id: Uuid, data: Value) {
    let leave_data: LeaveBroadcastData = match serde_json::from_value(data) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("Invalid leaveBroadcast data: {}", e);
            send_error_response(state, user_id, None, "Invalid broadcast ID").await;
            return;
        }
    };
    
    // Call the broadcast service to leave
    let result = state.broadcast
        .leave_broadcast(leave_data.broadcast_id, user_id, leave_data.position_seconds, state)
        .await;
    
    match result {
        Ok(()) => {
            // Send success response
            let response_payload = WsPayload::new(
                WsEvent::LeaveBroadcastResponse,
                json!({
                    "success": true,
                    "broadcastId": leave_data.broadcast_id,
                    "requestId": leave_data.request_id,
                }),
            );
            state.ws.send_to_user(user_id, response_payload).await;
            
            // Notify host/cohosts that listener left
            if let Ok(host_ids) = state.broadcast_repo.get_host_and_cohost_ids(leave_data.broadcast_id).await {
                let notification_payload = WsPayload::new(
                    WsEvent::BroadcastListenerLeft,
                    json!({
                        "userId": user_id,
                        "broadcastId": leave_data.broadcast_id,
                    }),
                );
                state.ws.send_to_users(&host_ids, notification_payload).await;
            }
            
            // Update live count
            if let Ok(new_count) = state.redis.decr(&RedisService::live_count_key(leave_data.broadcast_id)).await {
                if let Ok(participant_ids) = state.broadcast_repo.get_participant_ids(leave_data.broadcast_id).await {
                    let count_payload = WsPayload::new(
                        WsEvent::NumberOfLiveListeners,
                        json!({
                            "broadcastId": leave_data.broadcast_id,
                            "count": new_count.max(0),
                        }),
                    );
                    state.ws.send_to_users(&participant_ids, count_payload).await;
                }
            }
        }
        Err(e) => {
            send_error_response(state, user_id, leave_data.request_id, e.to_string()).await;
        }
    }
}

// ==================== HELPER FUNCTIONS ====================

async fn check_is_active_host(state: &MenoState, user_id: Uuid) -> bool {
    if let Ok(Some(broadcast)) = state.broadcast_repo.find_active_hosted_by(user_id).await {
        broadcast.status == crate::modules::broadcast::model::BroadcastStatus::Active
    } else {
        false
    }
}

async fn handle_disconnect(state: &MenoState, user_id: Uuid) {
    // Remove presence from Redis
    let key = RedisService::presence_key(user_id);
    let _ = state.redis.del(&key).await;
    
    // Check if user was a host of an active broadcast
    if let Ok(Some(broadcast)) = state.broadcast_repo.find_active_hosted_by(user_id).await {
        // Notify participants that host disconnected
        if let Ok(participant_ids) = state.broadcast_repo.get_participant_ids(broadcast.id).await {
            let payload = WsPayload::new(
                WsEvent::HostDisconnected,
                json!({
                    "broadcastId": broadcast.id,
                    "userId": user_id,
                }),
            );
            state.ws.send_to_users(&participant_ids, payload).await;
        }
        
        // Start grace period
        // This would call into broadcast_service.on_host_disconnected()
    }
    
    // Check if user was a listener
    if let Ok(Some(participant)) = state.broadcast_repo.find_active_participant(user_id).await {
        // Remove from broadcast_participants
        let _ = state.broadcast_repo.remove_listener(participant.broadcast_id, user_id).await;
        
        // Update live count
        let _ = state.redis.decr(&RedisService::live_count_key(participant.broadcast_id)).await;
        
        // Notify host
        if let Ok(host_ids) = state.broadcast_repo.get_host_and_cohost_ids(participant.broadcast_id).await {
            let payload = WsPayload::new(
                WsEvent::BroadcastListenerLeft,
                json!({
                    "userId": user_id,
                    "broadcastId": participant.broadcast_id,
                }),
            );
            state.ws.send_to_users(&host_ids, payload).await;
        }
    }
}

async fn send_error_response(
    state: &MenoState,
    user_id: Uuid,
    request_id: Option<String>,
    message: String,
) {
    let error_payload = WsPayload::new(
        WsEvent::BroadcastError,
        json!({
            "message": message,
            "requestId": request_id,
        }),
    );
    state.ws.send_to_user(user_id, error_payload).await;
}

fn error_response(code: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (code, Json(json!({ "error": message })))
}
```