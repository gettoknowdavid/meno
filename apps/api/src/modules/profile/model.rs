use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Profile {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub email: String,
    pub avatar_id: Option<String>,
    pub avatar_url: Option<String>,
    pub verified: bool,
    pub followers: i64,
    pub following: i64,
    pub broadcasts: i64,
    pub created_at: OffsetDateTime,
}
