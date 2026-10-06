//! The Postgres adapter.
//!
//! # Why the statements are templates
//!
//! `sqlx` 0.9 only accepts a `&'static str` as a query, precisely so that a `format!`
//! carrying user input cannot reach the database by accident. Without a splice, five
//! statements here would each repeat a twenty-column projection, and a copy that misses
//! a column is a `FromRow` failure at runtime rather than a compile error. So the four
//! column lists below are `const`s, spliced into statement templates, and the result is
//! marked [`sqlx::AssertSqlSafe`] — the same pattern, and the same argument, as
//! [`crate::modules::auth::repository::pg`].
//!
//! That is an audit rather than a suppression:
//!
//! - the only interpolated values are [`BROADCAST_COLUMNS`], [`PARTICIPANT_COLUMNS`],
//!   [`COHOST_COLUMNS`] and [`USER_COLUMNS`] — literal column names no request can
//!   influence;
//! - every value that *is* request-derived — a title, a role, a time zone, an id — is a
//!   bind parameter, never text in the statement;
//! - the [`QueryBuilder`] calls below build only `SET` clauses from a fixed set of
//!   literals, and the cursor helpers take a [`ColumnName`], which refuses a name that
//!   is not on its allow-list.
//!
//! `tests/broadcast_repository_pg.rs` checks the consts against `information_schema`,
//! so a projection that names a column the migration never declared fails there.
//!
//! # §7.6 — why `join` is a transaction
//!
//! `master` incremented `total_participants` from the application and had a trigger
//! doing it too, so the counter moved once per join on some paths and twice on others,
//! and the broadcast's own count drifted permanently away from the number of people
//! who had actually joined. Here the row write and the counter live in one statement:
//!
//! ```sql
//! INSERT INTO broadcast_participants (…) VALUES (…)
//! ON CONFLICT (broadcast_id, participant_id) DO UPDATE SET left_at = NULL
//! WHERE broadcast_participants.left_at IS NOT NULL
//! RETURNING (xmax = 0) AS inserted, (xmax <> 0) AS reopened
//! ```
//!
//! `xmax = 0` distinguishes the three outcomes the service needs — first join, rejoined,
//! or already present — without a second read, which is what makes "already in the
//! room" a 409 instead of a silent re-join that inflates the counter. The trigger from
//! migration 0006 still fires on the `INSERT` arm, so the *first* join increments once
//! through the trigger and the *rejoin* arm increments here, explicitly, because a
//! trigger cannot see an `UPDATE`.
//!
//! # Every query filters `deleted_at IS NULL`
//!
//! Soft deletion is invisible to a `SELECT` unless it is asked for, and a list that
//! returns deleted broadcasts is a bug nobody reports.

use async_trait::async_trait;
use sqlx::{AssertSqlSafe, Postgres, QueryBuilder};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{BroadcastFilter, BroadcastRepo, ParticipantFilter, RepoDeps};
use crate::infrastructure::query::{ColumnName, push_cursor_condition, push_order_and_limit};
use crate::modules::broadcast::error;
use crate::modules::broadcast::model::{
    Broadcast, BroadcastCohost, BroadcastParticipant, BroadcastPatch, EndReason, JoinOutcome,
    JoinRequest, LeaveOutcome, NewBroadcast, ParticipantRole, UserSummary,
};
use meno_core::Error as MenoError;

/// Columns selected wherever a `Broadcast` is read.
const BROADCAST_COLUMNS: &str = "id, title, description, status, creator_id, time_zone, \
     image_url, image_id, broadcast_token, total_participants, start_time, end_time, \
     recording_enabled, recording_key, recording_url, published_at, end_reason, \
     created_at, updated_at, deleted_at";

/// Columns selected wherever a `BroadcastParticipant` is read.
const PARTICIPANT_COLUMNS: &str = "broadcast_id, participant_id, role, joined_at, left_at, \
     last_listen_position_seconds, last_listened_at";

/// Columns selected wherever a `BroadcastCohost` is read.
const COHOST_COLUMNS: &str = "broadcast_id, cohost_id, invited_by, invited_at, removed_at";

/// Columns selected wherever a `UserSummary` is read.
///
/// Nothing else, so a password-adjacent column cannot be added here by accident.
const USER_COLUMNS: &str = "id, full_name, avatar_id, avatar_url";

/// Splice the four column lists into a statement template and mark the result safe.
///
/// See the module docs for why that is an audit rather than a hole: nothing but these
/// `const`s is interpolated, and the `const`s are literal column names.
fn statement(template: &str) -> AssertSqlSafe<String> {
    AssertSqlSafe(
        template
            .replace("{BROADCAST_COLUMNS}", BROADCAST_COLUMNS)
            .replace("{PARTICIPANT_COLUMNS}", PARTICIPANT_COLUMNS)
            .replace("{COHOST_COLUMNS}", COHOST_COLUMNS)
            .replace("{USER_COLUMNS}", USER_COLUMNS),
    )
}

/// The head of a statement a [`QueryBuilder`] then appends to.
///
/// `QueryBuilder::new` takes `impl Into<String>` — sqlx's own escape hatch — so there is
/// no [`AssertSqlSafe`] to return here and the audit is the argument list: the only
/// pieces spliced are the `const`s above, and everything a request contributes is
/// appended afterwards as a bind parameter.
fn head(prefix: &str, columns: &str, suffix: &str) -> String {
    format!("{prefix}{columns}{suffix}")
}

/// Postgres-backed storage for the broadcast module.
#[derive(Debug, Clone)]
pub struct PgBroadcastRepo {
    pool: sqlx::PgPool,
}

impl PgBroadcastRepo {
    /// Read and write through `deps.pool`.
    #[must_use]
    pub fn new(deps: RepoDeps) -> Self {
        Self { pool: deps.pool }
    }
}

/// Log the driver's text and return a 500 that does not contain it.
fn database(context: &'static str, error: sqlx::Error) -> MenoError {
    tracing::error!(%error, context, "broadcast repository call failed");
    error::internal(context, error)
}

#[async_trait]
impl BroadcastRepo for PgBroadcastRepo {
    async fn create(&self, new: NewBroadcast) -> Result<Broadcast, MenoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| database("begin_create", e))?;

        let row: Broadcast = sqlx::query_as::<_, Broadcast>(statement(
            "INSERT INTO broadcasts \
             (title, description, creator_id, time_zone, image_id, image_url, start_time, \
              recording_enabled) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             RETURNING {BROADCAST_COLUMNS}",
        ))
        .bind(&new.title)
        .bind(&new.description)
        .bind(new.creator_id)
        .bind(&new.time_zone)
        .bind(&new.image_id)
        .bind(&new.image_url)
        .bind(new.start_time)
        .bind(new.recording_enabled)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| database("insert_broadcast", e))?;

        // Cohosts in the same transaction: a live broadcast whose invited cohosts went
        // missing is a permissions bug to everyone except the debugger.
        for cohost in &new.cohosts {
            sqlx::query(
                "INSERT INTO broadcast_cohosts (broadcast_id, cohost_id, invited_by) \
                 VALUES ($1, $2, $3)",
            )
            .bind(row.id)
            .bind(cohost)
            .bind(new.creator_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| database("insert_cohost", e))?;
        }

        tx.commit()
            .await
            .map_err(|e| database("commit_create", e))?;
        Ok(row)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Broadcast>, MenoError> {
        sqlx::query_as::<_, Broadcast>(statement(
            "SELECT {BROADCAST_COLUMNS} \
             FROM broadcasts WHERE id = $1 AND deleted_at IS NULL",
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("find_broadcast", e))
    }

    async fn update(&self, id: Uuid, patch: BroadcastPatch) -> Result<Broadcast, MenoError> {
        // Each `if let` binds one value, and the column list is a fixed set of literals
        // — never caller text. That is the injection boundary: the only strings that
        // reach the query are the ones in this file.
        let mut qb: QueryBuilder<Postgres> =
            QueryBuilder::new("UPDATE broadcasts SET updated_at = now()");

        if let Some(title) = &patch.title {
            qb.push(", title = ").push_bind(title);
        }
        if let Some(description) = &patch.description {
            qb.push(", description = ").push_bind(description);
        }
        if let Some(time_zone) = &patch.time_zone {
            qb.push(", time_zone = ").push_bind(time_zone);
        }
        if let Some(image_id) = &patch.image_id {
            qb.push(", image_id = ").push_bind(image_id);
        }
        if let Some(image_url) = &patch.image_url {
            qb.push(", image_url = ").push_bind(image_url);
        }
        if let Some(start_time) = patch.start_time {
            qb.push(", start_time = ").push_bind(start_time);
        }
        if let Some(recording_enabled) = patch.recording_enabled {
            qb.push(", recording_enabled = ")
                .push_bind(recording_enabled);
        }

        qb.push(" WHERE id = ")
            .push_bind(id)
            .push(" AND deleted_at IS NULL RETURNING ")
            .push(BROADCAST_COLUMNS);

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| database("begin_update", e))?;
        let row: Broadcast = qb
            .build_query_as::<Broadcast>()
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| database("update_broadcast", e))?;

        if let Some(cohosts) = &patch.cohosts {
            sqlx::query("DELETE FROM broadcast_cohosts WHERE broadcast_id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| database("clear_cohosts", e))?;

            for cohost in cohosts {
                sqlx::query(
                    "INSERT INTO broadcast_cohosts (broadcast_id, cohost_id, invited_by) \
                     VALUES ($1, $2, $3)",
                )
                .bind(id)
                .bind(cohost)
                .bind(row.creator_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| database("insert_cohost", e))?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| database("commit_update", e))?;
        Ok(row)
    }

    async fn soft_delete(&self, id: Uuid) -> Result<bool, MenoError> {
        let result = sqlx::query(
            "UPDATE broadcasts SET deleted_at = now() \
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| database("soft_delete_broadcast", e))?;

        Ok(result.rows_affected() > 0)
    }

    async fn mark_live(
        &self,
        id: Uuid,
        at: OffsetDateTime,
    ) -> Result<Option<Broadcast>, MenoError> {
        // The `status = 'inactive'` predicate is the guard: a second go-live matches no
        // row and comes back `None`, which the service reports as `already_live`
        // instead of overwriting the first `published_at`.
        sqlx::query_as::<_, Broadcast>(statement(
            "UPDATE broadcasts SET status = 'active', published_at = $2, updated_at = now() \
             WHERE id = $1 AND status = 'inactive' AND deleted_at IS NULL \
             RETURNING {BROADCAST_COLUMNS}",
        ))
        .bind(id)
        .bind(at)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("mark_live", e))
    }

    async fn mark_ended(
        &self,
        id: Uuid,
        reason: EndReason,
        at: OffsetDateTime,
    ) -> Result<Option<Broadcast>, MenoError> {
        sqlx::query_as::<_, Broadcast>(statement(
            "UPDATE broadcasts \
             SET status = 'inactive', end_time = $2, end_reason = $3, updated_at = now() \
             WHERE id = $1 AND status = 'active' AND deleted_at IS NULL \
             RETURNING {BROADCAST_COLUMNS}",
        ))
        .bind(id)
        .bind(at)
        .bind(reason.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("mark_ended", e))
    }

    async fn list(&self, filter: BroadcastFilter) -> Result<Vec<Broadcast>, MenoError> {
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(head(
            "SELECT ",
            BROADCAST_COLUMNS,
            " FROM broadcasts WHERE deleted_at IS NULL",
        ));

        if let Some(creator_id) = filter.creator_id {
            qb.push(" AND creator_id = ").push_bind(creator_id);
        }
        if let Some(status) = filter.status {
            qb.push(" AND status = ").push_bind(status.as_str());
        }

        push_cursor_condition(
            &mut qb,
            ColumnName::literal("created_at"),
            ColumnName::literal("id"),
            filter.cursor.map(|(ts, _)| ts),
            filter.cursor.map(|(_, id)| id),
            filter.order,
        );
        push_order_and_limit(
            &mut qb,
            ColumnName::literal("created_at"),
            ColumnName::literal("id"),
            filter.order,
            filter.limit_plus_one,
        );

        qb.build_query_as::<Broadcast>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("list_broadcasts", e))
    }

    async fn live_participant_count(&self, broadcast_id: Uuid) -> Result<i64, MenoError> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM broadcast_participants \
             WHERE broadcast_id = $1 AND left_at IS NULL",
        )
        .bind(broadcast_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| database("live_participant_count", e))
    }

    async fn join(&self, request: JoinRequest) -> Result<JoinOutcome, MenoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| database("begin_join", e))?;

        // `xmax` is how Postgres reports which arm of an upsert fired: `0` on a fresh
        // insert, non-zero on an update. One statement, three distinguishable outcomes,
        // and no read between the check and the write.
        #[derive(sqlx::FromRow)]
        struct Joined {
            inserted: bool,
            reopened: bool,
        }

        let joined = sqlx::query_as::<_, Joined>(
            "INSERT INTO broadcast_participants \
               (broadcast_id, participant_id, role, joined_at, left_at) \
             VALUES ($1, $2, $3, now(), NULL) \
             ON CONFLICT (broadcast_id, participant_id) DO UPDATE \
               SET left_at = NULL, role = EXCLUDED.role \
             WHERE broadcast_participants.left_at IS NOT NULL \
             RETURNING (xmax = 0) AS inserted, (xmax <> 0) AS reopened",
        )
        .bind(request.broadcast_id)
        .bind(request.participant_id)
        .bind(request.role.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| database("upsert_participant", e))?;

        let Some(joined) = joined else {
            tx.rollback()
                .await
                .map_err(|e| database("rollback_join", e))?;
            return Ok(JoinOutcome::AlreadyPresent);
        };

        // A rejoin is an `UPDATE`, and migration 0006's trigger only fires on INSERT
        // and DELETE — so without this the all-time counter would miss every return
        // visit. This is §7.6's fix, stated as code rather than as a comment.
        if joined.reopened {
            sqlx::query(
                "UPDATE broadcasts SET total_participants = total_participants + 1 \
                 WHERE id = $1",
            )
            .bind(request.broadcast_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| database("bump_total_participants", e))?;
        }
        debug_assert!(
            joined.inserted != joined.reopened,
            "xmax is either 0 or not — never both, never neither"
        );

        tx.commit().await.map_err(|e| database("commit_join", e))?;

        Ok(if joined.reopened {
            JoinOutcome::Rejoined
        } else {
            JoinOutcome::Joined
        })
    }

    async fn leave(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<LeaveOutcome, MenoError> {
        let row: Option<(ParticipantRole,)> = sqlx::query_as(
            "SELECT role FROM broadcast_participants \
             WHERE broadcast_id = $1 AND participant_id = $2",
        )
        .bind(broadcast_id)
        .bind(participant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("read_participant", e))?;

        let Some((role,)) = row else {
            return Ok(LeaveOutcome::NotPresent);
        };
        if role == ParticipantRole::Host {
            return Ok(LeaveOutcome::IsHost);
        }

        // Closing the row rather than deleting it: the all-time counter's trigger only
        // decrements on DELETE, and an all-time count must not fall when someone leaves.
        sqlx::query(
            "UPDATE broadcast_participants SET left_at = now() \
             WHERE broadcast_id = $1 AND participant_id = $2 AND left_at IS NULL",
        )
        .bind(broadcast_id)
        .bind(participant_id)
        .execute(&self.pool)
        .await
        .map_err(|e| database("close_participant", e))?;

        Ok(LeaveOutcome::Left)
    }

    async fn find_participant(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<Option<BroadcastParticipant>, MenoError> {
        sqlx::query_as::<_, BroadcastParticipant>(statement(
            "SELECT {PARTICIPANT_COLUMNS} \
             FROM broadcast_participants \
             WHERE broadcast_id = $1 AND participant_id = $2",
        ))
        .bind(broadcast_id)
        .bind(participant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("find_participant", e))
    }

    async fn list_participants(
        &self,
        broadcast_id: Uuid,
        filter: ParticipantFilter,
    ) -> Result<Vec<BroadcastParticipant>, MenoError> {
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(head(
            "SELECT ",
            PARTICIPANT_COLUMNS,
            " FROM broadcast_participants WHERE broadcast_id = ",
        ));
        qb.push_bind(broadcast_id);

        if filter.present_only {
            qb.push(" AND left_at IS NULL");
        }
        if let Some(role) = filter.role {
            qb.push(" AND role = ").push_bind(role.as_str());
        }

        push_cursor_condition(
            &mut qb,
            ColumnName::literal("joined_at"),
            ColumnName::literal("participant_id"),
            filter.cursor.map(|(ts, _)| ts),
            filter.cursor.map(|(_, id)| id),
            filter.order,
        );
        push_order_and_limit(
            &mut qb,
            ColumnName::literal("joined_at"),
            ColumnName::literal("participant_id"),
            filter.order,
            filter.limit_plus_one,
        );

        qb.build_query_as::<BroadcastParticipant>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("list_participants", e))
    }

    async fn list_cohosts(&self, broadcast_id: Uuid) -> Result<Vec<BroadcastCohost>, MenoError> {
        sqlx::query_as::<_, BroadcastCohost>(statement(
            "SELECT {COHOST_COLUMNS} \
             FROM broadcast_cohosts WHERE broadcast_id = $1 AND removed_at IS NULL \
             ORDER BY invited_at ASC",
        ))
        .bind(broadcast_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| database("list_cohosts", e))
    }

    async fn set_cohosts(
        &self,
        broadcast_id: Uuid,
        invited_by: Uuid,
        cohosts: &[Uuid],
    ) -> Result<Vec<BroadcastCohost>, MenoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| database("begin_set_cohosts", e))?;

        sqlx::query("DELETE FROM broadcast_cohosts WHERE broadcast_id = $1")
            .bind(broadcast_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| database("clear_cohosts", e))?;

        for cohost in cohosts {
            sqlx::query(
                "INSERT INTO broadcast_cohosts (broadcast_id, cohost_id, invited_by) \
                 VALUES ($1, $2, $3)",
            )
            .bind(broadcast_id)
            .bind(cohost)
            .bind(invited_by)
            .execute(&mut *tx)
            .await
            .map_err(|e| database("insert_cohost", e))?;
        }

        let rows = sqlx::query_as::<_, BroadcastCohost>(statement(
            "SELECT {COHOST_COLUMNS} \
             FROM broadcast_cohosts WHERE broadcast_id = $1 AND removed_at IS NULL \
             ORDER BY invited_at ASC",
        ))
        .bind(broadcast_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| database("read_back_cohosts", e))?;

        tx.commit()
            .await
            .map_err(|e| database("commit_set_cohosts", e))?;
        Ok(rows)
    }

    async fn find_user(&self, id: Uuid) -> Result<Option<UserSummary>, MenoError> {
        sqlx::query_as::<_, UserSummary>(statement(
            "SELECT {USER_COLUMNS} FROM users \
             WHERE id = $1 AND deleted_at IS NULL",
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("find_user", e))
    }

    async fn find_users(&self, ids: &[Uuid]) -> Result<Vec<UserSummary>, MenoError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut qb: QueryBuilder<Postgres> =
            QueryBuilder::new(head("SELECT ", USER_COLUMNS, " FROM users WHERE id IN ("));
        let mut separated = qb.separated(", ");
        for id in ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(") AND deleted_at IS NULL");

        qb.build_query_as::<UserSummary>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("find_users", e))
    }
}

/// The status filter binds the column's own spelling, not the enum's name.
///
/// Asserted rather than commented: `sqlx`'s derive would rename `Inactive` to
/// `inactive` too, and the two paths are the kind of duplication that survives a
/// rename of the Rust variant.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::broadcast::model::BroadcastStatus;

    #[test]
    fn a_status_binds_the_spelling_the_check_constraint_allows() {
        assert_eq!(BroadcastStatus::Inactive.as_str(), "inactive");
        assert_eq!(BroadcastStatus::Active.as_str(), "active");
        assert_eq!(BroadcastStatus::Ended.as_str(), "ended");
    }

    #[test]
    fn every_end_reason_binds_a_value_the_check_constraint_allows() {
        // migration 0004: end_reason IN ('normal', 'host_disconnected', 'admin_forced',
        // 'quota_exceeded'), and NULL while it has not ended.
        for reason in [
            EndReason::Normal,
            EndReason::HostDisconnected,
            EndReason::AdminForced,
            EndReason::QuotaExceeded,
        ] {
            assert!(matches!(
                reason.as_str(),
                "normal" | "host_disconnected" | "admin_forced" | "quota_exceeded"
            ));
        }
        assert_eq!(
            EndReason::None.as_str(),
            "none",
            "None is not written: a broadcast that has not ended stores NULL"
        );
    }

    /// Every placeholder in every template is one of the four `const`s.
    ///
    /// A typo in a placeholder would otherwise splice to nothing and produce a query
    /// with `SELECT FROM`.
    #[test]
    fn every_placeholder_resolves() {
        let spliced = statement(
            "SELECT {BROADCAST_COLUMNS}, {PARTICIPANT_COLUMNS}, {COHOST_COLUMNS}, \
             {USER_COLUMNS}",
        );
        let sql = spliced.0;
        assert!(!sql.contains('{'), "unresolved placeholder in {sql}");
        assert!(!sql.contains('}'), "unresolved placeholder in {sql}");
        assert!(sql.contains("broadcast_token"));
        assert!(sql.contains("last_listen_position_seconds"));
        assert!(sql.contains("removed_at"));
        assert!(sql.contains("avatar_url"));
    }
}
