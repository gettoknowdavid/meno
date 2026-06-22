#[derive(Debug, serde::Deserialize, validator::Validate)]
pub struct CreateNoteRequest {
    pub id: uuid::Uuid,
    #[validate(length(max = 100, message = "Title cannot exceed 100 characters"))]
    pub title: String,
    #[validate(length(max = 10000, message = "Content cannot exceed 10,000 characters"))]
    pub content: String,
    #[serde(default)]
    pub pinned: bool,
    pub folder_id: Option<uuid::Uuid>,
}

#[derive(Debug, serde::Deserialize, validator::Validate)]
pub struct CreateFolderRequest {
    pub id: uuid::Uuid,
    #[validate(length(max = 100, message = "Title cannot exceed 100 characters"))]
    pub title: String,
    #[serde(default)]
    pub pinned: Option<bool>,
}

#[derive(Debug, serde::Deserialize, validator::Validate)]
pub struct UpdateNoteRequest {
    #[validate(length(max = 100, message = "Title cannot exceed 100 characters"))]
    pub title: Option<String>,
    #[validate(length(max = 10000, message = "Content cannot exceed 10,000 characters"))]
    pub content: Option<String>,
    pub pinned: Option<bool>,
    pub folder_id: Option<uuid::Uuid>,
    pub base_version: i32,
}

#[derive(Debug, serde::Deserialize, validator::Validate)]
pub struct UpdateFolderRequest {
    #[validate(length(max = 100, message = "Title cannot exceed 100 characters"))]
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub base_version: i32,
}

#[derive(Debug, serde::Deserialize)]
pub struct DeleteNoteRequest {
    pub base_version: i32,
}

#[derive(Debug, serde::Deserialize)]
pub struct DeleteFolderRequest {
    pub base_version: i32,
    pub should_delete_notes: Option<bool>,
}

#[derive(Debug, serde::Deserialize, validator::Validate)]
pub struct MoveNotesToFolderRequest {
    #[validate(length(min = 1, max = 200, message = "Provide between 1 and 200 note ids"))]
    pub note_ids: Vec<uuid::Uuid>,
    pub folder_id: Option<uuid::Uuid>,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct NotesQuery {
    pub folder_id: Option<uuid::Uuid>,
    pub pinned: Option<bool>,
    pub keywords: Option<String>,
    #[serde(flatten)]
    pub pagination: crate::shared::pagination::CursorParams,
}
impl NotesQuery {
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }
    pub fn cursor(&self) -> Option<&crate::shared::pagination::Cursor> {
        self.pagination.cursor.as_ref()
    }
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct FoldersQuery {
    pub pinned: Option<bool>,
    pub keywords: Option<String>,
    #[serde(flatten)]
    pub pagination: crate::shared::pagination::CursorParams,
}
impl FoldersQuery {
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }
    pub fn cursor(&self) -> Option<&crate::shared::pagination::Cursor> {
        self.pagination.cursor.as_ref()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NoteDto {
    pub id: uuid::Uuid,
    pub title: String,
    pub content: String,
    pub pinned: bool,
    pub folder_id: Option<uuid::Uuid>,
    pub version: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: time::OffsetDateTime,
    pub deleted: bool,
}
impl From<crate::modules::notes::model::Note> for NoteDto {
    fn from(n: crate::modules::notes::model::Note) -> Self {
        Self {
            id: n.id,
            title: n.title,
            content: n.content,
            pinned: n.pinned,
            folder_id: n.folder_id,
            version: n.version,
            created_at: n.created_at,
            updated_at: n.updated_at,
            deleted: n.deleted_at.is_some(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FolderDto {
    pub id: uuid::Uuid,
    pub title: String,
    pub pinned: bool,
    pub version: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: time::OffsetDateTime,
    pub deleted: bool,
}
impl From<crate::modules::notes::model::Folder> for FolderDto {
    fn from(f: crate::modules::notes::model::Folder) -> Self {
        Self {
            id: f.id,
            title: f.title,
            pinned: f.pinned,
            version: f.version,
            created_at: f.created_at,
            updated_at: f.updated_at,
            deleted: f.deleted_at.is_some(),
        }
    }
}

/// Used inside `MutationResult.server_entity` and `NotesError::VersionConflict` —
/// either entity type can be returned from the same generic sync machinery.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum ConflictEntity {
    Note(NoteDto),
    Folder(FolderDto),
}
impl ConflictEntity {
    pub fn id(&self) -> uuid::Uuid {
        match self {
            ConflictEntity::Note(n) => n.id,
            ConflictEntity::Folder(f) => f.id,
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct NotesSyncQuery {
    pub notes_cursor: Option<crate::shared::pagination::Cursor>,
    pub folders_cursor: Option<crate::shared::pagination::Cursor>,
    pub limit: Option<i64>,
}
impl NotesSyncQuery {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(200).clamp(1, 500)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct NotesSyncResponse {
    pub notes: Vec<NoteDto>,
    pub notes_next_cursor: Option<crate::shared::pagination::Cursor>,
    pub notes_has_more: bool,
    pub folders: Vec<FolderDto>,
    pub folders_next_cursor: Option<crate::shared::pagination::Cursor>,
    pub folders_has_more: bool,
    /// Informational only — NOT used as a cursor. The per-entity cursors
    /// above are the only valid resume points.
    #[serde(with = "time::serde::rfc3339")]
    pub server_time: time::OffsetDateTime,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct NoteMutation {
    pub id: uuid::Uuid,
    pub title: String,
    pub content: String,
    pub pinned: bool,
    pub folder_id: Option<uuid::Uuid>,
    /// Full-snapshot semantics: this is the note's complete desired state,
    /// not a delta — see the note on (A) above.
    pub deleted: bool,
    /// `None` = "I believe this id has never reached the server before."
    /// `Some(v)` = "the last version I saw was v; apply only if still current."
    pub base_version: Option<i32>,
    /// Client-side wall clock at the moment this mutation was made locally.
    /// Used ONLY as the last-write-wins tiebreaker when a version conflict
    /// is detected — never trusted for anything else (clock skew is real).
    #[serde(with = "time::serde::rfc3339")]
    pub client_updated_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FolderMutation {
    pub id: uuid::Uuid,
    pub title: String,
    pub pinned: bool,
    pub deleted: bool,
    pub base_version: Option<i32>,
    #[serde(with = "time::serde::rfc3339")]
    pub client_updated_at: time::OffsetDateTime,
}

/// Internally-tagged on `entityType` so a single mutation log can interleave
/// note and folder changes in whatever order the client's local queue has
/// them — ordering inside the array is preserved in the response, but the
/// server is free to (and does) reorder *processing* internally; see service.rs.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "entityType", rename_all = "snake_case")]
pub enum SyncMutation {
    Note(NoteMutation),
    Folder(FolderMutation),
}

#[derive(Debug, serde::Deserialize)]
pub struct NotesSyncPushRequest {
    pub mutations: Vec<SyncMutation>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Note,
    Folder,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MutationOutcome {
    /// Applied to the database (or, for `applied_unpersisted`, correctly
    /// determined that nothing needed to be written).
    Applied,

    /// `base_version` was stale — `server_entity` carries the current row
    /// so the client's sync engine can resolve the conflict without a
    /// second round trip.
    Conflict,

    /// Failed per-item validation. Does NOT fail the rest of the batch.
    Rejected,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationResult {
    pub id: uuid::Uuid,
    pub entity_type: EntityType,
    pub outcome: MutationOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_entity: Option<ConflictEntity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
impl MutationResult {
    pub fn applied(entity_type: EntityType, entity: ConflictEntity) -> Self {
        Self {
            id: entity.id(),
            entity_type,
            outcome: MutationOutcome::Applied,
            server_entity: Some(entity),
            reason: None,
        }
    }
    /// Used when a "create-then-delete-while-offline" mutation is acknowledged
    /// without ever touching the database — see service.rs for why that's safe.
    pub fn applied_unpersisted(id: uuid::Uuid, entity_type: EntityType) -> Self {
        Self {
            id,
            entity_type,
            outcome: MutationOutcome::Applied,
            server_entity: None,
            reason: None,
        }
    }
    pub fn conflict(entity_type: EntityType, entity: ConflictEntity) -> Self {
        Self {
            id: entity.id(),
            entity_type,
            outcome: MutationOutcome::Conflict,
            server_entity: Some(entity),
            reason: None,
        }
    }
    pub fn rejected(id: uuid::Uuid, entity_type: EntityType, reason: impl Into<String>) -> Self {
        Self {
            id,
            entity_type,
            outcome: MutationOutcome::Rejected,
            server_entity: None,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct NotesSyncPushResponse {
    pub results: Vec<MutationResult>,
}
