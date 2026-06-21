use crate::modules::notes::dto;
use crate::modules::notes::dto::UpdateNoteRequest;
use crate::modules::notes::errors::NotesError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::middleware::extractors::MenoBody;
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
