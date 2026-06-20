#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Note {
    pub id: uuid::Uuid,
    pub title: String,
    pub content: String,
    pub pinned: bool,
    pub folder_id: Option<uuid::Uuid>,
    pub creator_id: uuid::Uuid,
    pub version: i32,
    pub created_at: time::OffsetDateTime,
    pub updated_at: time::OffsetDateTime,
    pub deleted_at: Option<time::OffsetDateTime>,
}
impl Note {
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Folder {
    pub id: uuid::Uuid,
    pub title: String,
    pub pinned: bool,
    pub creator_id: uuid::Uuid,
    pub version: i32,
    pub created_at: time::OffsetDateTime,
    pub updated_at: time::OffsetDateTime,
    pub deleted_at: Option<time::OffsetDateTime>,
}
