//! The notes rules, as orchestration (§5.1's `service` row).
//!
//! # What changed from `master`
//!
//! - **It holds a trait object, not a generic pool-holder.** `master`'s
//!   `NotesService<R: NotesRepo>` also carried the `PgPool` so it could open the batch
//!   transaction itself. The transaction moved into
//!   [`NotesRepo::apply_sync_batch`], so
//!   the service holds one `Arc<dyn NotesRepo>` and the whole suite can run against the
//!   in-memory double.
//! - **Validation is at the boundary.** `dto::Validatable` runs in the handler; what is
//!   here are only the rules that need state — is this the caller's note, what happened
//!   to a guarded write, how a batch resolves.
//! - **A version conflict is a shared taxonomy entry.** `master` attached the server's
//!   copy to the error; the shared taxonomy has no room for it, and the sync path carries
//!   it per-item instead.

use std::collections::HashMap;
use std::sync::Arc;

use time::OffsetDateTime;
use uuid::Uuid;

use super::dto::{
    ConflictEntity, CreateFolderRequest, DeleteFolderRequest, FolderDto, FoldersQuery,
    MoveNotesToFolderRequest, MutationResult, NoteDto, NotesQuery, NotesSyncPushRequest,
    NotesSyncPushResponse, NotesSyncQuery, NotesSyncResponse, UpdateFolderRequest,
    UpdateNoteRequest,
};
use super::error;
use super::model::{EntityType, MAX_MUTATIONS_PER_PUSH, MutationResolution, SyncEntity};
use super::repository::{
    CreateFolderInput, CreateNoteInput, FolderFilter, NoteFilter, NotesOnFolderDelete, NotesRepo,
    UpdateFolderInput, UpdateNoteInput,
};
use meno_core::{Cursor, CursorPage, Error as MenoError};

/// The notes module's rules.
#[derive(Clone)]
pub struct NotesService {
    repo: Arc<dyn NotesRepo>,
}

impl std::fmt::Debug for NotesService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotesService").finish_non_exhaustive()
    }
}

impl NotesService {
    /// Build from its storage. `state.rs` supplies the concrete adapter.
    #[must_use]
    pub fn new(repo: Arc<dyn NotesRepo>) -> Self {
        Self { repo }
    }

    /// Create a note.
    ///
    /// # Errors
    ///
    /// A repository failure. The id is client-chosen, so a duplicate is a driver
    /// conflict rendered as a `500`.
    pub async fn create_note(
        &self,
        creator_id: Uuid,
        request: &super::dto::CreateNoteRequest,
    ) -> Result<NoteDto, MenoError> {
        let input = CreateNoteInput {
            id: request.id,
            title: &request.title,
            content: &request.content,
            pinned: request.pinned,
            folder_id: request.folder_id,
            creator_id,
        };
        let note = self.repo.create_note(&input).await?;
        Ok(NoteDto::from(note))
    }

    /// Patch a note, guarded on `base_version`.
    ///
    /// # Errors
    ///
    /// `404` when no live note carries the id; `409 VERSION_CONFLICT` when it moved on.
    pub async fn update_note(
        &self,
        id: Uuid,
        creator_id: Uuid,
        request: &UpdateNoteRequest,
    ) -> Result<NoteDto, MenoError> {
        let input = UpdateNoteInput {
            title: request.title.as_deref(),
            content: request.content.as_deref(),
            pinned: request.pinned,
            folder_id: request.folder_id,
            note_id: id,
            creator_id,
            base_version: request.base_version,
        };

        match self.repo.update_note_by_version(&input).await? {
            Some(note) => Ok(NoteDto::from(note)),
            None => Err(self.explain_note_miss(id, creator_id).await?),
        }
    }

    /// Soft-delete a note, guarded on `base_version`.
    ///
    /// # Errors
    ///
    /// `404` when no live note carries the id; `409 VERSION_CONFLICT` when it moved on.
    pub async fn delete_note(
        &self,
        id: Uuid,
        creator_id: Uuid,
        base_version: i32,
    ) -> Result<NoteDto, MenoError> {
        match self
            .repo
            .soft_delete_note(id, creator_id, base_version)
            .await?
        {
            Some(note) => Ok(NoteDto::from(note)),
            None => Err(self.explain_note_miss(id, creator_id).await?),
        }
    }

    /// One page of notes.
    ///
    /// # Errors
    ///
    /// `400 INVALID_CURSOR` for a `?cursor=` that cannot be decoded, or a repository
    /// failure.
    pub async fn get_notes(
        &self,
        creator_id: Uuid,
        query: &NotesQuery,
    ) -> Result<CursorPage<NoteDto>, MenoError> {
        let limit = query.limit();
        let filter = NoteFilter {
            creator_id,
            folder_id: query.folder_id,
            pinned: query.pinned,
            keywords: query.keywords.clone(),
            limit_plus_one: query.limit_plus_one(),
            cursor: decode_cursor(query.cursor())?,
        };

        let rows = self.repo.find_notes(&filter).await?;
        let notes: Vec<NoteDto> = rows.into_iter().map(NoteDto::from).collect();

        Ok(CursorPage::from_rows(notes, limit, |note| {
            Cursor::from_timestamp_id(note.created_at, note.id)
        }))
    }

    /// Create a folder.
    ///
    /// # Errors
    ///
    /// A repository failure.
    pub async fn create_folder(
        &self,
        creator_id: Uuid,
        request: &CreateFolderRequest,
    ) -> Result<FolderDto, MenoError> {
        let input = CreateFolderInput {
            id: request.id,
            title: &request.title,
            pinned: request.pinned,
            creator_id,
        };
        let folder = self.repo.create_folder(&input).await?;
        Ok(FolderDto::from(folder))
    }

    /// Patch a folder, guarded on `base_version`.
    ///
    /// # Errors
    ///
    /// `404` when no live folder carries the id; `409 VERSION_CONFLICT` when it moved on.
    pub async fn update_folder(
        &self,
        id: Uuid,
        creator_id: Uuid,
        request: &UpdateFolderRequest,
    ) -> Result<FolderDto, MenoError> {
        let input = UpdateFolderInput {
            title: request.title.as_deref(),
            pinned: request.pinned,
            folder_id: id,
            creator_id,
            base_version: request.base_version,
        };

        match self.repo.update_folder_by_version(&input).await? {
            Some(folder) => Ok(FolderDto::from(folder)),
            None => Err(self.explain_folder_miss(id, creator_id).await?),
        }
    }

    /// Soft-delete a folder and either delete or unfile its notes.
    ///
    /// # Errors
    ///
    /// `404` when no live folder carries the id; `409 VERSION_CONFLICT` when it moved on.
    pub async fn delete_folder(
        &self,
        id: Uuid,
        creator_id: Uuid,
        request: &DeleteFolderRequest,
    ) -> Result<FolderDto, MenoError> {
        // `false` is the safe branch: `master` parsed this field and then ignored it,
        // leaving every note pointing at a soft-deleted folder.
        let on_delete = if request.should_delete_notes {
            NotesOnFolderDelete::Delete
        } else {
            NotesOnFolderDelete::Orphan
        };

        match self
            .repo
            .delete_folder(id, creator_id, request.base_version, on_delete)
            .await?
        {
            Some(folder) => Ok(FolderDto::from(folder)),
            None => Err(self.explain_folder_miss(id, creator_id).await?),
        }
    }

    /// One page of folders.
    ///
    /// # Errors
    ///
    /// `400 INVALID_CURSOR` for a bad cursor, or a repository failure.
    pub async fn get_folders(
        &self,
        creator_id: Uuid,
        query: &FoldersQuery,
    ) -> Result<CursorPage<FolderDto>, MenoError> {
        let limit = query.limit();
        let filter = FolderFilter {
            creator_id,
            pinned: query.pinned,
            keywords: query.keywords.clone(),
            limit_plus_one: query.limit_plus_one(),
            cursor: decode_cursor(query.cursor())?,
        };

        let rows = self.repo.find_folders(&filter).await?;
        let folders: Vec<FolderDto> = rows.into_iter().map(FolderDto::from).collect();

        Ok(CursorPage::from_rows(folders, limit, |folder| {
            Cursor::from_timestamp_id(folder.created_at, folder.id)
        }))
    }

    /// File a note into a folder.
    ///
    /// Both lookups are owner-scoped, so ownership cannot be forged by passing someone
    /// else's ids — the reads simply find nothing.
    ///
    /// # Errors
    ///
    /// `404` when the note or the folder does not exist for this caller.
    pub async fn add_note_to_folder(
        &self,
        creator_id: Uuid,
        note_id: Uuid,
        folder_id: Uuid,
    ) -> Result<NoteDto, MenoError> {
        // Two independent reads, one round trip's worth of latency.
        let (note_result, folder_result) = tokio::join!(
            self.repo.find_note_by_id(note_id, creator_id),
            self.repo.find_folder_by_id(folder_id, creator_id),
        );

        let note = note_result?.ok_or_else(error::note_not_found)?;
        folder_result?.ok_or_else(error::folder_not_found)?;

        // Already there: answer the current row rather than a redundant version bump.
        if note.folder_id == Some(folder_id) {
            return Ok(NoteDto::from(note));
        }

        match self
            .repo
            .add_note_to_folder(creator_id, note_id, folder_id)
            .await?
        {
            Some(updated) => Ok(NoteDto::from(updated)),
            // Deleted between the read and the write.
            None => Err(error::note_not_found()),
        }
    }

    /// Unfile a note from a folder.
    ///
    /// # Errors
    ///
    /// `404` when the note is not in that folder for this caller.
    pub async fn remove_note_from_folder(
        &self,
        creator_id: Uuid,
        note_id: Uuid,
        folder_id: Uuid,
    ) -> Result<NoteDto, MenoError> {
        self.repo
            .remove_note_from_folder(creator_id, note_id, folder_id)
            .await?
            .map(NoteDto::from)
            .ok_or_else(error::note_not_found)
    }

    /// Move a batch of notes to a folder (or out of one).
    ///
    /// # Errors
    ///
    /// `404` when the destination folder does not exist for this caller.
    pub async fn move_notes_to_folder(
        &self,
        creator_id: Uuid,
        request: &MoveNotesToFolderRequest,
    ) -> Result<Vec<NoteDto>, MenoError> {
        if request.note_ids.is_empty() {
            return Ok(vec![]);
        }

        if let Some(folder_id) = request.folder_id {
            self.repo
                .find_folder_by_id(folder_id, creator_id)
                .await?
                .ok_or_else(error::folder_not_found)?;
        }

        let notes = self
            .repo
            .move_notes_to_folder(creator_id, request.folder_id, &request.note_ids)
            .await?;
        Ok(notes.into_iter().map(NoteDto::from).collect())
    }

    /// Everything changed since the client's cursors.
    ///
    /// # Errors
    ///
    /// `400 INVALID_CURSOR` for a cursor that cannot be decoded.
    pub async fn sync_pull(
        &self,
        creator_id: Uuid,
        query: &NotesSyncQuery,
    ) -> Result<NotesSyncResponse, MenoError> {
        let limit = query.limit();
        let server_time = OffsetDateTime::now_utc();

        let notes_cursor = decode_cursor(query.notes_cursor.as_ref())?;
        let folders_cursor = decode_cursor(query.folders_cursor.as_ref())?;

        let (notes_result, folders_result) = tokio::join!(
            self.repo
                .find_notes_changed_since(creator_id, notes_cursor, limit + 1),
            self.repo
                .find_folders_changed_since(creator_id, folders_cursor, limit + 1),
        );
        let mut notes = notes_result?;
        let mut folders = folders_result?;

        let notes_has_more = notes.len() > limit as usize;
        notes.truncate(limit as usize);

        let folders_has_more = folders.len() > limit as usize;
        folders.truncate(limit as usize);

        // When nothing changed, echo the client's own cursor back rather than returning
        // `None`, so a client never needs a special case for \"caught up, nothing new\".
        let notes_next_cursor = notes
            .last()
            .map(|note| Cursor::from_timestamp_id(note.updated_at, note.id))
            .or_else(|| query.notes_cursor.clone());
        let folders_next_cursor = folders
            .last()
            .map(|folder| Cursor::from_timestamp_id(folder.updated_at, folder.id))
            .or_else(|| query.folders_cursor.clone());

        Ok(NotesSyncResponse {
            notes: notes.into_iter().map(NoteDto::from).collect(),
            notes_next_cursor,
            notes_has_more,
            folders: folders.into_iter().map(FolderDto::from).collect(),
            folders_next_cursor,
            folders_has_more,
            server_time,
        })
    }

    /// Apply a client's queued mutations in one transaction.
    ///
    /// # Errors
    ///
    /// `400` when the batch exceeds [`MAX_MUTATIONS_PER_PUSH`], or a repository failure.
    pub async fn sync_push(
        &self,
        creator_id: Uuid,
        request: NotesSyncPushRequest,
    ) -> Result<NotesSyncPushResponse, MenoError> {
        if request.mutations.is_empty() {
            return Ok(NotesSyncPushResponse { results: vec![] });
        }
        if request.mutations.len() > MAX_MUTATIONS_PER_PUSH {
            return Err(error::too_many_mutations(MAX_MUTATIONS_PER_PUSH));
        }

        let batch = super::model::SyncBatch::split(request.mutations);

        // The id and type of every mutation, so a resolution that carries no row — a
        // create-then-delete that never persisted — can still be reported positionally.
        let mut identity: HashMap<usize, (Uuid, EntityType)> = HashMap::with_capacity(batch.len());
        for (index, mutation) in &batch.folders {
            identity.insert(*index, (mutation.id, EntityType::Folder));
        }
        for (index, mutation) in &batch.notes {
            identity.insert(*index, (mutation.id, EntityType::Note));
        }

        let resolved = self.repo.apply_sync_batch(creator_id, &batch).await?;

        let results = resolved
            .into_iter()
            .map(|resolved| {
                let fallback = identity.get(&resolved.index).copied();
                to_mutation_result(resolved.resolution, fallback)
            })
            .collect();

        Ok(NotesSyncPushResponse { results })
    }

    /// Purge soft-deleted rows past a retention cutoff. Returns `(notes, folders)`.
    ///
    /// # Errors
    ///
    /// A repository failure.
    pub async fn purge_stale(&self, cutoff: OffsetDateTime) -> Result<(u64, u64), MenoError> {
        let notes = self.repo.purge_deleted_notes_older_than(cutoff).await?;
        let folders = self.repo.purge_deleted_folders_older_than(cutoff).await?;
        Ok((notes, folders))
    }

    /// Say *why* a guarded note write missed: gone, or moved on.
    async fn explain_note_miss(&self, id: Uuid, creator_id: Uuid) -> Result<MenoError, MenoError> {
        match self.repo.find_note_by_id(id, creator_id).await? {
            Some(_) => Ok(error::version_conflict()),
            None => Ok(error::note_not_found()),
        }
    }

    /// Say *why* a guarded folder write missed.
    async fn explain_folder_miss(
        &self,
        id: Uuid,
        creator_id: Uuid,
    ) -> Result<MenoError, MenoError> {
        match self.repo.find_folder_by_id(id, creator_id).await? {
            Some(_) => Ok(error::version_conflict()),
            None => Ok(error::folder_not_found()),
        }
    }
}

/// Decode an optional timer cursor, returning `400 INVALID_CURSOR` on failure.
fn decode_cursor(cursor: Option<&Cursor>) -> Result<Option<(OffsetDateTime, Uuid)>, MenoError> {
    cursor
        .map(Cursor::to_timestamp_id)
        .transpose()
        .map_err(MenoError::from)
}

/// Turn a repository resolution into the wire result.
fn to_mutation_result(
    resolution: MutationResolution,
    fallback: Option<(Uuid, EntityType)>,
) -> MutationResult {
    match resolution {
        MutationResolution::Applied {
            entity: Some(entity),
        } => MutationResult::applied(conflict_entity(entity)),
        MutationResolution::Applied { entity: None } => match fallback {
            Some((id, entity_type)) => MutationResult::applied_unpersisted(id, entity_type),
            // The repository returned no row and no identity — impossible for a real
            // adapter, but reported rather than panicking.
            None => MutationResult::rejected(
                Uuid::nil(),
                EntityType::Note,
                "mutation resolved without an identity".to_owned(),
            ),
        },
        MutationResolution::Conflict { entity } => {
            MutationResult::conflict(conflict_entity(entity))
        }
        MutationResolution::Rejected { reason } => match fallback {
            Some((id, entity_type)) => MutationResult::rejected(id, entity_type, reason),
            None => MutationResult::rejected(Uuid::nil(), EntityType::Note, reason),
        },
    }
}

/// Project a stored entity into its wire shape.
fn conflict_entity(entity: SyncEntity) -> ConflictEntity {
    match entity {
        SyncEntity::Note(note) => ConflictEntity::Note(NoteDto::from(note)),
        SyncEntity::Folder(folder) => ConflictEntity::Folder(FolderDto::from(folder)),
    }
}

#[cfg(test)]
mod tests {
    //! The pure helpers, asserted without a repository.

    use super::*;

    #[test]
    fn a_bad_cursor_decodes_to_the_stable_wire_code() {
        let bad = Cursor("not-a-cursor".to_owned());
        let error = decode_cursor(Some(&bad)).expect_err("must fail");
        assert_eq!(error.code(), meno_core::ErrorCode::InvalidCursor);
    }

    #[test]
    fn an_absent_cursor_decodes_to_none() {
        assert!(
            decode_cursor(None)
                .expect("no cursor is not a failure")
                .is_none()
        );
    }

    #[test]
    fn an_unpersisted_resolution_uses_the_mutations_identity() {
        let result = to_mutation_result(
            MutationResolution::Applied { entity: None },
            Some((Uuid::from_u128(9), EntityType::Note)),
        );
        let json = serde_json::to_value(&result).expect("serialises");
        assert_eq!(json["outcome"], "APPLIED");
        assert_eq!(json["id"], Uuid::from_u128(9).to_string());
    }

    #[test]
    fn a_rejection_uses_the_mutations_identity_and_reason() {
        let result = to_mutation_result(
            MutationResolution::Rejected {
                reason: "title exceeds 100 characters".to_owned(),
            },
            Some((Uuid::from_u128(9), EntityType::Folder)),
        );
        let json = serde_json::to_value(&result).expect("serialises");
        assert_eq!(json["outcome"], "REJECTED");
        assert_eq!(json["entityType"], "folder");
        assert_eq!(json["reason"], "title exceeds 100 characters");
    }
}
