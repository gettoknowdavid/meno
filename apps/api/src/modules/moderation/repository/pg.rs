//! Postgres-backed [`ModerationRepo`].
//!
//! Every statement is a literal: nothing here is spliced from caller text, so no
//! `AssertSqlSafe` is needed (compare [`crate::modules::chat::repository::pg`], whose
//! templates do splice column lists).

use async_trait::async_trait;
use sqlx::{AssertSqlSafe, FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{ModerationRepo, ReportFilter};
use crate::modules::moderation::model::{
    BlockedUser, NewReport, Report, ReportStatus, ReportSubject,
};
use meno_core::Error as MenoError;

/// A driver failure, named and logged, never echoed to the client (§4.2).
fn internal(context: &'static str, detail: impl std::fmt::Display) -> MenoError {
    MenoError::Internal {
        context,
        detail: detail.to_string(),
    }
}

const REPORT_COLUMNS: &str =
    "id, reporter_id, subject_type, subject_id, reason, details, status, created_at, resolved_at";

/// A report row as storage holds it.
#[derive(Debug, FromRow)]
struct ReportRow {
    id: Uuid,
    reporter_id: Uuid,
    subject_type: String,
    subject_id: Uuid,
    reason: String,
    details: Option<String>,
    status: String,
    created_at: OffsetDateTime,
    resolved_at: Option<OffsetDateTime>,
}

impl ReportRow {
    /// The typed row, or an error naming the value this build does not understand.
    ///
    /// A `CHECK` in the migration constrains both columns, so an unknown value here means
    /// the constraint and the enum have drifted — an internal fault, not a client's.
    fn into_report(self) -> Result<Report, MenoError> {
        let subject_type = ReportSubject::parse(&self.subject_type).ok_or_else(|| {
            internal(
                "read_report",
                format!("unknown subject {}", self.subject_type),
            )
        })?;
        let status = ReportStatus::parse(&self.status)
            .ok_or_else(|| internal("read_report", format!("unknown status {}", self.status)))?;

        Ok(Report {
            id: self.id,
            reporter_id: self.reporter_id,
            subject_type,
            subject_id: self.subject_id,
            reason: self.reason,
            details: self.details,
            status,
            created_at: self.created_at,
            resolved_at: self.resolved_at,
        })
    }
}

/// Postgres-backed [`ModerationRepo`].
#[derive(Debug, Clone)]
pub struct PgModerationRepo {
    pool: PgPool,
}

impl PgModerationRepo {
    /// Build from a pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ModerationRepo for PgModerationRepo {
    async fn block(&self, blocker_id: Uuid, blocked_id: Uuid) -> Result<bool, MenoError> {
        // The `WHERE EXISTS` is what makes an unknown id a `false` rather than a foreign
        // key violation the service would have to translate; the service then answers 404
        // only when nothing was inserted *and* the target does not exist.
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO public.user_blocks (blocker_id, blocked_id) \
             SELECT $1, $2 \
             WHERE EXISTS (SELECT 1 FROM public.users u WHERE u.id = $2 AND u.deleted_at IS NULL) \
             ON CONFLICT DO NOTHING \
             RETURNING blocked_id",
        )
        .bind(blocker_id)
        .bind(blocked_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("block_user", e))?;

        Ok(inserted.is_some())
    }

    async fn unblock(&self, blocker_id: Uuid, blocked_id: Uuid) -> Result<bool, MenoError> {
        let removed: Option<Uuid> = sqlx::query_scalar(
            "DELETE FROM public.user_blocks \
             WHERE blocker_id = $1 AND blocked_id = $2 \
             RETURNING blocked_id",
        )
        .bind(blocker_id)
        .bind(blocked_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("unblock_user", e))?;

        Ok(removed.is_some())
    }

    async fn blocked(&self, blocker_id: Uuid) -> Result<Vec<BlockedUser>, MenoError> {
        let rows = sqlx::query_as::<_, BlockedUser>(
            "SELECT b.blocked_id AS id, u.full_name, b.created_at AS blocked_at \
             FROM public.user_blocks b \
             JOIN public.users u ON u.id = b.blocked_id \
             WHERE b.blocker_id = $1 AND u.deleted_at IS NULL \
             ORDER BY b.created_at DESC, b.blocked_id DESC",
        )
        .bind(blocker_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| internal("list_blocked", e))?;

        Ok(rows)
    }

    async fn user_exists(&self, user_id: Uuid) -> Result<bool, MenoError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM public.users WHERE id = $1 AND deleted_at IS NULL)",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| internal("moderation_user_exists", e))?;

        Ok(exists)
    }

    async fn broadcast_exists(&self, broadcast_id: Uuid) -> Result<bool, MenoError> {
        // Not `deleted_at IS NULL`: a report about a broadcast that has since been removed
        // is still a report somebody filed.
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM public.broadcasts WHERE id = $1)")
                .bind(broadcast_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| internal("moderation_broadcast_exists", e))?;

        Ok(exists)
    }

    async fn create_report(&self, new: NewReport<'_>) -> Result<Report, MenoError> {
        // `AssertSqlSafe` is the claim that the only interpolated text is
        // [`REPORT_COLUMNS`], a `const` in this file; every caller value is a bind.
        let row = sqlx::query_as::<_, ReportRow>(AssertSqlSafe(format!(
            "INSERT INTO public.moderation_reports \
             (reporter_id, subject_type, subject_id, reason, details) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING {REPORT_COLUMNS}"
        )))
        .bind(new.reporter_id)
        .bind(new.subject_type.as_str())
        .bind(new.subject_id)
        .bind(new.reason)
        .bind(new.details)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| internal("create_report", e))?;

        row.into_report()
    }

    async fn reports(&self, filter: ReportFilter) -> Result<Vec<Report>, MenoError> {
        // Both cursor halves are bound separately and the null branch keeps the first
        // page free of a special statement: `$2 IS NULL` is the whole of "no cursor".
        let where_cursor = "(created_at < $2 OR (created_at = $2 AND id < $3))";
        let sql = format!(
            "SELECT {REPORT_COLUMNS} FROM public.moderation_reports \
             WHERE ($1::text IS NULL OR status = $1) \
               AND ($2::timestamptz IS NULL OR {where_cursor}) \
             ORDER BY created_at DESC, id DESC \
             LIMIT $4"
        );

        let (cursor_at, cursor_id) = match filter.cursor {
            Some((at, id)) => (Some(at), Some(id)),
            None => (None, None),
        };

        let rows = sqlx::query_as::<_, ReportRow>(AssertSqlSafe(sql))
            .bind(filter.status.map(ReportStatus::as_str))
            .bind(cursor_at)
            .bind(cursor_id)
            .bind(filter.limit_plus_one)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| internal("list_reports", e))?;

        rows.into_iter().map(ReportRow::into_report).collect()
    }

    async fn report(&self, id: Uuid) -> Result<Option<Report>, MenoError> {
        let row = sqlx::query_as::<_, ReportRow>(AssertSqlSafe(format!(
            "SELECT {REPORT_COLUMNS} FROM public.moderation_reports WHERE id = $1"
        )))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("read_report", e))?;

        row.map(ReportRow::into_report).transpose()
    }

    async fn resolve_report(&self, id: Uuid, status: ReportStatus) -> Result<bool, MenoError> {
        // `resolved_at` moves only for a real decision, which the migration's CHECK also
        // enforces; the service refuses `open` before this is reached.
        let resolved: Option<Uuid> = sqlx::query_scalar(
            "UPDATE public.moderation_reports \
             SET status = $2, resolved_at = now() \
             WHERE id = $1 \
             RETURNING id",
        )
        .bind(id)
        .bind(status.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("resolve_report", e))?;

        Ok(resolved.is_some())
    }
}
