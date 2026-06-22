use crate::modules::notes::dto;
use crate::modules::notes::dto::{
    ConflictEntity, CreateFolderRequest, DeleteFolderRequest, EntityType, FolderDto,
    FolderMutation, FoldersQuery, MoveNotesToFolderRequest, MutationResult, NoteMutation,
    NotesQuery, NotesSyncPushRequest, NotesSyncPushResponse, NotesSyncQuery, NotesSyncResponse,
    SyncMutation, UpdateFolderRequest,
};
use crate::modules::notes::errors::NotesError;
use crate::modules::notes::repository;
use crate::modules::notes::repository::{
    CreateFolderInput, FolderSnapshotInput, NoteSnapshotInput, UpdateFolderInput,
    UpsertFolderInput, UpsertNoteInput,
};
use crate::shared::pagination::{Cursor, CursorPage};
use dto::{CreateNoteRequest, NoteDto, UpdateNoteRequest};
use repository::{NotesRepo, NotesRepository, UpdateNoteInput};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashSet;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

const MAX_MUTATIONS_PER_PUSH: usize = 200;

pub type DynNotesService = NotesService<NotesRepository>;

#[derive(Clone)]
pub struct NotesService<R: NotesRepo = NotesRepository> {
    repo: Arc<R>,
    db: PgPool,
}
impl<R: NotesRepo> NotesService<R> {
    #[must_use]
    pub fn new(repo: Arc<R>, db: PgPool) -> Self {
        Self { repo, db }
    }

    pub async fn create_note(
        &self,
        creator_id: Uuid,
        req: &CreateNoteRequest,
    ) -> Result<NoteDto, NotesError> {
        let input = repository::CreateNoteInput {
            id: req.id,
            title: &req.title,
            content: &req.content,
            pinned: req.pinned,
            folder_id: req.folder_id,
            creator_id,
        };
        let note = self.repo.create_note(&self.db, &input).await?;
        Ok(NoteDto::from(note))
    }

    pub async fn update_note(
        &self,
        id: Uuid,
        creator_id: Uuid,
        req: &UpdateNoteRequest,
    ) -> Result<NoteDto, NotesError> {
        let input = UpdateNoteInput {
            title: req.title.as_deref(),
            content: req.content.as_deref(),
            pinned: req.pinned,
            folder_id: req.folder_id,
            base_version: req.base_version,
            note_id: id,
            creator_id,
        };
        match self.repo.update_note_by_version(&self.db, &input).await? {
            Some(note) => Ok(NoteDto::from(note)),
            None => {
                let note = self
                    .repo
                    .find_note_by_id(id, creator_id)
                    .await?
                    .ok_or(NotesError::NoteNotFound)?;
                let entity = ConflictEntity::Note(NoteDto::from(note));
                Err(NotesError::VersionConflict(entity))
            }
        }
    }

    pub async fn delete_note(
        &self,
        id: Uuid,
        creator_id: Uuid,
        base_version: i32,
    ) -> Result<NoteDto, NotesError> {
        match self
            .repo
            .soft_delete_note(&self.db, id, creator_id, base_version)
            .await?
        {
            Some(note) => Ok(NoteDto::from(note)),
            None => {
                let current = self
                    .repo
                    .find_note_by_id(id, creator_id)
                    .await?
                    .ok_or(NotesError::NoteNotFound)?;
                let conflict = ConflictEntity::Note(NoteDto::from(current));
                Err(NotesError::VersionConflict(conflict))
            }
        }
    }

    pub async fn get_notes(
        &self,
        creator_id: Uuid,
        query: &NotesQuery,
    ) -> Result<CursorPage<NoteDto>, NotesError> {
        let limit = query.limit();
        let rows = self.repo.find_notes(creator_id, query).await?;
        let notes = rows.into_iter().map(NoteDto::from).collect();
        let page = CursorPage::from_rows(notes, limit, |n| {
            Cursor::from_timestamp_id(n.created_at, n.id)
        });
        Ok(page)
    }

    pub async fn create_folder(
        &self,
        creator_id: Uuid,
        req: &CreateFolderRequest,
    ) -> Result<FolderDto, NotesError> {
        let input = CreateFolderInput {
            id: req.id,
            title: &req.title,
            pinned: req.pinned,
            creator_id,
        };
        let folder = self.repo.create_folder(&self.db, &input).await?;
        Ok(FolderDto::from(folder))
    }

    pub async fn update_folder(
        &self,
        id: Uuid,
        creator_id: Uuid,
        req: &UpdateFolderRequest,
    ) -> Result<FolderDto, NotesError> {
        let input = UpdateFolderInput {
            title: req.title.as_deref(),
            pinned: req.pinned,
            folder_id: id,
            creator_id,
            base_version: req.base_version,
        };
        match self.repo.update_folder_by_version(&self.db, &input).await? {
            Some(folder) => Ok(FolderDto::from(folder)),
            None => {
                let folder = self
                    .repo
                    .find_folder_by_id(id, creator_id)
                    .await?
                    .ok_or(NotesError::FolderNotFound)?;
                let entity = ConflictEntity::Folder(FolderDto::from(folder));
                Err(NotesError::VersionConflict(entity))
            }
        }
    }

    pub async fn delete_folder(
        &self,
        id: Uuid,
        creator_id: Uuid,
        req: &DeleteFolderRequest,
    ) -> Result<FolderDto, NotesError> {
        match self
            .repo
            .soft_delete_folder(&self.db, id, creator_id, req.base_version)
            .await?
        {
            Some(folder) => Ok(FolderDto::from(folder)),
            None => {
                let current = self
                    .repo
                    .find_folder_by_id(id, creator_id)
                    .await?
                    .ok_or(NotesError::FolderNotFound)?;
                let conflict = ConflictEntity::Folder(FolderDto::from(current));
                Err(NotesError::VersionConflict(conflict))
            }
        }
    }

    pub async fn get_folders(
        &self,
        creator_id: Uuid,
        query: &FoldersQuery,
    ) -> Result<CursorPage<FolderDto>, NotesError> {
        let limit = query.limit();
        let rows = self.repo.find_folders(creator_id, query).await?;
        let folders = rows.into_iter().map(FolderDto::from).collect();
        let page = CursorPage::from_rows(folders, limit, |f| {
            Cursor::from_timestamp_id(f.created_at, f.id)
        });
        Ok(page)
    }

    pub async fn add_note_to_folder(
        &self,
        creator_id: Uuid,
        note_id: Uuid,
        folder_id: Uuid,
    ) -> Result<NoteDto, NotesError> {
        let (note_result, folder_result) = tokio::join!(
            self.repo.find_note_by_id(note_id, creator_id),
            self.repo.find_folder_by_id(folder_id, creator_id),
        );

        let note = note_result?.ok_or(NotesError::NoteNotFound)?;
        let folder = folder_result?.ok_or(NotesError::FolderNotFound)?;

        if let Some(note_folder_id) = note.folder_id
            && note_folder_id == folder_id
        {
            return Ok(NoteDto::from(note));
        }

        if note.creator_id != creator_id {
            return Err(NotesError::NotNoteOwner);
        }

        if folder.creator_id != creator_id {
            return Err(NotesError::NotFolderOwner);
        }

        match self
            .repo
            .add_note_to_folder(&self.db, creator_id, note_id, folder_id)
            .await?
        {
            Some(n) => Ok(NoteDto::from(n)),
            None => {
                let conflict = ConflictEntity::Note(NoteDto::from(note));
                Err(NotesError::VersionConflict(conflict))
            }
        }
    }

    pub async fn remove_note_to_folder(
        &self,
        creator_id: Uuid,
        note_id: Uuid,
        folder_id: Uuid,
    ) -> Result<NoteDto, NotesError> {
        let note = self
            .repo
            .remove_note_from_folder(&self.db, creator_id, note_id, folder_id)
            .await?
            .ok_or(NotesError::NoteNotFound)?;
        Ok(NoteDto::from(note))
    }

    pub async fn move_notes_to_folder(
        &self,
        creator_id: Uuid,
        req: &MoveNotesToFolderRequest,
    ) -> Result<Vec<NoteDto>, NotesError> {
        if req.note_ids.is_empty() {
            return Ok(vec![]);
        }

        if req.note_ids.len() > 200 {
            return Err(NotesError::BadRequest(
                "Cannot move more than 200 notes at once".into(),
            ));
        }

        if let Some(folder_id) = req.folder_id {
            let _ = self
                .repo
                .find_folder_by_id(folder_id, creator_id)
                .await?
                .ok_or(NotesError::FolderNotFound)?;
        }

        let notes = self.repo.move_notes_to_folder(creator_id, req).await?;
        Ok(notes.into_iter().map(NoteDto::from).collect())
    }

    pub async fn sync_pull(
        &self,
        creator_id: Uuid,
        query: &NotesSyncQuery,
    ) -> Result<NotesSyncResponse, NotesError> {
        let limit = query.limit();
        let server_time = OffsetDateTime::now_utc();

        let notes_cursor = query
            .notes_cursor
            .as_ref()
            .map(Cursor::to_timestamp_id)
            .transpose()?;

        let folders_cursor = query
            .folders_cursor
            .as_ref()
            .map(Cursor::to_timestamp_id)
            .transpose()?;

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

        // If nothing changed, echo back the client's existing cursor rather
        // than returning None — that way the client never needs special-case
        // handling for "first page" vs. "fully caught up, nothing new".
        let notes_next_cursor = notes
            .last()
            .map(|n| Cursor::from_timestamp_id(n.updated_at, n.id))
            .or_else(|| query.notes_cursor.clone());

        let folders_has_more = folders.len() > limit as usize;
        folders.truncate(limit as usize);
        let folders_next_cursor = folders
            .last()
            .map(|f| Cursor::from_timestamp_id(f.updated_at, f.id))
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

    pub async fn sync_push(
        &self,
        creator_id: Uuid,
        req: NotesSyncPushRequest,
    ) -> Result<NotesSyncPushResponse, NotesError> {
        if req.mutations.is_empty() {
            return Ok(NotesSyncPushResponse { results: vec![] });
        }

        if req.mutations.len() > MAX_MUTATIONS_PER_PUSH {
            return Err(NotesError::BadRequest(format!(
                "Cannot push more than {MAX_MUTATIONS_PER_PUSH} mutations per request"
            )));
        }

        // Folders MUST be applied before notes: a note in this same batch may
        // reference a folder_id created in this same batch (e.g. the user made
        // a folder and filed three notes into it, all offline, all queued
        // together). Splitting by type lets us guarantee that ordering
        // regardless of how the client happened to interleave the array.
        let mut folder_mutations = Vec::new();
        let mut note_mutations = Vec::new();
        for (idx, m) in req.mutations.into_iter().enumerate() {
            match m {
                SyncMutation::Folder(fm) => folder_mutations.push((idx, fm)),
                SyncMutation::Note(nm) => note_mutations.push((idx, nm)),
            }
        }

        let mut tx: Transaction<'_, Postgres> = self.db.begin().await?;
        let mut results: Vec<(usize, MutationResult)> =
            Vec::with_capacity(folder_mutations.len() + note_mutations.len());

        for (idx, fm) in folder_mutations {
            let result = self.apply_folder_mutation(&mut tx, creator_id, fm).await?;
            results.push((idx, result));
        }

        // Ownership check: a note mutation could reference a folder_id
        // belonging to a different user (buggy client, or a tampered
        // request) — the FK only proves the folder EXISTS, never that this
        // caller owns it. Anything that fails ownership is silently unfiled
        // rather than rejecting the whole mutation; the note's content is
        // still valid and should still sync.
        let referenced_folder_ids: Vec<Uuid> = note_mutations
            .iter()
            .filter_map(|(_, m)| m.folder_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let owned: HashSet<Uuid> = if referenced_folder_ids.is_empty() {
            HashSet::new()
        } else {
            self.repo
                .owned_folder_ids(creator_id, &referenced_folder_ids)
                .await?
        };

        for (idx, mut nm) in note_mutations {
            if let Some(fid) = nm.folder_id
                && !owned.contains(&fid)
            {
                tracing::warn!(
                    creator_id = %creator_id, note_id = %nm.id, folder_id = %fid,
                    "sync_push: note referenced a folder not owned by this user — unfiling"
                );
                nm.folder_id = None;
            }
            let result = self.apply_note_mutation(&mut tx, creator_id, nm).await?;
            results.push((idx, result));
        }

        tx.commit().await?;

        // Restore original submission order so the client can zip results
        // back against its local mutation queue positionally.
        results.sort_by_key(|(idx, _)| *idx);
        let results = results.into_iter().map(|(_, r)| r).collect();

        Ok(NotesSyncPushResponse { results })
    }

    async fn apply_note_mutation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        creator_id: Uuid,
        m: NoteMutation,
    ) -> Result<MutationResult, NotesError> {
        // Cheap per-item validation — one bad mutation must not poison the
        // rest of the batch, so it's reported back as Rejected, not an Err.
        if m.title.chars().count() > 100 {
            return Ok(MutationResult::rejected(
                m.id,
                EntityType::Note,
                "title exceeds 100 characters",
            ));
        }
        if m.content.chars().count() > 10_000 {
            return Ok(MutationResult::rejected(
                m.id,
                EntityType::Note,
                "content exceeds 10,000 characters",
            ));
        }

        let snapshot = NoteSnapshotInput {
            title: &m.title,
            content: &m.content,
            pinned: m.pinned,
            folder_id: m.folder_id,
            deleted: m.deleted,
        };

        match m.base_version {
            None => {
                if m.deleted {
                    // Created, then deleted, entirely offline, before ever
                    // syncing. No other device has ever seen this id — there
                    // is nothing to tell anyone, so skip the write entirely.
                    return Ok(MutationResult::applied_unpersisted(m.id, EntityType::Note));
                }
                let input = UpsertNoteInput {
                    id: m.id,
                    creator_id,
                    snapshot: &snapshot,
                };
                match self.repo.upsert_note_if_absent(&mut **tx, &input).await? {
                    Some(note) => Ok(MutationResult::applied(
                        EntityType::Note,
                        ConflictEntity::Note(note.into()),
                    )),
                    None => {
                        // Id collision — almost always this exact mutation was
                        // already applied by an earlier attempt at this same
                        // push (client retried after a timeout that the server
                        // actually committed past). Resolve with LWW instead
                        // of treating it as a hard failure.
                        let existing = self
                            .repo
                            .find_note_by_id(m.id, creator_id)
                            .await?
                            .ok_or(NotesError::NoteNotFound)?;

                        if m.client_updated_at > existing.updated_at {
                            let note = self
                                .repo
                                .force_apply_note_snapshot(&mut **tx, m.id, creator_id, &snapshot)
                                .await?;
                            let conflict = ConflictEntity::Note(note.into());
                            Ok(MutationResult::applied(EntityType::Note, conflict))
                        } else {
                            let conflict = ConflictEntity::Note(existing.into());
                            Ok(MutationResult::conflict(EntityType::Note, conflict))
                        }
                    }
                }
            }
            Some(base_version) => {
                match self
                    .repo
                    .apply_note_snapshot_if_version_matches(
                        &mut **tx,
                        m.id,
                        creator_id,
                        base_version,
                        &snapshot,
                    )
                    .await?
                {
                    Some(note) => Ok(MutationResult::applied(
                        EntityType::Note,
                        ConflictEntity::Note(note.into()),
                    )),
                    None => {
                        let existing = self
                            .repo
                            .find_note_by_id(m.id, creator_id)
                            .await?
                            .ok_or(NotesError::NoteNotFound)?;

                        let conflict = ConflictEntity::Note(existing.into());
                        Ok(MutationResult::conflict(EntityType::Note, conflict))
                    }
                }
            }
        }
    }

    async fn apply_folder_mutation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        creator_id: Uuid,
        m: FolderMutation,
    ) -> Result<MutationResult, NotesError> {
        if m.title.chars().count() > 100 {
            return Ok(MutationResult::rejected(
                m.id,
                EntityType::Folder,
                "title exceeds 100 characters",
            ));
        }

        let snapshot = FolderSnapshotInput {
            title: &m.title,
            pinned: m.pinned,
            deleted: m.deleted,
        };

        match m.base_version {
            None => {
                if m.deleted {
                    return Ok(MutationResult::applied_unpersisted(
                        m.id,
                        EntityType::Folder,
                    ));
                }
                match self
                    .repo
                    .upsert_folder_if_absent(
                        &mut **tx,
                        &UpsertFolderInput {
                            id: m.id,
                            creator_id,
                            snapshot: &snapshot,
                        },
                    )
                    .await?
                {
                    Some(folder) => Ok(MutationResult::applied(
                        EntityType::Folder,
                        ConflictEntity::Folder(folder.into()),
                    )),
                    None => {
                        let existing = self
                            .repo
                            .find_folder_by_id(m.id, creator_id)
                            .await?
                            .ok_or(NotesError::FolderNotFound)?;
                        if m.client_updated_at > existing.updated_at {
                            let folder = self
                                .repo
                                .force_apply_folder_snapshot(&mut **tx, m.id, creator_id, &snapshot)
                                .await?;
                            let conflict = ConflictEntity::Folder(folder.into());
                            Ok(MutationResult::applied(EntityType::Folder, conflict))
                        } else {
                            let conflict = ConflictEntity::Folder(existing.into());
                            Ok(MutationResult::conflict(EntityType::Folder, conflict))
                        }
                    }
                }
            }
            Some(base_version) => {
                // IMPORTANT: deleting a folder through the sync log only ever
                // tombstones the folder row itself — it does NOT cascade to
                // orphan or delete its notes the way the dedicated
                // `DELETE /folders/:id` endpoint does. A cascading side-effect
                // buried inside a generic mutation-replay is exactly the kind
                // of implicit, hard-to-reason-about behaviour offline sync
                // should avoid. The client is expected to enqueue its own
                // explicit note mutations (unfile or delete) for every note
                // that was in the folder — it already knows locally which
                // notes those are, having computed the cascade decision on
                // the FE before ever calling sync.
                match self
                    .repo
                    .apply_folder_snapshot_if_version_matches(
                        &mut **tx,
                        m.id,
                        creator_id,
                        base_version,
                        &snapshot,
                    )
                    .await?
                {
                    Some(folder) => Ok(MutationResult::applied(
                        EntityType::Folder,
                        ConflictEntity::Folder(folder.into()),
                    )),
                    None => {
                        let existing = self
                            .repo
                            .find_folder_by_id(m.id, creator_id)
                            .await?
                            .ok_or(NotesError::FolderNotFound)?;
                        let conflict = ConflictEntity::Folder(existing.into());
                        Ok(MutationResult::conflict(EntityType::Folder, conflict))
                    }
                }
            }
        }
    }

    pub async fn purge_stale(&self, cutoff: OffsetDateTime) -> Result<(u64, u64), NotesError> {
        let notes_deleted = self.repo.purge_deleted_notes_older_than(cutoff).await?;
        let folders_deleted = self.repo.purge_deleted_folders_older_than(cutoff).await?;
        Ok((notes_deleted, folders_deleted))
    }
}
