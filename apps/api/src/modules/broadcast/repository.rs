use crate::modules::broadcast::dto::UserSummary;
use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::broadcast::model::{Broadcast, BroadcastParticipant};
use sqlx::{Postgres, Transaction};
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

#[derive(Clone)]
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

    pub async fn find_users_batch(&self, ids: &[Uuid]) -> Result<Vec<UserSummary>, BroadcastError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as!(
            UserSummary,
            r#"SELECT id, full_name, avatar_id, avatar_url
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

    pub async fn find_user_summary(&self, id: Uuid) -> Result<Option<UserSummary>, BroadcastError> {
        sqlx::query_as!(
            UserSummary,
            r#"SELECT id, full_name, avatar_id, avatar_url
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
}
