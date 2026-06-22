use crate::modules::notes::dto;
use crate::modules::notes::dto::{
    CreateFolderRequest, DeleteFolderRequest, DeleteNoteRequest, FolderDto, FoldersQuery,
    MoveNotesToFolderRequest, NotesQuery, UpdateFolderRequest, UpdateNoteRequest,
};
use crate::modules::notes::errors::NotesError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::middleware::extractors::{MenoBody, MenoQuery};
use crate::shared::pagination::CursorPage;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::Extension;
use axum::extract::{Path, State};
use dto::{CreateNoteRequest, NoteDto};
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

pub async fn create_note(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoBody(body): MenoBody<CreateNoteRequest>,
) -> Result<MenoResponse<NoteDto>, NotesError> {
    body.validate()?;
    let response = app.notes.service.create_note(auth.id, &body).await?;
    Ok(MenoResponse::created("Note created", response))
}

pub async fn update_note(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoBody(body): MenoBody<UpdateNoteRequest>,
) -> Result<MenoResponse<NoteDto>, NotesError> {
    body.validate()?;
    let response = app.notes.service.update_note(id, auth.id, &body).await?;
    Ok(MenoResponse::ok("Note updated", response))
}

pub async fn delete_note(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoBody(body): MenoBody<DeleteNoteRequest>,
) -> Result<MenoResponse<NoteDto>, NotesError> {
    let version = body.base_version;
    let response = app.notes.service.delete_note(id, auth.id, version).await?;
    Ok(MenoResponse::ok("Note deleted", response))
}

pub async fn get_notes(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoQuery(query): MenoQuery<NotesQuery>,
) -> Result<MenoResponse<CursorPage<NoteDto>>, NotesError> {
    let response = app.notes.service.get_notes(auth.id, &query).await?;
    Ok(MenoResponse::ok("Notes retrieved", response))
}

pub async fn create_folder(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoBody(body): MenoBody<CreateFolderRequest>,
) -> Result<MenoResponse<FolderDto>, NotesError> {
    body.validate()?;
    let response = app.notes.service.create_folder(auth.id, &body).await?;
    Ok(MenoResponse::created("Folder created", response))
}

pub async fn update_folder(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoBody(body): MenoBody<UpdateFolderRequest>,
) -> Result<MenoResponse<FolderDto>, NotesError> {
    body.validate()?;
    let response = app.notes.service.update_folder(id, auth.id, &body).await?;
    Ok(MenoResponse::ok("Folder updated", response))
}

pub async fn delete_folder(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoBody(body): MenoBody<DeleteFolderRequest>,
) -> Result<MenoResponse<FolderDto>, NotesError> {
    let response = app.notes.service.delete_folder(id, auth.id, &body).await?;
    Ok(MenoResponse::ok("Folder deleted", response))
}

pub async fn get_folders(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoQuery(query): MenoQuery<FoldersQuery>,
) -> Result<MenoResponse<CursorPage<FolderDto>>, NotesError> {
    let response = app.notes.service.get_folders(auth.id, &query).await?;
    Ok(MenoResponse::ok("Folders retrieved", response))
}

pub async fn add_note_to_folder(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path((note_id, folder_id)): Path<(Uuid, Uuid)>,
) -> Result<MenoResponse<NoteDto>, NotesError> {
    let response = app
        .notes
        .service
        .add_note_to_folder(auth.id, note_id, folder_id)
        .await?;
    Ok(MenoResponse::ok("Note added to folder", response))
}

pub async fn remove_note_from_folder(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path((note_id, folder_id)): Path<(Uuid, Uuid)>,
) -> Result<MenoResponse<NoteDto>, NotesError> {
    let response = app
        .notes
        .service
        .remove_note_to_folder(auth.id, note_id, folder_id)
        .await?;
    Ok(MenoResponse::ok("Note removed from folder", response))
}

pub async fn move_notes_to_folder(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoBody(body): MenoBody<MoveNotesToFolderRequest>,
) -> Result<MenoResponse<Vec<NoteDto>>, NotesError> {
    body.validate()?;
    let response = app
        .notes
        .service
        .move_notes_to_folder(auth.id, &body)
        .await?;
    Ok(MenoResponse::ok("Notes moved", response))
}
