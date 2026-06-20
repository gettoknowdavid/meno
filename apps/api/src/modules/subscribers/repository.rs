use crate::modules::auth::model::User;
use crate::modules::subscribers::dto::SubscriberItem;
use crate::modules::subscribers::errors::SubscribersError;
use crate::shared::pagination::{CursorParams, Order};
use crate::shared::repository::{push_cursor_condition, push_order_and_limit};
use sqlx::{Postgres, QueryBuilder};
use tracing::instrument;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct SubscribersRepository {
    db: sqlx::PgPool,
}
impl SubscribersRepository {
    #[must_use]
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
pub trait SubscribersRepo: Send + Sync + 'static {
    async fn create(
        &self,
        subscriber_id: Uuid,
        subscription_id: Uuid,
    ) -> Result<(), SubscribersError>;

    async fn delete(
        &self,
        subscriber_id: Uuid,
        subscription_id: Uuid,
    ) -> Result<(), SubscribersError>;

    async fn find_subscribers(
        &self,
        subscription_id: Uuid,
        viewer_id: Option<Uuid>,
        params: &CursorParams,
    ) -> Result<Vec<SubscriberItem>, SubscribersError>;

    async fn find_subscriptions(
        &self,
        subscriber_id: Uuid,
        viewer_id: Option<Uuid>,
        params: &CursorParams,
    ) -> Result<Vec<SubscriberItem>, SubscribersError>;

    async fn user_exists(&self, id: Uuid) -> Result<bool, SubscribersError>;

    /// Returns a Set of user IDs that `viewer_id` follows, from `candidate_ids`.
    /// Used to annotate list results with `is_following`.
    async fn batch_is_subscribed(
        &self,
        viewer_id: Uuid,
        candidate_ids: &[Uuid],
    ) -> Result<std::collections::HashSet<Uuid>, SubscribersError>;

    async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>, SubscribersError>;
}

#[async_trait::async_trait]
impl SubscribersRepo for SubscribersRepository {
    #[instrument(skip(self), fields(subscriber_id = %subscriber_id, subscription_id = %subscription_id))]
    async fn create(
        &self,
        subscriber_id: Uuid,
        subscription_id: Uuid,
    ) -> Result<(), SubscribersError> {
        tracing::info!("Creating subscriber relationship");
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
        tracing::info!("Successfully created subscriber relationship");
        Ok(())
    }

    #[instrument(skip(self), fields(subscriber_id = %subscriber_id, subscription_id = %subscription_id))]
    async fn delete(
        &self,
        subscriber_id: Uuid,
        subscription_id: Uuid,
    ) -> Result<(), SubscribersError> {
        tracing::info!("Deleting subscriber relationship");
        sqlx::query!(
            r#"DELETE FROM user_subscribers WHERE subscriber_id = $1 AND subscription_id = $2"#,
            subscriber_id,
            subscription_id,
        )
        .execute(&self.db)
        .await
        .map_err(SubscribersError::Database)?;
        tracing::info!("Successfully deleted subscriber relationship");
        Ok(())
    }

    #[instrument(skip(self, params, viewer_id), fields(subscription_id = %subscription_id, ?viewer_id, limit = params.limit))]
    async fn find_subscribers(
        &self,
        subscription_id: Uuid,
        viewer_id: Option<Uuid>,
        params: &CursorParams,
    ) -> Result<Vec<SubscriberItem>, SubscribersError> {
        tracing::debug!("Fetching subscribers list");

        let (cursor_ts, cursor_id) = match &params.cursor {
            None => (None, None),
            Some(c) => {
                let (ts, id) = c.to_timestamp_id()?;
                (Some(ts), Some(id))
            }
        };

        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            r"
            SELECT
                u.id,
                u.full_name,
                u.bio,
                u.avatar_url,
                u.avatar_id,
                us.created_at AS subscribed_at,
                false AS is_following
            FROM user_subscribers us
            INNER JOIN users u
                ON u.id = us.subscriber_id
                AND u.deleted_at IS NULL
            WHERE us.subscription_id =
            ",
        );
        qb.push_bind(subscription_id);

        push_cursor_condition(
            &mut qb,
            "us.created_at",
            "us.subscriber_id",
            cursor_ts,
            cursor_id,
            Order::Desc,
        );

        push_order_and_limit(
            &mut qb,
            "us.created_at",
            "us.subscriber_id",
            Order::Desc,
            params.limit_plus_one(),
        );

        let mut rows = qb
            .build_query_as::<SubscriberItem>()
            .fetch_all(&self.db)
            .await?;

        // Batch check: does the viewer follow each of these users?
        if let Some(vid) = viewer_id {
            let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
            let followed = self.batch_is_subscribed(vid, &ids).await?;
            for row in &mut rows {
                row.is_following = followed.contains(&row.id);
            }
        }

        tracing::info!(count = rows.len(), "Fetched subscribers");

        Ok(rows)
    }

    #[instrument(skip(self, params, viewer_id), fields(subscriber_id = %subscriber_id, ?viewer_id, limit = params.limit))]
    async fn find_subscriptions(
        &self,
        subscriber_id: Uuid,
        viewer_id: Option<Uuid>,
        params: &CursorParams,
    ) -> Result<Vec<SubscriberItem>, SubscribersError> {
        let (cursor_ts, cursor_id) = match &params.cursor {
            None => (None, None),
            Some(c) => {
                let (ts, id) = c.to_timestamp_id()?;
                (Some(ts), Some(id))
            }
        };

        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            r"
            SELECT
                u.id,
                u.full_name,
                u.bio,
                u.avatar_url,
                u.avatar_id,
                us.created_at AS subscribed_at,
                false AS is_following
            FROM user_subscribers us
                INNER JOIN users u
                ON u.id = us.subscription_id
                AND u.deleted_at IS NULL
            WHERE us.subscriber_id =
            ",
        );
        qb.push_bind(subscriber_id);

        push_cursor_condition(
            &mut qb,
            "us.created_at",
            "us.subscription_id",
            cursor_ts,
            cursor_id,
            Order::Desc,
        );

        push_order_and_limit(
            &mut qb,
            "us.created_at",
            "us.subscription_id",
            Order::Desc,
            params.limit_plus_one(),
        );

        let mut rows = qb
            .build_query_as::<SubscriberItem>()
            .fetch_all(&self.db)
            .await?;

        // Batch check: does the viewer follow each of these users?
        if let Some(vid) = viewer_id {
            let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
            let followed = self.batch_is_subscribed(vid, &ids).await?;
            for row in &mut rows {
                row.is_following = followed.contains(&row.id);
            }
        }

        Ok(rows)
    }

    async fn user_exists(&self, id: Uuid) -> Result<bool, SubscribersError> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND deleted_at IS NULL)
            AS "exists!""#,
            id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(SubscribersError::Database)
    }

    /// Returns a Set of user IDs that `viewer_id` follows, from `candidate_ids`.
    /// Used to annotate list results with `is_following`.
    async fn batch_is_subscribed(
        &self,
        viewer_id: Uuid,
        candidate_ids: &[Uuid],
    ) -> Result<std::collections::HashSet<Uuid>, SubscribersError> {
        if candidate_ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let rows = sqlx::query_scalar!(
            "SELECT subscription_id FROM user_subscribers
             WHERE subscriber_id = $1 AND subscription_id = ANY($2)",
            viewer_id,
            candidate_ids,
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows.into_iter().collect())
    }

    async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>, SubscribersError> {
        sqlx::query_as!(
            User,
            r#"SELECT
                    id,
                    full_name,
                    bio,
                    email,
                    avatar_id,
                    avatar_url,
                    verified,
                    role,
                    created_at,
                    updated_at,
                    deleted_at
               FROM users WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(SubscribersError::Database)
    }
}
