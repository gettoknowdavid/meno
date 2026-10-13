//! The notes wire contract (§9.5).
//!
//! # Casing
//!
//! `camelCase`, like `broadcast`, `chat` and `profile`: `folderId`, `baseVersion`,
//! `noteIds`, `nextCursor`. `master` emitted `snake_case` for every field here, which
//! meant a Dart client needed a second naming rule for one feature.
//!
//! # Dates
//!
//! Every timestamp is RFC 3339 with `#[serde(with = "time::serde::rfc3339")]`, so a date
//! cannot become a nine-number tuple by accident; `apps/api/tests/wire_dates.rs` enforces
//! it for this module like every other.
//!
//! # Validation is hand-written, at the boundary
//!
//! `master` used `validator`'s derive attributes, which produced a second error shape
//! (its `ValidationErrors` does not serialise to §4.2's `data` map). Every request type
//! here has a [`Validatable`] impl that accumulates into [`error::FieldErrors`], so a
//! client with two mistakes learns both in one round trip, and the rules quote the same
//! constants the columns mirror.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::error::{self, FieldErrors};
use super::model::{
    Folder, MAX_CONTENT_CHARS, MAX_NOTES_PER_MOVE, MAX_TITLE_CHARS, Note, SyncMutation,
};
use meno_core::Error as MenoError;

/// A request that can check its own shape before a handler looks at it.
pub trait Validatable {
    /// Run every rule, accumulating rather than short-circuiting.
    ///
    /// # Errors
    ///
    /// [`MenoError::Validation`] listing every field that failed and why.
    fn validate(&self) -> Result<(), MenoError>;
}

/// `POST /notes`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNoteRequest {
    /// The client-chosen id, so an offline-created note keeps it.
    pub id: Uuid,
    /// Title, up to [`MAX_TITLE_CHARS`] characters.
    pub title: String,
    /// Body, up to [`MAX_CONTENT_CHARS`] characters.
    pub content: String,
    /// Whether the note is pinned.
    #[serde(default)]
    pub pinned: bool,
    /// The folder to file it in.
    pub folder_id: Option<Uuid>,
}

impl Validatable for CreateNoteRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();
        validate_title(Some(&self.title), &mut fields);
        validate_content(Some(&self.content), &mut fields);
        fields.into_result()
    }
}

/// `PATCH /notes/{id}`.
///
/// Every content field is optional; `base_version` is required, because it is the
/// optimistic-concurrency token the write is guarded on.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNoteRequest {
    /// New title, if it changed.
    pub title: Option<String>,
    /// New body, if it changed.
    pub content: Option<String>,
    /// New pinned flag, if it changed.
    pub pinned: Option<bool>,
    /// New folder, if it changed. Absent clears the folder — the column is written
    /// unconditionally, as `master` did.
    pub folder_id: Option<Uuid>,
    /// The version the client last saw. Required.
    pub base_version: i32,
}

impl Validatable for UpdateNoteRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();
        validate_title(self.title.as_deref(), &mut fields);
        validate_content(self.content.as_deref(), &mut fields);
        if self.title.is_none()
            && self.content.is_none()
            && self.pinned.is_none()
            && self.folder_id.is_none()
        {
            fields.push("body", "A patch must change at least one field");
        }
        fields.into_result()
    }
}

/// `DELETE /notes/{id}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteNoteRequest {
    /// The version the client last saw. Required.
    pub base_version: i32,
}

/// `POST /folders`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFolderRequest {
    /// The client-chosen id.
    pub id: Uuid,
    /// Title, up to [`MAX_TITLE_CHARS`] characters.
    pub title: String,
    /// Whether the folder is pinned.
    #[serde(default)]
    pub pinned: bool,
}

impl Validatable for CreateFolderRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();
        validate_title(Some(&self.title), &mut fields);
        fields.into_result()
    }
}

/// `PATCH /folders/{id}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFolderRequest {
    /// New title, if it changed.
    pub title: Option<String>,
    /// New pinned flag, if it changed.
    pub pinned: Option<bool>,
    /// The version the client last saw. Required.
    pub base_version: i32,
}

impl Validatable for UpdateFolderRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();
        validate_title(self.title.as_deref(), &mut fields);
        if self.title.is_none() && self.pinned.is_none() {
            fields.push("body", "A patch must change at least one field");
        }
        fields.into_result()
    }
}

/// `DELETE /folders/{id}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFolderRequest {
    /// The version the client last saw. Required.
    pub base_version: i32,
    /// Whether the folder's notes should be deleted with it.
    ///
    /// `false` (the default, the safe branch) unfiles them instead. `master` parsed this
    /// field and then ignored it, leaving every note pointing at a soft-deleted folder.
    #[serde(default)]
    pub should_delete_notes: bool,
}

/// `PUT /notes` — move a batch of notes into a folder (or out of one).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveNotesToFolderRequest {
    /// The notes to move, 1–[`MAX_NOTES_PER_MOVE`] of them.
    pub note_ids: Vec<Uuid>,
    /// The destination, or `None` to unfile.
    pub folder_id: Option<Uuid>,
}

impl Validatable for MoveNotesToFolderRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();
        if self.note_ids.is_empty() {
            fields.push("noteIds", "Provide at least one note id");
        } else if self.note_ids.len() > MAX_NOTES_PER_MOVE {
            fields.push(
                "noteIds",
                format!("Provide between 1 and {MAX_NOTES_PER_MOVE} note ids"),
            );
        }
        fields.into_result()
    }
}

/// `GET /notes` — the filter set, already at the boundary.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesQuery {
    /// Only notes in this folder.
    pub folder_id: Option<Uuid>,
    /// Only notes with this pinned flag.
    pub pinned: Option<bool>,
    /// Full-text keywords.
    pub keywords: Option<String>,
    /// The embedded `?cursor=` / `?limit=`.
    #[serde(flatten)]
    pub pagination: meno_core::CursorParams,
}

impl NotesQuery {
    /// Validated page size, clamped to [1, 100].
    #[must_use]
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }

    /// `limit + 1` — the probe row.
    #[must_use]
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }

    /// The opaque cursor from the previous page.
    #[must_use]
    pub fn cursor(&self) -> Option<&meno_core::Cursor> {
        self.pagination.cursor.as_ref()
    }
}

/// `GET /folders` — the filter set.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldersQuery {
    /// Only folders with this pinned flag.
    pub pinned: Option<bool>,
    /// Full-text keywords.
    pub keywords: Option<String>,
    /// The embedded `?cursor=` / `?limit=`.
    #[serde(flatten)]
    pub pagination: meno_core::CursorParams,
}

impl FoldersQuery {
    /// Validated page size, clamped to [1, 100].
    #[must_use]
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }

    /// `limit + 1` — the probe row.
    #[must_use]
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }

    /// The opaque cursor from the previous page.
    #[must_use]
    pub fn cursor(&self) -> Option<&meno_core::Cursor> {
        self.pagination.cursor.as_ref()
    }
}

/// `GET /notes/sync` — where to resume each stream.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesSyncQuery {
    /// The notes cursor from the previous pull.
    pub notes_cursor: Option<meno_core::Cursor>,
    /// The folders cursor from the previous pull.
    pub folders_cursor: Option<meno_core::Cursor>,
    /// Per-stream page size. Clamped to [1, 500]; defaults to 200.
    pub limit: Option<i64>,
}

impl NotesSyncQuery {
    /// Validated per-stream page size.
    #[must_use]
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(200).clamp(1, 500)
    }
}

/// `POST /notes/sync` — the client's queued mutations.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesSyncPushRequest {
    /// The mutations, in the client's queue order.
    pub mutations: Vec<SyncMutation>,
}

impl Validatable for NotesSyncPushRequest {
    fn validate(&self) -> Result<(), MenoError> {
        if self.mutations.len() > super::model::MAX_MUTATIONS_PER_PUSH {
            return Err(error::too_many_mutations(
                super::model::MAX_MUTATIONS_PER_PUSH,
            ));
        }
        Ok(())
    }
}

/// A note as the wire carries it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDto {
    /// The note id.
    pub id: Uuid,
    /// Title.
    pub title: String,
    /// Body.
    pub content: String,
    /// Whether it is pinned.
    pub pinned: bool,
    /// Its folder, if any.
    pub folder_id: Option<Uuid>,
    /// Optimistic-concurrency counter.
    pub version: i32,
    /// When it was created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When it was last written.
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    /// Whether it is soft-deleted.
    pub deleted: bool,
}

impl From<Note> for NoteDto {
    fn from(note: Note) -> Self {
        // Read the derived flag before the fields are moved out.
        let deleted = note.is_deleted();
        Self {
            id: note.id,
            title: note.title,
            content: note.content,
            pinned: note.pinned,
            folder_id: note.folder_id,
            version: note.version,
            created_at: note.created_at,
            updated_at: note.updated_at,
            deleted,
        }
    }
}

/// A folder as the wire carries it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    /// The folder id.
    pub id: Uuid,
    /// Title.
    pub title: String,
    /// Whether it is pinned.
    pub pinned: bool,
    /// Optimistic-concurrency counter.
    pub version: i32,
    /// When it was created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When it was last written.
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    /// Whether it is soft-deleted.
    pub deleted: bool,
}

impl From<Folder> for FolderDto {
    fn from(folder: Folder) -> Self {
        // Read the derived flag before the fields are moved out.
        let deleted = folder.is_deleted();
        Self {
            id: folder.id,
            title: folder.title,
            pinned: folder.pinned,
            version: folder.version,
            created_at: folder.created_at,
            updated_at: folder.updated_at,
            deleted,
        }
    }
}

/// Either entity type, as carried by a sync result.
///
/// Untagged so the two shapes are told apart by their fields, which is what lets one
/// mutation log interleave notes and folders.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ConflictEntity {
    /// A note's current state.
    Note(NoteDto),
    /// A folder's current state.
    Folder(FolderDto),
}

impl ConflictEntity {
    /// The entity's id.
    #[must_use]
    pub fn id(&self) -> Uuid {
        match self {
            Self::Note(note) => note.id,
            Self::Folder(folder) => folder.id,
        }
    }
}

/// What happened to one mutation, as the wire reports it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MutationOutcome {
    /// Written (or correctly determined nothing needed writing).
    Applied,
    /// `base_version` was stale; `server_entity` is the current row.
    Conflict,
    /// Failed per-item validation. The rest of the batch still applied.
    Rejected,
}

/// One mutation's result, in submission order.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationResult {
    /// The entity id.
    pub id: Uuid,
    /// Which kind of entity it is.
    pub entity_type: super::model::EntityType,
    /// What happened.
    pub outcome: MutationOutcome,
    /// The server's copy, on an applied or conflicted mutation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_entity: Option<ConflictEntity>,
    /// Why a mutation was rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl MutationResult {
    /// An applied mutation carrying the row as written.
    #[must_use]
    pub fn applied(entity: ConflictEntity) -> Self {
        Self {
            id: entity.id(),
            entity_type: match &entity {
                ConflictEntity::Note(_) => super::model::EntityType::Note,
                ConflictEntity::Folder(_) => super::model::EntityType::Folder,
            },
            outcome: MutationOutcome::Applied,
            server_entity: Some(entity),
            reason: None,
        }
    }

    /// An applied mutation with no row — a create-then-delete that never persisted.
    #[must_use]
    pub fn applied_unpersisted(id: Uuid, entity_type: super::model::EntityType) -> Self {
        Self {
            id,
            entity_type,
            outcome: MutationOutcome::Applied,
            server_entity: None,
            reason: None,
        }
    }

    /// A conflict carrying the server's current row.
    #[must_use]
    pub fn conflict(entity: ConflictEntity) -> Self {
        Self {
            id: entity.id(),
            entity_type: match &entity {
                ConflictEntity::Note(_) => super::model::EntityType::Note,
                ConflictEntity::Folder(_) => super::model::EntityType::Folder,
            },
            outcome: MutationOutcome::Conflict,
            server_entity: Some(entity),
            reason: None,
        }
    }

    /// A per-item rejection.
    #[must_use]
    pub fn rejected(id: Uuid, entity_type: super::model::EntityType, reason: String) -> Self {
        Self {
            id,
            entity_type,
            outcome: MutationOutcome::Rejected,
            server_entity: None,
            reason: Some(reason),
        }
    }
}

/// `POST /notes/sync` — the results, in submission order.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesSyncPushResponse {
    /// One result per submitted mutation.
    pub results: Vec<MutationResult>,
}

/// `GET /notes/sync` — everything changed since each cursor.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesSyncResponse {
    /// Notes changed since `notesCursor`.
    pub notes: Vec<NoteDto>,
    /// Where to resume the notes stream.
    pub notes_next_cursor: Option<meno_core::Cursor>,
    /// Whether another notes page exists.
    pub notes_has_more: bool,
    /// Folders changed since `foldersCursor`.
    pub folders: Vec<FolderDto>,
    /// Where to resume the folders stream.
    pub folders_next_cursor: Option<meno_core::Cursor>,
    /// Whether another folders page exists.
    pub folders_has_more: bool,
    /// Informational only — never a cursor. The per-entity cursors are the resume points.
    #[serde(with = "time::serde::rfc3339")]
    pub server_time: OffsetDateTime,
}

/// Shared title rule: the same ceiling for notes and folders, and the same message.
fn validate_title(title: Option<&str>, fields: &mut FieldErrors) {
    if let Some(title) = title
        && title.chars().count() > MAX_TITLE_CHARS
    {
        fields.push(
            "title",
            format!("Title cannot exceed {MAX_TITLE_CHARS} characters"),
        );
    }
}

/// Shared body rule.
fn validate_content(content: Option<&str>, fields: &mut FieldErrors) {
    if let Some(content) = content
        && content.chars().count() > MAX_CONTENT_CHARS
    {
        fields.push(
            "content",
            format!("Content cannot exceed {MAX_CONTENT_CHARS} characters"),
        );
    }
}

#[cfg(test)]
mod tests {
    //! The wire shape and the boundary rules.

    use super::*;
    use meno_core::ErrorCode;

    fn create_note() -> CreateNoteRequest {
        CreateNoteRequest {
            id: Uuid::from_u128(1),
            title: "Groceries".to_owned(),
            content: "milk".to_owned(),
            pinned: false,
            folder_id: None,
        }
    }

    #[test]
    fn a_well_formed_note_passes() {
        assert!(create_note().validate().is_ok());
    }

    #[test]
    fn both_bad_fields_are_reported_in_one_pass() {
        let error = CreateNoteRequest {
            title: "t".repeat(MAX_TITLE_CHARS + 1),
            content: "c".repeat(MAX_CONTENT_CHARS + 1),
            ..create_note()
        }
        .validate()
        .expect_err("refused");

        let MenoError::Validation { fields } = error else {
            panic!("expected a validation failure");
        };
        assert!(fields.contains_key("title"), "{fields:?}");
        assert!(fields.contains_key("content"), "{fields:?}");
    }

    #[test]
    fn an_empty_patch_is_refused_for_both_verbs() {
        let note = UpdateNoteRequest {
            title: None,
            content: None,
            pinned: None,
            folder_id: None,
            base_version: 1,
        };
        assert!(note.validate().is_err());

        let folder = UpdateFolderRequest {
            title: None,
            pinned: None,
            base_version: 1,
        };
        assert!(folder.validate().is_err());
    }

    #[test]
    fn a_patch_carrying_one_field_is_accepted() {
        let note = UpdateNoteRequest {
            title: Some("A new title".to_owned()),
            content: None,
            pinned: None,
            folder_id: None,
            base_version: 1,
        };
        assert!(note.validate().is_ok());
    }

    #[test]
    fn a_move_must_address_at_least_one_and_at_most_the_ceiling() {
        let empty = MoveNotesToFolderRequest {
            note_ids: vec![],
            folder_id: None,
        };
        assert!(empty.validate().is_err());

        let too_many = MoveNotesToFolderRequest {
            note_ids: vec![Uuid::from_u128(1); MAX_NOTES_PER_MOVE + 1],
            folder_id: None,
        };
        assert!(too_many.validate().is_err());

        let ok = MoveNotesToFolderRequest {
            note_ids: vec![Uuid::from_u128(1)],
            folder_id: Some(Uuid::from_u128(2)),
        };
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn an_oversized_push_is_a_bad_request_at_the_boundary() {
        let mutations = (0..=super::super::model::MAX_MUTATIONS_PER_PUSH)
            .map(|i| {
                SyncMutation::Note(super::super::model::NoteMutation {
                    id: Uuid::from_u128(i as u128),
                    title: "t".to_owned(),
                    content: "c".to_owned(),
                    pinned: false,
                    folder_id: None,
                    deleted: false,
                    base_version: None,
                    client_updated_at: OffsetDateTime::UNIX_EPOCH,
                })
            })
            .collect();

        let error = NotesSyncPushRequest { mutations }
            .validate()
            .expect_err("refused");
        assert_eq!(error.code(), ErrorCode::BadRequest);
    }

    #[test]
    fn a_query_reads_camel_case_and_clamps_its_limit() {
        let query: NotesQuery = serde_json::from_str(
            r#"{"folderId":"11111111-1111-1111-1111-111111111111","limit":"5"}"#,
        )
        .expect("decodes");

        assert_eq!(query.limit(), 5);
        assert_eq!(
            query.folder_id,
            Some(Uuid::from_u128(0x1111_1111_1111_1111_1111_1111_1111_1111))
        );

        let absurd: NotesQuery = serde_json::from_str(r#"{"limit":"10000"}"#).expect("decodes");
        assert_eq!(absurd.limit(), 100, "clamped like every other list");
    }

    #[test]
    fn a_note_response_uses_camel_case_and_rfc3339() {
        let note = Note {
            id: Uuid::from_u128(1),
            title: "T".to_owned(),
            content: "C".to_owned(),
            pinned: true,
            folder_id: Some(Uuid::from_u128(2)),
            creator_id: Uuid::from_u128(3),
            version: 4,
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("instant"),
            updated_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("instant"),
            deleted_at: None,
        };

        let json = serde_json::to_value(NoteDto::from(note)).expect("serialises");

        assert!(json.get("folderId").is_some(), "{json}");
        assert!(!json.to_string().contains("folder_id"), "{json}");
        assert!(json["createdAt"].is_string(), "{json}");
        assert_eq!(json["deleted"], serde_json::json!(false));
    }

    #[test]
    fn a_version_conflict_result_carries_the_server_copy() {
        let note = Note {
            id: Uuid::from_u128(1),
            title: "server copy".to_owned(),
            content: String::new(),
            pinned: false,
            folder_id: None,
            creator_id: Uuid::from_u128(3),
            version: 2,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            deleted_at: None,
        };

        let result = MutationResult::conflict(ConflictEntity::Note(note.into()));
        let json = serde_json::to_value(&result).expect("serialises");

        assert_eq!(json["outcome"], "CONFLICT");
        assert_eq!(json["entityType"], "note");
        assert_eq!(json["serverEntity"]["title"], "server copy");
        assert!(json.get("reason").is_none(), "omitted, not null: {json}");
    }

    #[test]
    fn a_rejected_result_carries_a_reason_and_no_entity() {
        let result = MutationResult::rejected(
            Uuid::from_u128(1),
            super::super::model::EntityType::Note,
            "title exceeds 100 characters".to_owned(),
        );
        let json = serde_json::to_value(&result).expect("serialises");

        assert_eq!(json["outcome"], "REJECTED");
        assert_eq!(json["reason"], "title exceeds 100 characters");
        assert!(json.get("serverEntity").is_none(), "{json}");
    }
}
