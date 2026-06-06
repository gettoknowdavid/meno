use crate::modules::auth;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Compact user shape embedded inside broadcast responses.
#[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
pub struct UserSummary {
    pub id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub avatar_id: Option<String>,
    pub avatar_url: Option<String>,
}
impl From<auth::model::User> for UserSummary {
    fn from(u: auth::model::User) -> Self {
        UserSummary {
            id: u.id,
            full_name: u.full_name,
            bio: u.bio,
            avatar_id: u.avatar_id,
            avatar_url: u.avatar_url,
        }
    }
}
