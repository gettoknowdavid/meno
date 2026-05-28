use crate::modules::broadcast::dto::UserSummary;
use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::broadcast::model::{
    Broadcast, BroadcastParticipant, EndReason, ParticipantRole,
};
use sqlx::{Postgres, QueryBuilder, Transaction};
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

pub struct UpsertParticipantInput {
    pub broadcast_id: Uuid,
    pub participant_id: Uuid,
    pub role: ParticipantRole,
    pub joined_at: OffsetDateTime,
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

    pub async fn upsert_participant<'t>(
        &self,
        input: &UpsertParticipantInput,
        tx: &mut Transaction<'t, Postgres>,
    ) -> Result<(), BroadcastError> {
        sqlx::query!(
            r#"INSERT INTO broadcast_participants (broadcast_id, participant_id, role, joined_at)
               VALUES ($1, $2, $3::text, $4)
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
        sqlx::query!(
            r#"UPDATE broadcasts
               SET status = 'inactive', end_time   = NOW(), end_reason = $2, updated_at = NOW()
               WHERE id = $1"#,
            broadcast_id,
            reason.to_string(),
        )
        .execute(&mut **tx)
        .await
        .map_err(BroadcastError::Database)?;
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

    pub async fn get_cohosts(
        &self,
        broadcast_id: Uuid,
    ) -> Result<Vec<UserSummary>, BroadcastError> {
        sqlx::query_as!(
            UserSummary,
            r#"SELECT u.id, u.full_name, u.avatar_id, u.avatar_url
               FROM broadcast_cohosts bc
               JOIN users u ON u.id = bc.cohost_id
               WHERE bc.broadcast_id = $1 AND bc.removed_at IS NULL AND deleted_at IS NUll"#,
            broadcast_id,
        )
        .fetch_all(&self.db)
        .await
        .map_err(BroadcastError::Database)
    }
}
