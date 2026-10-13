//! The notes endpoints (plan §9.3).
//!
//! Each function is parse → validate → delegate → respond. Nothing here decides anything
//! [`super::service`] can decide; the only `if` a handler is allowed is which extractor it
//! received.
//!
//! # The envelope
//!
//! Every function returns [`Outcome`], whose error half is one `MenoError` rendered by
//! the crate's single §4.2 renderer. Dates are formatted by [`super::dto`] with
//! `#[serde(with = "time::serde::rfc3339")]`, so nothing here touches a timestamp.

use axum::extract::{Extension, Path, Query, State};
use uuid::Uuid;

use super::dto::{
    CreateFolderRequest, CreateNoteRequest, DeleteFolderRequest, DeleteNoteRequest, FolderDto,
    FoldersQuery, MoveNotesToFolderRequest, NoteDto, NotesQuery, NotesSyncPushRequest,
    NotesSyncPushResponse, NotesSyncQuery, NotesSyncResponse, UpdateFolderRequest,
    UpdateNoteRequest, Validatable,
};
use super::state::NotesState;
use crate::middleware::auth::AuthUser;
use crate::types::meno_response::{MenoResponse, Outcome};
use meno_core::CursorPage;

/// `POST /notes` — create.
///
/// # Errors
///
/// 422 with the field map when the body is unacceptable.
pub async fn create_note(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    axum::Json(request): axum::Json<CreateNoteRequest>,
) -> Outcome<NoteDto> {
    request.validate()?;
    let note = state.service.create_note(auth.id, &request).await?;
    Ok(MenoResponse::created("Note created", note))
}

/// `PATCH /notes/{id}`.
///
/// # Errors
///
/// 404 when no live note carries the id; 409 `VERSION_CONFLICT` when it moved on; 422
/// when the body is unacceptable.
pub async fn update_note(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<UpdateNoteRequest>,
) -> Outcome<NoteDto> {
    request.validate()?;
    let note = state.service.update_note(id, auth.id, &request).await?;
    Ok(MenoResponse::ok("Note updated", note))
}

/// `DELETE /notes/{id}`.
///
/// # Errors
///
/// 404 when no live note carries the id; 409 `VERSION_CONFLICT` when it moved on.
pub async fn delete_note(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<DeleteNoteRequest>,
) -> Outcome<NoteDto> {
    let note = state
        .service
        .delete_note(id, auth.id, request.base_version)
        .await?;
    Ok(MenoResponse::ok("Note deleted", note))
}

/// `GET /notes` — a page.
///
/// # Errors
///
/// 400 `INVALID_CURSOR` when `?cursor=` cannot be decoded.
pub async fn get_notes(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<NotesQuery>,
) -> Outcome<CursorPage<NoteDto>> {
    let page = state.service.get_notes(auth.id, &query).await?;
    Ok(MenoResponse::ok("Notes retrieved", page))
}

/// `POST /folders` — create.
///
/// # Errors
///
/// 422 with the field map when the body is unacceptable.
pub async fn create_folder(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    axum::Json(request): axum::Json<CreateFolderRequest>,
) -> Outcome<FolderDto> {
    request.validate()?;
    let folder = state.service.create_folder(auth.id, &request).await?;
    Ok(MenoResponse::created("Folder created", folder))
}

/// `PATCH /folders/{id}`.
///
/// # Errors
///
/// 404 when no live folder carries the id; 409 `VERSION_CONFLICT` when it moved on; 422
/// when the body is unacceptable.
pub async fn update_folder(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<UpdateFolderRequest>,
) -> Outcome<FolderDto> {
    request.validate()?;
    let folder = state.service.update_folder(id, auth.id, &request).await?;
    Ok(MenoResponse::ok("Folder updated", folder))
}

/// `DELETE /folders/{id}` — delete the folder, deleting or unfiling its notes.
///
/// # Errors
///
/// 404 when no live folder carries the id; 409 `VERSION_CONFLICT` when it moved on.
pub async fn delete_folder(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<DeleteFolderRequest>,
) -> Outcome<FolderDto> {
    let folder = state.service.delete_folder(id, auth.id, &request).await?;
    Ok(MenoResponse::ok("Folder deleted", folder))
}

/// `GET /folders` — a page.
///
/// # Errors
///
/// 400 `INVALID_CURSOR` when `?cursor=` cannot be decoded.
pub async fn get_folders(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<FoldersQuery>,
) -> Outcome<CursorPage<FolderDto>> {
    let page = state.service.get_folders(auth.id, &query).await?;
    Ok(MenoResponse::ok("Folders retrieved", page))
}

/// `PUT /notes/{noteId}/folders/{folderId}` — file a note.
///
/// # Errors
///
/// 404 when the note or the folder does not exist for this caller.
pub async fn add_note_to_folder(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    Path((note_id, folder_id)): Path<(Uuid, Uuid)>,
) -> Outcome<NoteDto> {
    let note = state
        .service
        .add_note_to_folder(auth.id, note_id, folder_id)
        .await?;
    Ok(MenoResponse::ok("Note added to folder", note))
}

/// `DELETE /notes/{noteId}/folders/{folderId}` — unfile a note.
///
/// # Errors
///
/// 404 when the note is not in that folder for this caller.
pub async fn remove_note_from_folder(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    Path((note_id, folder_id)): Path<(Uuid, Uuid)>,
) -> Outcome<NoteDto> {
    let note = state
        .service
        .remove_note_from_folder(auth.id, note_id, folder_id)
        .await?;
    Ok(MenoResponse::ok("Note removed from folder", note))
}

/// `PUT /notes` — move a batch of notes.
///
/// # Errors
///
/// 404 when the destination folder does not exist for this caller; 422 when the batch is
/// empty or too large.
pub async fn move_notes_to_folder(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    axum::Json(request): axum::Json<MoveNotesToFolderRequest>,
) -> Outcome<Vec<NoteDto>> {
    request.validate()?;
    let notes = state
        .service
        .move_notes_to_folder(auth.id, &request)
        .await?;
    Ok(MenoResponse::ok("Notes moved", notes))
}

/// `GET /notes/sync` — pull changes.
///
/// # Errors
///
/// 400 `INVALID_CURSOR` when a cursor cannot be decoded.
pub async fn sync_pull(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<NotesSyncQuery>,
) -> Outcome<NotesSyncResponse> {
    let response = state.service.sync_pull(auth.id, &query).await?;
    Ok(MenoResponse::ok("Sync data retrieved", response))
}

/// `POST /notes/sync` — push queued mutations.
///
/// # Errors
///
/// 400 when the batch exceeds the ceiling.
pub async fn sync_push(
    State(state): State<NotesState>,
    Extension(auth): Extension<AuthUser>,
    axum::Json(request): axum::Json<NotesSyncPushRequest>,
) -> Outcome<NotesSyncPushResponse> {
    request.validate()?;
    let response = state.service.sync_push(auth.id, request).await?;
    Ok(MenoResponse::ok("Mutations processed", response))
}
