use crate::modules::auth::model::AuthProvider;
use crate::modules::profile::dto::{ProfileSearchQuery, ProfileSearchResult};
use crate::modules::profile::errors::ProfileError;
use crate::modules::profile::model::Profile;
use sqlx::{Postgres, QueryBuilder};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProfileRepository {
    db: sqlx::PgPool,
}
impl ProfileRepository {
    #[must_use]
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
pub trait ProfileRepo: Send + Sync + 'static {
    async fn find_by_id(&self, user_id: Uuid) -> Result<Option<Profile>, ProfileError>;
    async fn find_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, ProfileError>;
    async fn find_avatar_key(&self, user_id: Uuid) -> Result<Option<String>, ProfileError>;
    async fn update_profile(
        &self,
        id: Uuid,
        full_name: Option<&str>,
        bio: Option<&str>,
        avatar_key: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<Profile, ProfileError>;
    async fn is_following(
        &self,
        subscription_id: Uuid,
        subscriber_id: Uuid,
    ) -> Result<bool, ProfileError>;
    async fn search_profiles(
        &self,
        query: &ProfileSearchQuery,
        current_user_id: Uuid,
    ) -> Result<Vec<ProfileSearchResult>, ProfileError>;
}

#[async_trait::async_trait]
impl ProfileRepo for ProfileRepository {
    async fn find_by_id(&self, user_id: Uuid) -> Result<Option<Profile>, ProfileError> {
        sqlx::query_as!(
            Profile,
            r#"SELECT id, full_name, bio, email, avatar_id, avatar_url,
                      verified, followers, following, broadcasts, created_at
               FROM users WHERE id = $1 AND deleted_at IS NULL"#,
            user_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(ProfileError::Database)
    }
    async fn find_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, ProfileError> {
        let rows = sqlx::query!(
            "SELECT provider_type::text as provider_type FROM user_identities WHERE user_id = $1",
            user_id,
        )
        .fetch_all(&self.db)
        .await
        .map_err(ProfileError::Database)?;

        let providers = rows
            .iter()
            .filter_map(|r| AuthProvider::from_str(&r.provider_type).ok())
            .collect();

        Ok(providers)
    }
    async fn find_avatar_key(&self, user_id: Uuid) -> Result<Option<String>, ProfileError> {
        sqlx::query_scalar!("SELECT avatar_id FROM users WHERE id = $1", user_id)
            .fetch_optional(&self.db)
            .await
            .map_err(ProfileError::Database)
            .map(|r| r.flatten())
    }
    async fn update_profile(
        &self,
        id: Uuid,
        full_name: Option<&str>,
        bio: Option<&str>,
        avatar_key: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<Profile, ProfileError> {
        sqlx::query_as!(
            Profile,
            r#"UPDATE users SET
                 full_name =  COALESCE($1, full_name),
                 bio = COALESCE($2, bio),
                 avatar_id = COALESCE($3, avatar_id),
                 avatar_url = COALESCE($4, avatar_url),
                 updated_at = NOW()
                WHERE id = $5 AND deleted_at IS NULL
                RETURNING id, full_name, bio, email, avatar_id, avatar_url,
                          verified, followers, following, broadcasts, created_at"#,
            full_name,
            bio,
            avatar_key,
            avatar_url,
            id,
        )
        .fetch_one(&self.db)
        .await
        .map_err(ProfileError::Database)
    }
    async fn is_following(
        &self,
        subscription_id: Uuid,
        subscriber_id: Uuid,
    ) -> Result<bool, ProfileError> {
        let exists = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM user_subscribers
                WHERE subscription_id = $1 AND subscriber_id = $2
            )"#,
            subscription_id,
            subscriber_id
        )
        .fetch_one(&self.db)
        .await
        .map_err(ProfileError::Database)?;
        Ok(exists.unwrap_or(false))
    }
    async fn search_profiles(
        &self,
        query: &ProfileSearchQuery,
        current_user_id: Uuid,
    ) -> Result<Vec<ProfileSearchResult>, ProfileError> {
        let (cursor_rank, cursor_ts, cursor_id) = match &query.cursor() {
            None => (None, None, None),
            Some(c) => {
                let (rank, ts, id) = c.to_rank_timestamp_id()?;
                (Some(rank), Some(ts), Some(id))
            }
        };

        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            r"
            SELECT
                u.id,
                u.full_name,
                u.bio,
                u.avatar_url,
                u.followers,
                u.following,
                u.broadcasts,
                u.created_at
                EXISTS(
                    SELECT 1 FROM user_subscribers us
                    WHERE us.subscriber_id =
            ",
        );

        qb.push_bind(current_user_id);

        qb.push(
            " AND us.subscription_id = u.id
            ) AS is_following,
            ts_rank(to_tsvector('english', full_name), plainto_tsquery('english',",
        );

        qb.push_bind(&query.q)
            .push(
                r")) AS rank
                    FROM users
                    WHERE deleted_at IS NULL
                    AND to_tsvector('english', full_name) @@ plainto_tsquery('english',
                    ",
            )
            .push_bind(&query.q)
            .push(")");

        // Cursor condition for rank-based pagination
        // We use (rank, created_at, id) as the composite cursor
        if let (Some(rank), Some(ts), Some(id)) = (cursor_rank, cursor_ts, cursor_id) {
            qb.push(" AND (rank, u.created_at, u.id) < (")
                .push_bind(rank)
                .push(", ")
                .push_bind(ts)
                .push(", ")
                .push_bind(id)
                .push(")");
        }

        // Sort by rank first, then recency for stable cursor
        qb.push(" ORDER BY rank DESC, created_at DESC, id DESC")
            .push(" LIMIT ")
            .push_bind(query.limit_plus_one());

        let rows = qb
            .build_query_as::<ProfileSearchResult>()
            .fetch_all(&self.db)
            .await?;

        Ok(rows)
    }
}
