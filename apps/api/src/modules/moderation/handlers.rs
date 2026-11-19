//! The moderation endpoints (plan §9.3).
//!
//! Each function is parse → delegate → respond. The operator gate, the existence checks
//! and the block rules all live in [`super::service`]; nothing here decides anything it
//! could decide.
//!
//! # The five paths and why they are shaped this way
//!
//! | Path | Who | Answer |
//! | --- | --- | --- |
//! | `POST /moderation/reports` | any signed-in account | `201` with the filed report |
//! | `GET /moderation/reports` | an operator | `200` with a page of the queue |
//! | `POST /moderation/reports/{id}` | an operator | `200` with the resolved report |
//! | `GET /moderation/blocks` | any signed-in account | `200` with its own block list |
//! | `POST`/`DELETE /moderation/blocks/{id}` | any signed-in account | `200` with no body |
//!
//! The block list is *not* paginated: it is bounded by one person's decisions, and an
//! unpaged read is what lets a client hold the whole list of accounts it must not see.

use axum::extract::{Extension, Path, Query, State};
use uuid::Uuid;

use super::dto::{
    BlockedUserResponse, CreateReportRequest, ReportQuery, ReportResponse, ResolveReportRequest,
};
use super::state::ModerationState;
use crate::middleware::auth::AuthUser;
use crate::types::meno_response::{MenoResponse, Outcome};
use meno_core::CursorPage;

/// `POST /moderation/reports` — file a report about an account or a broadcast.
///
/// # Errors
///
/// `400` when the reason or details break their bounds; `404` when the subject does not
/// exist.
pub async fn create_report(
    State(state): State<ModerationState>,
    Extension(auth): Extension<AuthUser>,
    axum::Json(request): axum::Json<CreateReportRequest>,
) -> Outcome<ReportResponse> {
    let report = state.service.report(&auth, &request).await?;
    Ok(MenoResponse::created("Report submitted", report))
}

/// `GET /moderation/reports` — the operator queue.
///
/// # Errors
///
/// `403` for a caller who is not an operator; `400` `INVALID_CURSOR` for a malformed
/// cursor.
pub async fn list_reports(
    State(state): State<ModerationState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<ReportQuery>,
) -> Outcome<CursorPage<ReportResponse>> {
    let page = state
        .service
        .reports(&auth, query.status, &query.pagination)
        .await?;
    Ok(MenoResponse::ok("Reports", page))
}

/// `POST /moderation/reports/{id}` — record an operator's decision.
///
/// # Errors
///
/// `403` for a caller who is not an operator; `400` for `status: open`; `404` for an id
/// no report carries.
pub async fn resolve_report(
    State(state): State<ModerationState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<ResolveReportRequest>,
) -> Outcome<ReportResponse> {
    let report = state.service.resolve(&auth, id, request.status).await?;
    Ok(MenoResponse::ok("Report resolved", report))
}

/// `POST /moderation/blocks/{id}` — block an account.
///
/// # Errors
///
/// `400` for the caller's own id; `404` when no live account carries the id.
pub async fn block(
    State(state): State<ModerationState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<()> {
    state.service.block(&auth, id).await?;
    Ok(MenoResponse::no_content("User blocked"))
}

/// `DELETE /moderation/blocks/{id}` — remove a block. Idempotent.
///
/// # Errors
///
/// `404` when no live account carries the id.
pub async fn unblock(
    State(state): State<ModerationState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<()> {
    state.service.unblock(&auth, id).await?;
    Ok(MenoResponse::no_content("User unblocked"))
}

/// `GET /moderation/blocks` — the caller's own block list.
///
/// # Errors
///
/// Whatever storage reports.
pub async fn blocked(
    State(state): State<ModerationState>,
    Extension(auth): Extension<AuthUser>,
) -> Outcome<Vec<BlockedUserResponse>> {
    let blocked = state.service.blocked(&auth).await?;
    Ok(MenoResponse::ok("Blocked users", blocked))
}
