use crate::jobs::Jobs;
use crate::modules::notes::dto;
use crate::modules::notes::dto::{ConflictEntity, NotesQuery};
use crate::modules::notes::errors::NotesError;
use crate::modules::notes::repository;
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
}
