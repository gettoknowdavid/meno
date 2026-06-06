use crate::modules::broadcast::dto::ParticipantListItem;
use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::subscribers::errors::SubscribersError;
use crate::shared::pagination::PaginationDirection;
use crate::shared::types::dto::UserSummary;
use sqlx::QueryBuilder;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct SubscribersRepository {
    db: sqlx::PgPool,
}
impl SubscribersRepository {
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }

    pub async fn create(
        &self,
        subscriber_id: Uuid,
        subscription_id: Uuid,
    ) -> Result<(), SubscribersError> {
        sqlx::query!(
            r#"INSERT INTO user_subscribers (subscriber_id, subscription_id)
            VALUES ($1, $2)
            ON CONFLICT (subscriber_id, subscription_id) DO NOTHING"#,
            subscriber_id,
            subscription_id,
        )
        .execute(&self.db)
        .await
        .map_err(SubscribersError::Database)?;
        Ok(())
    }

    pub async fn delete(
        &self,
        subscriber_id: Uuid,
        subscription_id: Uuid,
    ) -> Result<(), SubscribersError> {
        sqlx::query!(
            r#"DELETE FROM user_subscribers WHERE subscriber_id = $1 AND subscription_id = $2"#,
            subscriber_id,
            subscription_id,
        )
        .execute(&self.db)
        .await
        .map_err(SubscribersError::Database)?;
        Ok(())
    }

    pub async fn find_subscribers(
        &self,
        subscription_id: Uuid,
        limit: i64,
        cursor: Option<Uuid>,
        direction: PaginationDirection,
    ) -> Result<Vec<UserSummary>, SubscribersError> {
        let limit = limit.clamp(1, 100);
        let fetch_limit = limit + 1;

        let mut query = QueryBuilder::new(
            r#"SELECT u.id, u.full_name, u.bio, u.avatar_url, u.avatar_id
                FROM user_subscribers us
                INNER JOIN users u ON u.id = us.subscriber_id AND u.deleted_at IS NULL
                WHERE us.subscription_id = "#,
        );

        query.push_bind(subscription_id);

        match direction {
            PaginationDirection::Next => {
                if let Some(cursor_id) = cursor {
                    query.push(" AND us.subscriber_id < ");
                    query.push_bind(cursor_id);
                }
                query.push(" ORDER BY us.subscriber_id DESC ");
            }
            PaginationDirection::Previous => {
                if let Some(cursor_id) = cursor {
                    query.push(" AND us.subscriber_id > ");
                    query.push_bind(cursor_id);
                }
                query.push(" ORDER BY us.subscriber_id ASC ");
            }
        }

        query.push(" LIMIT ");
        query.push_bind(fetch_limit);

        let rows = query
            .build_query_as::<UserSummary>()
            .fetch_all(&self.db)
            .await
            .map_err(SubscribersError::Database)?;

        Ok(rows)
    }

    pub async fn user_exists(&self, id: Uuid) -> Result<bool, SubscribersError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND deleted_at IS NULL)
            AS "exists!""#,
            id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(SubscribersError::Database)
    }
}
