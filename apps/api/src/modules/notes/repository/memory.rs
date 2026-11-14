//! The in-memory double of [`NotesRepo`] (§10).
//!
//! The point of this file is that it is *not* more permissive than the SQL. Version
//! guards match `WHERE version = $n`, a soft delete removes a row from every read exactly
//! as `deleted_at IS NULL` does, `updated_at` moves on every write the way the migration's
//! trigger moves it, and a create collision resolves through the same last-write-wins
//! rule the Postgres adapter implements. A test that passes here describes something the
//! statements really do.
//!
//! # The one approximation
//!
//! Full-text search is approximated by a case-insensitive substring test on the title and
//! body. The statements use `to_tsvector`/`plainto_tsquery`, which stems words; the double
//! deliberately says so here rather than pretending to match it, and no test asserts on a
//! stemmed match.

use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard};

use time::OffsetDateTime;
use uuid::Uuid;

use super::{
    CreateFolderInput, CreateNoteInput, FolderFilter, NoteFilter, NotesOnFolderDelete, NotesRepo,
    UpdateFolderInput, UpdateNoteInput,
};
use crate::modules::notes::error;
use crate::modules::notes::model::{
    Folder, FolderMutation, MAX_CONTENT_CHARS, MAX_TITLE_CHARS, MutationResolution, Note,
    NoteMutation, ResolvedMutation, SyncBatch, SyncEntity,
};
use meno_core::Error as MenoError;

/// Everything the double remembers, behind one lock.
#[derive(Debug, Default)]
struct State {
    /// Every note ever written, deleted ones included — as the table has them.
    notes: Vec<Note>,
    /// Every folder ever written.
    folders: Vec<Folder>,
}

/// The double the notes suites run against.
#[derive(Debug, Default)]
pub struct InMemoryNotesRepo {
    state: Mutex<State>,
}

impl InMemoryNotesRepo {
    /// An empty repository.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Write a note with a caller-chosen timestamp, bypassing every guard.
    ///
    /// A fixture, not behaviour: it exists so feeds can be paginated against known
    /// instants without a sleep.
    pub fn seed_note(&self, note: Note) {
        self.lock().notes.push(note);
    }

    /// Write a folder with a caller-chosen timestamp, bypassing every guard.
    pub fn seed_folder(&self, folder: Folder) {
        self.lock().folders.push(folder);
    }

    /// A note by id, deleted or not — for assertions that do not go through the trait.
    #[must_use]
    pub fn note(&self, id: Uuid) -> Option<Note> {
        self.lock().notes.iter().find(|n| n.id == id).cloned()
    }

    /// A folder by id, deleted or not.
    #[must_use]
    pub fn folder(&self, id: Uuid) -> Option<Folder> {
        self.lock().folders.iter().find(|f| f.id == id).cloned()
    }

    /// How many notes the double holds, deleted ones included.
    #[must_use]
    pub fn stored_note_count(&self) -> usize {
        self.lock().notes.len()
    }

    /// How many folders the double holds.
    #[must_use]
    pub fn stored_folder_count(&self) -> usize {
        self.lock().folders.len()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Whether `haystack` contains `keyword`, case-insensitively.
///
/// The double's stand-in for `plainto_tsquery`; see the module note.
fn matches(haystack: &str, keyword: &str) -> bool {
    haystack.to_lowercase().contains(&keyword.to_lowercase())
}

/// Stamp a note's `updated_at` and bump its version, the way the trigger plus the
/// statement do.
fn touch(note: &mut Note) {
    note.version += 1;
    note.updated_at = OffsetDateTime::now_utc();
}

/// Stamp a folder's `updated_at` and bump its version.
fn touch_folder(folder: &mut Folder) {
    folder.version += 1;
    folder.updated_at = OffsetDateTime::now_utc();
}

/// The current state of one mutation, resolved against the double's rows.
fn resolve_note(
    state: &mut State,
    creator_id: Uuid,
    mutation: &NoteMutation,
    folder_id: Option<Uuid>,
) -> MutationResolution {
    if mutation.title.chars().count() > MAX_TITLE_CHARS {
        return MutationResolution::Rejected {
            reason: format!("title exceeds {MAX_TITLE_CHARS} characters"),
        };
    }
    if mutation.content.chars().count() > MAX_CONTENT_CHARS {
        return MutationResolution::Rejected {
            reason: format!("content exceeds {MAX_CONTENT_CHARS} characters"),
        };
    }

    match mutation.base_version {
        None => {
            if mutation.deleted {
                return MutationResolution::Applied { entity: None };
            }
            let occupied = state.notes.iter().any(|n| n.id == mutation.id);
            if !occupied {
                let now = OffsetDateTime::now_utc();
                let note = Note {
                    id: mutation.id,
                    title: mutation.title.clone(),
                    content: mutation.content.clone(),
                    pinned: mutation.pinned,
                    folder_id,
                    creator_id,
                    version: 1,
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                };
                state.notes.push(note.clone());
                return MutationResolution::Applied {
                    entity: Some(SyncEntity::Note(note)),
                };
            }
            resolve_existing_note(state, creator_id, mutation, folder_id)
        }
        Some(base_version) => {
            let matched = state.notes.iter().position(|n| {
                n.id == mutation.id && n.creator_id == creator_id && n.version == base_version
            });
            match matched {
                Some(at) => {
                    apply_note_snapshot(&mut state.notes[at], mutation, folder_id);
                    MutationResolution::Applied {
                        entity: Some(SyncEntity::Note(state.notes[at].clone())),
                    }
                }
                None => resolve_existing_note(state, creator_id, mutation, folder_id),
            }
        }
    }
}

/// Resolve a note whose guarded write wrote nothing.
fn resolve_existing_note(
    state: &mut State,
    creator_id: Uuid,
    mutation: &NoteMutation,
    folder_id: Option<Uuid>,
) -> MutationResolution {
    let Some(at) = state
        .notes
        .iter()
        .position(|n| n.id == mutation.id && n.creator_id == creator_id)
    else {
        return MutationResolution::Rejected {
            reason: "note does not exist".to_owned(),
        };
    };

    let existing = state.notes[at].clone();
    if mutation.base_version.is_none() && mutation.client_updated_at > existing.updated_at {
        apply_note_snapshot(&mut state.notes[at], mutation, folder_id);
        return MutationResolution::Applied {
            entity: Some(SyncEntity::Note(state.notes[at].clone())),
        };
    }

    MutationResolution::Conflict {
        entity: SyncEntity::Note(existing),
    }
}

/// Write a full snapshot onto a note, if this creator owns it.
fn apply_note_snapshot(note: &mut Note, mutation: &NoteMutation, folder_id: Option<Uuid>) {
    note.title = mutation.title.clone();
    note.content = mutation.content.clone();
    note.pinned = mutation.pinned;
    note.folder_id = folder_id;
    if mutation.deleted && note.deleted_at.is_none() {
        note.deleted_at = Some(OffsetDateTime::now_utc());
    }
    touch(note);
}

/// Resolve one folder mutation.
fn resolve_folder(
    state: &mut State,
    creator_id: Uuid,
    mutation: &FolderMutation,
) -> MutationResolution {
    if mutation.title.chars().count() > MAX_TITLE_CHARS {
        return MutationResolution::Rejected {
            reason: format!("title exceeds {MAX_TITLE_CHARS} characters"),
        };
    }

    match mutation.base_version {
        None => {
            if mutation.deleted {
                return MutationResolution::Applied { entity: None };
            }
            let occupied = state.folders.iter().any(|f| f.id == mutation.id);
            if !occupied {
                let now = OffsetDateTime::now_utc();
                let folder = Folder {
                    id: mutation.id,
                    title: mutation.title.clone(),
                    pinned: mutation.pinned,
                    creator_id,
                    version: 1,
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                };
                state.folders.push(folder.clone());
                return MutationResolution::Applied {
                    entity: Some(SyncEntity::Folder(folder)),
                };
            }
            resolve_existing_folder(state, creator_id, mutation)
        }
        Some(base_version) => {
            let matched = state.folders.iter().position(|f| {
                f.id == mutation.id && f.creator_id == creator_id && f.version == base_version
            });
            match matched {
                Some(at) => {
                    apply_folder_snapshot(&mut state.folders[at], mutation);
                    MutationResolution::Applied {
                        entity: Some(SyncEntity::Folder(state.folders[at].clone())),
                    }
                }
                None => resolve_existing_folder(state, creator_id, mutation),
            }
        }
    }
}

/// Resolve a folder whose guarded write wrote nothing.
fn resolve_existing_folder(
    state: &mut State,
    creator_id: Uuid,
    mutation: &FolderMutation,
) -> MutationResolution {
    let Some(at) = state
        .folders
        .iter()
        .position(|f| f.id == mutation.id && f.creator_id == creator_id)
    else {
        return MutationResolution::Rejected {
            reason: "folder does not exist".to_owned(),
        };
    };

    let existing = state.folders[at].clone();
    if mutation.base_version.is_none() && mutation.client_updated_at > existing.updated_at {
        apply_folder_snapshot(&mut state.folders[at], mutation);
        return MutationResolution::Applied {
            entity: Some(SyncEntity::Folder(state.folders[at].clone())),
        };
    }

    MutationResolution::Conflict {
        entity: SyncEntity::Folder(existing),
    }
}

/// Write a full snapshot onto a folder.
fn apply_folder_snapshot(folder: &mut Folder, mutation: &FolderMutation) {
    folder.title = mutation.title.clone();
    folder.pinned = mutation.pinned;
    if mutation.deleted && folder.deleted_at.is_none() {
        folder.deleted_at = Some(OffsetDateTime::now_utc());
    }
    touch_folder(folder);
}

#[async_trait::async_trait]
impl NotesRepo for InMemoryNotesRepo {
    async fn create_note(&self, input: &CreateNoteInput<'_>) -> Result<Note, MenoError> {
        let mut state = self.lock();
        if state.notes.iter().any(|n| n.id == input.id) {
            return Err(error::internal("create_note", "a note with that id exists"));
        }
        let now = OffsetDateTime::now_utc();
        let note = Note {
            id: input.id,
            title: input.title.to_owned(),
            content: input.content.to_owned(),
            pinned: input.pinned,
            folder_id: input.folder_id,
            creator_id: input.creator_id,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        state.notes.push(note.clone());
        Ok(note)
    }

    async fn update_note_by_version(
        &self,
        input: &UpdateNoteInput<'_>,
    ) -> Result<Option<Note>, MenoError> {
        let mut state = self.lock();
        let Some(note) = state.notes.iter_mut().find(|n| {
            n.id == input.note_id
                && n.creator_id == input.creator_id
                && n.version == input.base_version
                && n.deleted_at.is_none()
        }) else {
            return Ok(None);
        };

        if let Some(title) = input.title {
            note.title = title.to_owned();
        }
        if let Some(content) = input.content {
            note.content = content.to_owned();
        }
        if let Some(pinned) = input.pinned {
            note.pinned = pinned;
        }
        // Written unconditionally, exactly as the statement's `folder_id = $4`.
        note.folder_id = input.folder_id;
        touch(note);
        Ok(Some(note.clone()))
    }

    async fn soft_delete_note(
        &self,
        id: Uuid,
        creator_id: Uuid,
        base_version: i32,
    ) -> Result<Option<Note>, MenoError> {
        let mut state = self.lock();
        let Some(note) = state.notes.iter_mut().find(|n| {
            n.id == id
                && n.creator_id == creator_id
                && n.version == base_version
                && n.deleted_at.is_none()
        }) else {
            return Ok(None);
        };
        note.deleted_at = Some(OffsetDateTime::now_utc());
        touch(note);
        Ok(Some(note.clone()))
    }

    async fn find_note_by_id(&self, id: Uuid, creator_id: Uuid) -> Result<Option<Note>, MenoError> {
        Ok(self
            .lock()
            .notes
            .iter()
            .find(|n| n.id == id && n.creator_id == creator_id && n.deleted_at.is_none())
            .cloned())
    }

    async fn find_notes(&self, filter: &NoteFilter) -> Result<Vec<Note>, MenoError> {
        let state = self.lock();
        let keywords = filter
            .keywords
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty());

        let mut rows: Vec<Note> = state
            .notes
            .iter()
            .filter(|n| n.creator_id == filter.creator_id && n.deleted_at.is_none())
            .filter(|n| filter.folder_id.is_none_or(|f| n.folder_id == Some(f)))
            .filter(|n| filter.pinned.is_none_or(|p| n.pinned == p))
            .filter(|n| keywords.is_none_or(|kw| matches(&n.title, kw) || matches(&n.content, kw)))
            .filter(|n| match filter.cursor {
                None => true,
                Some((ts, id)) => (n.created_at, n.id) < (ts, id),
            })
            .cloned()
            .collect();

        rows.sort_by_key(|n| (n.created_at, n.id));
        rows.reverse();
        rows.truncate(usize::try_from(filter.limit_plus_one).unwrap_or(usize::MAX));
        Ok(rows)
    }

    async fn add_note_to_folder(
        &self,
        creator_id: Uuid,
        note_id: Uuid,
        folder_id: Uuid,
    ) -> Result<Option<Note>, MenoError> {
        let mut state = self.lock();
        let Some(note) = state
            .notes
            .iter_mut()
            .find(|n| n.id == note_id && n.creator_id == creator_id && n.deleted_at.is_none())
        else {
            return Ok(None);
        };
        note.folder_id = Some(folder_id);
        touch(note);
        Ok(Some(note.clone()))
    }

    async fn remove_note_from_folder(
        &self,
        creator_id: Uuid,
        note_id: Uuid,
        folder_id: Uuid,
    ) -> Result<Option<Note>, MenoError> {
        let mut state = self.lock();
        let Some(note) = state.notes.iter_mut().find(|n| {
            n.id == note_id
                && n.creator_id == creator_id
                && n.folder_id == Some(folder_id)
                && n.deleted_at.is_none()
        }) else {
            return Ok(None);
        };
        note.folder_id = None;
        touch(note);
        Ok(Some(note.clone()))
    }

    async fn move_notes_to_folder(
        &self,
        creator_id: Uuid,
        folder_id: Option<Uuid>,
        note_ids: &[Uuid],
    ) -> Result<Vec<Note>, MenoError> {
        let mut state = self.lock();
        let ids: HashSet<Uuid> = note_ids.iter().copied().collect();
        let mut moved = Vec::new();
        for note in &mut state.notes {
            if ids.contains(&note.id) && note.creator_id == creator_id && note.deleted_at.is_none()
            {
                note.folder_id = folder_id;
                touch(note);
                moved.push(note.clone());
            }
        }
        Ok(moved)
    }

    async fn find_notes_changed_since(
        &self,
        creator_id: Uuid,
        cursor: Option<(OffsetDateTime, Uuid)>,
        limit: i64,
    ) -> Result<Vec<Note>, MenoError> {
        let state = self.lock();
        let mut rows: Vec<Note> = state
            .notes
            .iter()
            .filter(|n| n.creator_id == creator_id)
            .filter(|n| cursor.is_none_or(|(ts, id)| (n.updated_at, n.id) > (ts, id)))
            .cloned()
            .collect();
        rows.sort_by_key(|n| (n.updated_at, n.id));
        rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        Ok(rows)
    }

    async fn create_folder(&self, input: &CreateFolderInput<'_>) -> Result<Folder, MenoError> {
        let mut state = self.lock();
        if state.folders.iter().any(|f| f.id == input.id) {
            return Err(error::internal(
                "create_folder",
                "a folder with that id exists",
            ));
        }
        let now = OffsetDateTime::now_utc();
        let folder = Folder {
            id: input.id,
            title: input.title.to_owned(),
            pinned: input.pinned,
            creator_id: input.creator_id,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        state.folders.push(folder.clone());
        Ok(folder)
    }

    async fn update_folder_by_version(
        &self,
        input: &UpdateFolderInput<'_>,
    ) -> Result<Option<Folder>, MenoError> {
        let mut state = self.lock();
        let Some(folder) = state.folders.iter_mut().find(|f| {
            f.id == input.folder_id
                && f.creator_id == input.creator_id
                && f.version == input.base_version
                && f.deleted_at.is_none()
        }) else {
            return Ok(None);
        };
        if let Some(title) = input.title {
            folder.title = title.to_owned();
        }
        if let Some(pinned) = input.pinned {
            folder.pinned = pinned;
        }
        touch_folder(folder);
        Ok(Some(folder.clone()))
    }

    async fn delete_folder(
        &self,
        id: Uuid,
        creator_id: Uuid,
        base_version: i32,
        notes: NotesOnFolderDelete,
    ) -> Result<Option<Folder>, MenoError> {
        let mut state = self.lock();
        let Some(folder) = state.folders.iter_mut().find(|f| {
            f.id == id
                && f.creator_id == creator_id
                && f.version == base_version
                && f.deleted_at.is_none()
        }) else {
            return Ok(None);
        };
        folder.deleted_at = Some(OffsetDateTime::now_utc());
        touch_folder(folder);
        let folder = folder.clone();

        for note in &mut state.notes {
            if note.folder_id == Some(id)
                && note.creator_id == creator_id
                && note.deleted_at.is_none()
            {
                match notes {
                    NotesOnFolderDelete::Delete => {
                        note.deleted_at = Some(OffsetDateTime::now_utc());
                    }
                    NotesOnFolderDelete::Orphan => note.folder_id = None,
                }
                touch(note);
            }
        }

        Ok(Some(folder))
    }

    async fn find_folder_by_id(
        &self,
        id: Uuid,
        creator_id: Uuid,
    ) -> Result<Option<Folder>, MenoError> {
        Ok(self
            .lock()
            .folders
            .iter()
            .find(|f| f.id == id && f.creator_id == creator_id && f.deleted_at.is_none())
            .cloned())
    }

    async fn find_folders(&self, filter: &FolderFilter) -> Result<Vec<Folder>, MenoError> {
        let state = self.lock();
        let keywords = filter
            .keywords
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty());

        let mut rows: Vec<Folder> = state
            .folders
            .iter()
            .filter(|f| f.creator_id == filter.creator_id && f.deleted_at.is_none())
            .filter(|f| filter.pinned.is_none_or(|p| f.pinned == p))
            .filter(|f| keywords.is_none_or(|kw| matches(&f.title, kw)))
            .filter(|f| match filter.cursor {
                None => true,
                Some((ts, id)) => (f.created_at, f.id) < (ts, id),
            })
            .cloned()
            .collect();

        rows.sort_by_key(|f| (f.created_at, f.id));
        rows.reverse();
        rows.truncate(usize::try_from(filter.limit_plus_one).unwrap_or(usize::MAX));
        Ok(rows)
    }

    async fn find_folders_changed_since(
        &self,
        creator_id: Uuid,
        cursor: Option<(OffsetDateTime, Uuid)>,
        limit: i64,
    ) -> Result<Vec<Folder>, MenoError> {
        let state = self.lock();
        let mut rows: Vec<Folder> = state
            .folders
            .iter()
            .filter(|f| f.creator_id == creator_id)
            .filter(|f| cursor.is_none_or(|(ts, id)| (f.updated_at, f.id) > (ts, id)))
            .cloned()
            .collect();
        rows.sort_by_key(|f| (f.updated_at, f.id));
        rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        Ok(rows)
    }

    async fn apply_sync_batch(
        &self,
        creator_id: Uuid,
        batch: &SyncBatch,
    ) -> Result<Vec<ResolvedMutation>, MenoError> {
        let mut state = self.lock();

        let owned: HashSet<Uuid> = batch
            .notes
            .iter()
            .filter_map(|(_, m)| m.folder_id)
            .filter(|id| {
                state
                    .folders
                    .iter()
                    .any(|f| f.id == *id && f.creator_id == creator_id)
            })
            .collect();

        let mut resolved = Vec::with_capacity(batch.len());

        for (index, mutation) in &batch.folders {
            let resolution = resolve_folder(&mut state, creator_id, mutation);
            resolved.push(ResolvedMutation {
                index: *index,
                resolution,
            });
        }

        for (index, mutation) in &batch.notes {
            let folder_id = match mutation.folder_id {
                Some(folder_id) if owned.contains(&folder_id) => Some(folder_id),
                _ => None,
            };
            let resolution = resolve_note(&mut state, creator_id, mutation, folder_id);
            resolved.push(ResolvedMutation {
                index: *index,
                resolution,
            });
        }

        resolved.sort_by_key(|r| r.index);
        Ok(resolved)
    }

    async fn purge_deleted_notes_older_than(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<u64, MenoError> {
        let mut state = self.lock();
        let before = state.notes.len();
        state
            .notes
            .retain(|n| !n.deleted_at.is_some_and(|deleted| deleted < cutoff));
        Ok((before - state.notes.len()) as u64)
    }

    async fn purge_deleted_folders_older_than(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<u64, MenoError> {
        let mut state = self.lock();
        let before = state.folders.len();
        state
            .folders
            .retain(|f| !f.deleted_at.is_some_and(|deleted| deleted < cutoff));
        Ok((before - state.folders.len()) as u64)
    }
}
