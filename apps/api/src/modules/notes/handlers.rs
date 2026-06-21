use crate::modules::notes::dto;
use crate::modules::notes::dto::{DeleteNoteRequest, NotesQuery, UpdateNoteRequest};
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
    Ok(MenoResponse::created("Note created successfully", response))
}

pub async fn update_note(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoBody(body): MenoBody<UpdateNoteRequest>,
) -> Result<MenoResponse<NoteDto>, NotesError> {
    body.validate()?;
    let response = app.notes.service.update_note(id, auth.id, &body).await?;
    Ok(MenoResponse::ok("Note updated successfully", response))
}

pub async fn delete_note(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoBody(body): MenoBody<DeleteNoteRequest>,
) -> Result<MenoResponse<NoteDto>, NotesError> {
    let version = body.base_version;
    let response = app.notes.service.delete_note(id, auth.id, version).await?;
    Ok(MenoResponse::ok("Note deleted successfully", response))
}

pub async fn get_notes(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoQuery(query): MenoQuery<NotesQuery>,
) -> Result<MenoResponse<CursorPage<NoteDto>>, NotesError> {
    let response = app.notes.service.get_notes(auth.id, &query).await?;
    Ok(MenoResponse::ok("Notes retrieved successfully", response))
}
