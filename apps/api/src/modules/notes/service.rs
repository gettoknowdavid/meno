use crate::jobs::Jobs;
use crate::modules::notes::dto;
use crate::modules::notes::dto::{
    ConflictEntity, CreateFolderRequest, DeleteFolderRequest, FolderDto, FoldersQuery,
    MoveNotesToFolderRequest, NotesQuery, UpdateFolderRequest,
};
use crate::modules::notes::errors::NotesError;
use crate::modules::notes::repository;
use crate::modules::notes::repository::{CreateFolderInput, UpdateFolderInput};
use crate::shared::pagination::{Cursor, CursorPage};
use dto::{CreateNoteRequest, NoteDto, UpdateNoteRequest};
use repository::{NotesRepo, NotesRepository, UpdateNoteInput};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub type DynNotesService = NotesService<NotesRepository>;

#[derive(Clone)]
pub struct NotesService<R: NotesRepo = NotesRepository> {
    repo: Arc<R>,
    db: PgPool,
    jobs: Jobs,
}
impl<R: NotesRepo> NotesService<R> {
    #[must_use]
    pub fn new(repo: Arc<R>, db: PgPool, jobs: Jobs) -> Self {
        Self { repo, db, jobs }
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
}
