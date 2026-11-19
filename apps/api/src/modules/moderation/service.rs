//! The moderation rules, as orchestration (§5.1's `service` row).
//!
//! Four decisions live here and nowhere else:
//!
//! - **A report names something that exists.** The database cannot enforce it — the
//!   subject is one of two tables — so the service checks the right one before the
//!   insert, and answers the same 404 for both kinds.
//! - **The queue is an operator's.** [`AuthUser::role`] is the only trusted statement of
//!   who is asking; both queue methods refuse a non-admin before touching storage.
//! - **`open` is not a resolution.** An operator moves a report out of the queue, never
//!   back into it, because "resolved by reopening" would leave `resolved_at` null and the
//!   client reading a state the row cannot hold.
//! - **A block is a decision about one account**, and blocking yourself is a `400` rather
//!   than a database `CHECK` violation surfacing as a 500.

use std::sync::Arc;

use meno_core::{Cursor, CursorPage, CursorParams, Error as MenoError};
use uuid::Uuid;

use super::dto::{BlockedUserResponse, CreateReportRequest, ReportResponse};
use super::error;
use super::model::{NewReport, ReportStatus, ReportSubject};
use super::repository::{ModerationRepo, ReportFilter};
use crate::middleware::auth::AuthUser;
use meno_core::ErrorCode;

/// The moderation rules.
#[derive(Clone)]
pub struct ModerationService {
    repo: Arc<dyn ModerationRepo>,
}

impl std::fmt::Debug for ModerationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModerationService").finish_non_exhaustive()
    }
}

impl ModerationService {
    /// Build the service over its repository.
    #[must_use]
    pub fn new(repo: Arc<dyn ModerationRepo>) -> Self {
        Self { repo }
    }

    /// `POST /moderation/blocks/{id}`.
    ///
    /// # Errors
    ///
    /// [`error::cannot_block_self`] for the caller's own id, `404` for an id that names no
    /// live account, and whatever storage reports.
    #[tracing::instrument(skip(self), fields(blocker_id = %viewer.id, blocked_id = %target_id))]
    pub async fn block(&self, viewer: &AuthUser, target_id: Uuid) -> Result<(), MenoError> {
        if viewer.id == target_id {
            return Err(error::cannot_block_self());
        }

        let created = self.repo.block(viewer.id, target_id).await?;
        // Only a miss costs the second query, and only the second query can tell "already
        // blocked" (a success) from "no such account" (a 404).
        if !created && !self.repo.user_exists(target_id).await? {
            return Err(error::user_not_found());
        }
        Ok(())
    }

    /// `DELETE /moderation/blocks/{id}`.
    ///
    /// Idempotent: a block that is not there is the state the caller asked for, so a
    /// retried tap succeeds.
    ///
    /// # Errors
    ///
    /// `404` for an id that names no live account, and whatever storage reports.
    #[tracing::instrument(skip(self), fields(blocker_id = %viewer.id, blocked_id = %target_id))]
    pub async fn unblock(&self, viewer: &AuthUser, target_id: Uuid) -> Result<(), MenoError> {
        let removed = self.repo.unblock(viewer.id, target_id).await?;
        if !removed && !self.repo.user_exists(target_id).await? {
            return Err(error::user_not_found());
        }
        Ok(())
    }

    /// `GET /moderation/blocks` — the caller's own list.
    ///
    /// Unpaged on purpose: it is bounded by one person's decisions, and a page would let
    /// the list a client caches disagree with the blocks actually enforced.
    ///
    /// # Errors
    ///
    /// Whatever storage reports.
    #[tracing::instrument(skip(self), fields(blocker_id = %viewer.id))]
    pub async fn blocked(&self, viewer: &AuthUser) -> Result<Vec<BlockedUserResponse>, MenoError> {
        Ok(self
            .repo
            .blocked(viewer.id)
            .await?
            .into_iter()
            .map(BlockedUserResponse::from)
            .collect())
    }

    /// `POST /moderation/reports`.
    ///
    /// # Errors
    ///
    /// [`MenoError::Validation`] for a body that breaks the bounds, `404` when the named
    /// account or broadcast does not exist, and whatever storage reports.
    #[tracing::instrument(skip(self, request), fields(reporter_id = %viewer.id))]
    pub async fn report(
        &self,
        viewer: &AuthUser,
        request: &CreateReportRequest,
    ) -> Result<ReportResponse, MenoError> {
        request.validate()?;

        let exists = match request.subject_type {
            ReportSubject::User => self.repo.user_exists(request.subject_id).await?,
            ReportSubject::Broadcast => self.repo.broadcast_exists(request.subject_id).await?,
        };
        if !exists {
            return Err(error::subject_not_found());
        }

        let report = self
            .repo
            .create_report(NewReport {
                reporter_id: viewer.id,
                subject_type: request.subject_type,
                subject_id: request.subject_id,
                reason: request.reason.trim(),
                details: request
                    .details
                    .as_deref()
                    .map(str::trim)
                    .filter(|details| !details.is_empty()),
            })
            .await?;

        Ok(ReportResponse::from(report))
    }

    /// `GET /moderation/reports` — the operator queue.
    ///
    /// # Errors
    ///
    /// [`error::not_an_operator`] for a caller who is not an admin, a `400`
    /// `INVALID_CURSOR` for a malformed cursor, and whatever storage reports.
    #[tracing::instrument(skip(self, params), fields(operator_id = %viewer.id))]
    pub async fn reports(
        &self,
        viewer: &AuthUser,
        status: Option<ReportStatus>,
        params: &CursorParams,
    ) -> Result<CursorPage<ReportResponse>, MenoError> {
        require_operator(viewer)?;

        let cursor = params
            .cursor
            .as_ref()
            .map(Cursor::to_timestamp_id)
            .transpose()?;

        let rows = self
            .repo
            .reports(ReportFilter {
                status,
                limit_plus_one: params.limit_plus_one(),
                cursor,
            })
            .await?;

        let responses: Vec<ReportResponse> = rows.into_iter().map(ReportResponse::from).collect();
        Ok(CursorPage::from_rows(responses, params.limit(), |report| {
            Cursor::from_timestamp_id(report.created_at, report.id)
        }))
    }

    /// `POST /moderation/reports/{id}` — record an operator's decision.
    ///
    /// # Errors
    ///
    /// [`error::not_an_operator`], a `400` for `status: open`, and
    /// [`error::report_not_found`] for an id no report carries.
    #[tracing::instrument(skip(self), fields(operator_id = %viewer.id, report_id = %report_id))]
    pub async fn resolve(
        &self,
        viewer: &AuthUser,
        report_id: Uuid,
        status: ReportStatus,
    ) -> Result<ReportResponse, MenoError> {
        require_operator(viewer)?;

        if status == ReportStatus::Open {
            return Err(MenoError::BadRequest {
                code: ErrorCode::BadRequest,
                message: "A report can only be resolved, not reopened".to_owned(),
            });
        }

        if !self.repo.resolve_report(report_id, status).await? {
            return Err(error::report_not_found());
        }

        // The row the decision wrote, so an operator sees the state they just set rather
        // than a client-side guess at `resolvedAt`.
        self.repo
            .report(report_id)
            .await?
            .map(ReportResponse::from)
            .ok_or_else(error::report_not_found)
    }
}

/// Refuse anybody whose role does not reach admin.
fn require_operator(viewer: &AuthUser) -> Result<(), MenoError> {
    if viewer
        .role
        .at_least(crate::middleware::auth::UserRole::Admin)
    {
        return Ok(());
    }
    Err(error::not_an_operator())
}
