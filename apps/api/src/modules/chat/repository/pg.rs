//! The Postgres adapter for [`ChatRepo`].
//!
//! Plain `sqlx` runtime queries, not `query!` macros — the same trade as the rest of the
//! refactored tree: `cargo check` works with no database standing (§15), and the SQL is
//! exercised by `tests/chat_service.rs` against the in-memory double that mirrors it.
//!
//! # Injection
//!
//! Every value a request contributes is a bind parameter. The only interpolated text is
//! the column list in [`MESSAGE_SELECT`], spliced through [`statement`], which returns
//! [`sqlx::AssertSqlSafe`] — the same audit-by-type the auth and broadcast repositories
//! use: the *only* pieces that reach a statement are the `const`s in this file, and
//! everything a request contributes is appended afterwards as a bind.
//!
//! Queries built a clause at a time go through [`QueryBuilder`] instead, which takes
//! `impl Into<String>` and has no `AssertSqlSafe` to return — so there the audit is the
//! argument list, and its cursor helpers take a [`ColumnName`](crate::infrastructure::query::ColumnName)
//! that refuses anything not written in this module's own source.
//!
//! # Two writes, and why they are one statement each
//!
//! `master` checked whether a broadcast was live and whether the sender was in the
//! room, then inserted. Between the check and the insert the broadcast could end or the
//! participant could be removed, and the message landed anyway — a row in a room nobody
//! is in. Here the guard is inside the `INSERT … SELECT … WHERE EXISTS`, so the write
//! either happens in a state that allowed it or does not happen at all.
//!
//! The `WITH inserted AS (…) SELECT …` around it is what keeps it to *one* round trip
//! while still returning the message with its sender: the join onto `users` reads the
//! row the same statement just wrote.
//!
//! # `updated_at` is the database's
//!
//! Migration 0010 attaches `set_updated_at` to `chat_messages`, so `UPDATE` sets the
//! column on the way in. `master` also wrote `updated_at = NOW()` in the statement,
//! which was a second answer to a question the trigger had already answered.

use sqlx::{AssertSqlSafe, PgPool, Postgres, QueryBuilder};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{ChatRepo, MessageFilter};
use crate::infrastructure::query::{ColumnName, push_cursor_condition, push_order_and_limit};
use crate::modules::chat::error;
use crate::modules::chat::model::{
    ChatMessage, ChatReaction, NewMessage, NewReaction, SenderSummary,
};
use meno_core::Error as MenoError;

/// Postgres-backed storage for the chat module.
#[derive(Debug, Clone)]
pub struct PgChatRepo {
    pool: PgPool,
}

impl PgChatRepo {
    /// Read and write through `pool`.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// The flat projection a chat query returns: the message, plus its sender's columns.
///
/// Private, and the only reason it exists is that `sqlx::FromRow` cannot fill a nested
/// struct. [`ChatMessage`] is what leaves this file.
#[derive(Debug, sqlx::FromRow)]
struct MessageRow {
    id: Uuid,
    broadcast_id: Uuid,
    content: String,
    created_at: OffsetDateTime,
    updated_at: Option<OffsetDateTime>,
    sender_id: Uuid,
    sender_name: String,
    sender_bio: Option<String>,
    sender_avatar_id: Option<String>,
    sender_avatar_url: Option<String>,
}

impl From<MessageRow> for ChatMessage {
    fn from(row: MessageRow) -> Self {
        Self {
            id: row.id,
            broadcast_id: row.broadcast_id,
            content: row.content,
            created_at: row.created_at,
            updated_at: row.updated_at,
            sender: SenderSummary {
                id: row.sender_id,
                full_name: row.sender_name,
                bio: row.sender_bio,
                avatar_id: row.sender_avatar_id,
                avatar_url: row.sender_avatar_url,
            },
        }
    }
}

/// The columns every message read projects, in one place so a query and its row type
/// cannot disagree.
///
/// Spliced into each statement below through [`statement`], rather than written out
/// three times: the projection and the aliases it must produce are one fact, and three
/// copies of it are three chances for one of them to drift from [`MessageRow`].
const MESSAGE_SELECT: &str = "m.id, m.broadcast_id, m.content, m.created_at, m.updated_at, \
     u.id AS sender_id, u.full_name AS sender_name, u.bio AS sender_bio, \
     u.avatar_id AS sender_avatar_id, u.avatar_url AS sender_avatar_url";

/// Substitute the module's own column lists into a statement template.
///
/// `AssertSqlSafe` is not a formality: it is a claim that nothing a *caller* supplied is
/// in this string, and the only caller-supplied part of a statement in this file is a
/// bind parameter appended after it. The templates below are `const`s and their
/// placeholders name `const`s, so the claim holds structurally rather than by inspection.
fn statement(template: &str) -> AssertSqlSafe<String> {
    AssertSqlSafe(template.replace("{MESSAGE_SELECT}", MESSAGE_SELECT))
}

/// Log the driver's text and return a 500 that does not contain it.
fn database(context: &'static str, failure: sqlx::Error) -> MenoError {
    tracing::error!(error = %failure, context, "chat repository call failed");
    error::internal(context, failure)
}

/// The guard both chat writes share: the broadcast is live, and the sender is in it.
///
/// Written once and spliced into both templates, because the two must agree — a
/// reaction that could be posted where a message could not (or the reverse) is a rule
/// nobody could state in one sentence.
const WRITE_GUARD: &str = "\
     WHERE EXISTS (\n\
             SELECT 1 FROM broadcasts b\n\
             WHERE b.id = $3\n\
               AND b.status = 'active'\n\
               AND b.end_time IS NULL\n\
               AND b.deleted_at IS NULL\n\
         )\n\
       AND EXISTS (\n\
             SELECT 1 FROM broadcast_participants p\n\
             WHERE p.broadcast_id = $3\n\
               AND p.participant_id = $2\n\
               AND p.left_at IS NULL\n\
         )";

/// Insert a message that the guard admits, and return it with its sender.
///
/// The `WITH … RETURNING *` is what makes the check, the write and the join one round
/// trip: the join reads the row this statement just wrote, so a concurrent edit cannot
/// change what the caller is told it created.
fn insert_message_statement() -> AssertSqlSafe<String> {
    statement(&format!(
        "WITH inserted AS (\n\
             INSERT INTO chat_messages (content, sender_id, broadcast_id)\n\
             SELECT $1, $2, $3\n\
             {WRITE_GUARD}\n\
             RETURNING *\n\
         )\n\
         SELECT {{MESSAGE_SELECT}}\n\
         FROM inserted m\n\
         JOIN users u ON u.id = m.sender_id"
    ))
}

/// Insert a reaction the guard admits, and return it.
///
/// Spliced from [`WRITE_GUARD`], the same const the message insert uses, so the two can
/// only ever admit the same states.
fn insert_reaction_statement() -> AssertSqlSafe<String> {
    statement(&format!(
        "INSERT INTO chat_reactions (content, sender_id, broadcast_id)\n\
         SELECT $1, $2, $3\n\
         {WRITE_GUARD}\n\
         RETURNING id, broadcast_id, sender_id, content, created_at"
    ))
}

#[async_trait::async_trait]
impl ChatRepo for PgChatRepo {
    async fn insert_message(&self, new: NewMessage<'_>) -> Result<Option<ChatMessage>, MenoError> {
        sqlx::query_as::<_, MessageRow>(insert_message_statement())
            .bind(new.content)
            .bind(new.sender_id)
            .bind(new.broadcast_id)
            .fetch_optional(&self.pool)
            .await
            .map(|row| row.map(ChatMessage::from))
            .map_err(|e| database("insert_message", e))
    }

    async fn find_message(&self, message_id: Uuid) -> Result<Option<ChatMessage>, MenoError> {
        sqlx::query_as::<_, MessageRow>(statement(
            "SELECT {MESSAGE_SELECT} \
             FROM chat_messages m \
             JOIN users u ON u.id = m.sender_id \
             WHERE m.id = $1 AND m.deleted_at IS NULL",
        ))
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(ChatMessage::from))
        .map_err(|e| database("find_message", e))
    }

    async fn update_message(
        &self,
        message_id: Uuid,
        sender_id: Uuid,
        content: &str,
    ) -> Result<Option<ChatMessage>, MenoError> {
        // Ownership and liveness are both in the `WHERE`, so a message that was deleted
        // or reassigned between the caller's read and this write updates nothing rather
        // than an unrelated row. `updated_at` is the trigger's — see the module docs.
        sqlx::query_as::<_, MessageRow>(statement(
            "WITH updated AS (\n\
                 UPDATE chat_messages\n\
                 SET content = $2\n\
                 WHERE id = $1 AND sender_id = $3 AND deleted_at IS NULL\n\
                 RETURNING *\n\
             )\n\
             SELECT {MESSAGE_SELECT}\n\
             FROM updated m\n\
             JOIN users u ON u.id = m.sender_id",
        ))
        .bind(message_id)
        .bind(content)
        .bind(sender_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(ChatMessage::from))
        .map_err(|e| database("update_message", e))
    }

    async fn soft_delete_message(
        &self,
        message_id: Uuid,
        sender_id: Uuid,
    ) -> Result<bool, MenoError> {
        let result = sqlx::query(
            "UPDATE chat_messages SET deleted_at = now() \
             WHERE id = $1 AND sender_id = $2 AND deleted_at IS NULL",
        )
        .bind(message_id)
        .bind(sender_id)
        .execute(&self.pool)
        .await
        .map_err(|e| database("soft_delete_message", e))?;

        // `rows_affected` rather than a follow-up `SELECT`: a second delete touches no
        // row, and the caller needs only to know that it did not.
        Ok(result.rows_affected() == 1)
    }

    async fn list_messages(&self, filter: MessageFilter) -> Result<Vec<ChatMessage>, MenoError> {
        let (cursor_ts, cursor_id) = match filter.cursor {
            None => (None, None),
            Some((ts, id)) => (Some(ts), Some(id)),
        };

        // `QueryBuilder`, not `statement`: the keyset clause is appended conditionally,
        // and this is the constructor sqlx provides for exactly that.
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!(
            "SELECT {MESSAGE_SELECT} \
             FROM chat_messages m \
             JOIN users u ON u.id = m.sender_id AND u.deleted_at IS NULL \
             WHERE m.broadcast_id = "
        ));
        qb.push_bind(filter.broadcast_id)
            .push(" AND m.deleted_at IS NULL ");

        push_cursor_condition(
            &mut qb,
            ColumnName::literal("m.created_at"),
            ColumnName::literal("m.id"),
            cursor_ts,
            cursor_id,
            filter.order,
        );

        push_order_and_limit(
            &mut qb,
            ColumnName::literal("m.created_at"),
            ColumnName::literal("m.id"),
            filter.order,
            filter.limit_plus_one,
        );

        qb.build_query_as::<MessageRow>()
            .fetch_all(&self.pool)
            .await
            .map(|rows| rows.into_iter().map(ChatMessage::from).collect())
            .map_err(|e| database("list_messages", e))
    }

    async fn insert_reaction(
        &self,
        new: NewReaction<'_>,
    ) -> Result<Option<ChatReaction>, MenoError> {
        // The same guard as `insert_message`, spliced from the same const: a reaction is
        // as much a thing said into a live room as a message is.
        sqlx::query_as::<_, ChatReaction>(insert_reaction_statement())
            .bind(new.content)
            .bind(new.sender_id)
            .bind(new.broadcast_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| database("insert_reaction", e))
    }

    async fn is_live_broadcast(&self, broadcast_id: Uuid) -> Result<bool, MenoError> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM broadcasts \
             WHERE id = $1 AND status = 'active' AND end_time IS NULL AND deleted_at IS NULL)",
        )
        .bind(broadcast_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| database("is_live_broadcast", e))
    }

    async fn is_participant(&self, broadcast_id: Uuid, user_id: Uuid) -> Result<bool, MenoError> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM broadcast_participants \
             WHERE broadcast_id = $1 AND participant_id = $2 AND left_at IS NULL)",
        )
        .bind(broadcast_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| database("is_participant", e))
    }
}

#[cfg(test)]
mod tests {
    //! The parts of this file that are checkable without a database.
    //!
    //! Which is the projection and its mapping: a column the migration never declared
    //! is a runtime failure, and the only place to catch the *shape* of it here is the
    //! const, the templates that splice it, and the row type agreeing.

    use super::*;

    #[test]
    fn the_projection_aliases_every_sender_column_the_row_expects() {
        // `MESSAGE_SELECT` is interpolated into three statements and read by one
        // `FromRow`; a rename in either place is a runtime `ColumnNotFound`.
        for alias in [
            "id",
            "broadcast_id",
            "content",
            "created_at",
            "updated_at",
            "sender_id",
            "sender_name",
            "sender_bio",
            "sender_avatar_id",
            "sender_avatar_url",
        ] {
            assert!(
                MESSAGE_SELECT.contains(alias),
                "the projection must select `{alias}`: {MESSAGE_SELECT}"
            );
        }
    }

    #[test]
    fn every_template_is_resolved_and_none_is_left_holding_a_placeholder() {
        // A statement that reached Postgres with `{MESSAGE_SELECT}` still in it would
        // fail at runtime, on a request path, with a syntax error nobody can read.
        let spliced = [
            insert_message_statement().0,
            statement("SELECT {MESSAGE_SELECT} FROM chat_messages m").0,
            insert_reaction_statement().0,
        ];

        for sql in &spliced {
            assert!(
                !sql.contains('{') && !sql.contains('}'),
                "an unresolved placeholder was left in: {sql}"
            );
        }
    }

    #[test]
    fn both_writes_carry_the_same_guard() {
        // The two guarded inserts must refuse for the same two reasons. Stated as a
        // check on the SQL the driver would receive, because that is what a divergence
        // would look like: one write that admits a message into an ended broadcast.
        for sql in [insert_message_statement().0, insert_reaction_statement().0] {
            let normalised = sql.split_whitespace().collect::<Vec<_>>().join(" ");
            for clause in [
                "b.status = 'active'",
                "b.end_time IS NULL",
                "b.deleted_at IS NULL",
                "p.left_at IS NULL",
            ] {
                assert!(
                    normalised.contains(clause),
                    "a guarded write is missing `{clause}`: {normalised}"
                );
            }
        }
    }

    #[test]
    fn a_row_maps_into_a_message_without_losing_the_sender() {
        let sent = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("a fixture instant");
        let message = ChatMessage::from(MessageRow {
            id: Uuid::from_u128(1),
            broadcast_id: Uuid::from_u128(2),
            content: "hello".to_owned(),
            created_at: sent,
            updated_at: None,
            sender_id: Uuid::from_u128(3),
            sender_name: "Ada Lovelace".to_owned(),
            sender_bio: Some("mathematician".to_owned()),
            sender_avatar_id: Some("avatars/ada".to_owned()),
            sender_avatar_url: Some("https://cdn.test/ada".to_owned()),
        });

        assert_eq!(message.sender.id, Uuid::from_u128(3));
        assert_eq!(message.sender.full_name, "Ada Lovelace");
        assert_eq!(message.sender.bio.as_deref(), Some("mathematician"));
        assert_eq!(message.created_at, sent);
        assert!(!message.is_edited());
    }

    #[test]
    fn a_filter_without_a_cursor_is_a_first_page() {
        let filter = MessageFilter::first_page(Uuid::from_u128(9), 21);

        assert!(filter.cursor.is_none());
        assert_eq!(filter.limit_plus_one, 21);
        assert_eq!(filter.order, meno_core::Order::Desc);
    }
}
