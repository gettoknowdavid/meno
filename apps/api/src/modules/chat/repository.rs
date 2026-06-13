use crate::modules::chat::dto::{
    ChatMessageQuery, ChatMessageResponse, ChatMessageRow, ChatReactionRow,
};
use crate::modules::chat::errors::ChatError;
use crate::shared::pagination::Order;
use crate::shared::repository::{push_cursor_condition, push_order_and_limit};
use sqlx::QueryBuilder;
use uuid::Uuid;

#[derive(Clone)]
pub struct ChatRepository {
    db: sqlx::PgPool,
}
impl ChatRepository {
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }

    pub async fn create_message(
        &self,
        broadcast_id: Uuid,
        sender_id: Uuid,
        content: &str,
    ) -> Result<ChatMessageRow, ChatError> {
        sqlx::query_as!(
            ChatMessageRow,
            r#"WITH inserted AS (
                    INSERT INTO chat_messages (content, sender_id, broadcast_id)
                    VALUES ($1, $2, $3)
                    RETURNING *
            )
            SELECT i.*,
                   u.full_name AS sender_name,
                   u.bio AS sender_bio,
                   u.avatar_id AS sender_avatar_id,
                   u.avatar_url AS sender_avatar_url
            FROM inserted i
            JOIN users u ON u.id = i.sender_id"#,
            content,
            sender_id,
            broadcast_id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(ChatError::Database)
    }

    pub async fn find_messages(
        &self,
        broadcast_id: Uuid,
        query: &ChatMessageQuery,
    ) -> Result<Vec<ChatMessageResponse>, ChatError> {
        let (cursor_ts, cursor_id) = match query.cursor() {
            None => (None, None),
            Some(c) => {
                let (ts, id) = c.to_timestamp_id()?;
                (Some(ts), Some(id))
            }
        };

        let mut qb = QueryBuilder::new(
            r#"SELECT
                        m.id,
                        m.content,
                        m.broadcast_id,
                        m.created_at,
                        m.updated_at,
                        m.deleted_at,
                        m.sender_id,
                        u.full_name AS sender_name,
                        u.bio AS sender_bio
                        u.avatar_id AS sender_avatar_id,
                        u.avatar_url AS sender_avatar_url
            FROM chat_messages m
            JOIN users u ON u.id = m.sender_id AND u.deleted_at IS NULL
            WHERE m.broadcast_id = "#,
        );
        qb.push_bind(broadcast_id)
            .push(" AND m.deleted_at IS NULL ");

        push_cursor_condition(
            &mut qb,
            "m.created_at",
            "m.id",
            cursor_ts,
            cursor_id,
            Order::Desc,
        );

        push_order_and_limit(
            &mut qb,
            "m.created_at",
            "m.id",
            Order::Desc,
            query.limit_plus_one(),
        );

        let rows = qb
            .build_query_as::<ChatMessageRow>()
            .fetch_all(&self.db)
            .await?;

        Ok(rows.into_iter().map(ChatMessageResponse::from).collect())
    }

    pub async fn find_message_by_id(
        &self,
        message_id: Uuid,
    ) -> Result<Option<ChatMessageRow>, ChatError> {
        sqlx::query_as!(
            ChatMessageRow,
            r#"SELECT
                    m.*,
                    u.full_name AS sender_name,
                    u.bio AS sender_bio,
                    u.avatar_id AS sender_avatar_id,
                    u.avatar_url AS sender_avatar_url
            FROM chat_messages m
            JOIN users u ON u.id = m.sender_id
            WHERE m.id = $1 AND m.deleted_at IS NULL"#,
            message_id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(ChatError::Database)
    }

    pub async fn update_message(
        &self,
        message_id: Uuid,
        sender_id: Uuid,
        content: &str,
    ) -> Result<Option<ChatMessageRow>, ChatError> {
        sqlx::query_as!(
            ChatMessageRow,
            r#"WITH updated_message AS (
                    UPDATE chat_messages
                    SET content = $1, updated_at = NOW()
                    WHERE id = $2 AND sender_id = $3 AND deleted_at IS NULL
                    RETURNING *
            ) SELECT
                  um.*,
                  u.full_name AS sender_name,
                  u.bio AS sender_bio,
                  u.avatar_id AS sender_avatar_id,
                  u.avatar_url AS sender_avatar_url
            FROM updated_message um
            JOIN users u ON u.id = um.sender_id"#,
            content,
            message_id,
            sender_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(ChatError::Database)
    }

    pub async fn soft_delete_message(
        &self,
        message_id: Uuid,
        sender_id: Uuid,
    ) -> Result<bool, ChatError> {
        let result = sqlx::query!(
            r#"UPDATE chat_messages
            SET deleted_at = NOW()
            WHERE id = $1 AND sender_id = $2 AND deleted_at IS NULL
            "#,
            message_id,
            sender_id,
        )
        .execute(&self.db)
        .await
        .map_err(ChatError::Database)?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn create_reaction(
        &self,
        broadcast_id: Uuid,
        sender_id: Uuid,
        content: &str,
    ) -> Result<ChatReactionRow, ChatError> {
        sqlx::query_as!(
            ChatReactionRow,
            r#"INSERT INTO chat_reactions (content, sender_id, broadcast_id)
            VALUES ($1, $2, $3)
            RETURNING *"#,
            content,
            sender_id,
            broadcast_id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(ChatError::Database)
    }

    pub async fn is_broadcast_participant(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<bool, ChatError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM broadcast_participants
                WHERE broadcast_id = $1 AND participant_id = $2 AND left_at IS NULL
            ) AS "exists!""#,
            broadcast_id,
            participant_id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(ChatError::Database)
    }

    pub async fn is_active_broadcast(&self, broadcast_id: Uuid) -> Result<bool, ChatError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS(
                    SELECT 1 FROM broadcasts
                    WHERE id = $1
                      AND status = 'active'
                      AND end_time IS NULL
                      AND deleted_at IS NULL
            ) AS "exists!""#,
            broadcast_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(ChatError::Database)
    }
}
