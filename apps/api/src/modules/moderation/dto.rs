//! The moderation endpoints' wire shapes.
//!
//! Field names are snake_case on the way in because that is what this API's request
//! bodies use (`new_password`, `refresh_token`); responses carry `#[serde(rename_all =
//! "camelCase")]` with RFC 3339 timestamps, like every other response in the crate.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::model::{BlockedUser, Report, ReportStatus, ReportSubject};
use meno_core::{CursorParams, Error as MenoError};

/// Longest reason a report carries.
///
/// The client offers a fixed vocabulary, but this is a boundary and the boundary does not
/// trust the client: an unbounded string is an unbounded write.
pub const REASON_MAX_CHARS: usize = 120;

/// Longest free-text detail a report carries.
pub const DETAILS_MAX_CHARS: usize = 1_000;

/// `POST /moderation/reports`.
#[derive(Debug, Deserialize)]
pub struct CreateReportRequest {
    /// `user` or `broadcast`.
    pub subject_type: ReportSubject,
    /// The account or broadcast being reported.
    pub subject_id: Uuid,
    /// Why, in the client's short vocabulary.
    pub reason: String,
    /// Anything the reporter added.
    pub details: Option<String>,
}

impl CreateReportRequest {
    /// Check the body before anything is written.
    ///
    /// # Errors
    ///
    /// [`MenoError::Validation`] naming each field that failed.
    pub fn validate(&self) -> Result<(), MenoError> {
        let mut fields = meno_core::error::FieldErrors::new();

        if self.reason.trim().is_empty() {
            fields.push("reason", "A reason is required");
        } else if self.reason.chars().count() > REASON_MAX_CHARS {
            fields.push(
                "reason",
                format!("A reason is at most {REASON_MAX_CHARS} characters"),
            );
        }

        if let Some(details) = &self.details
            && details.chars().count() > DETAILS_MAX_CHARS
        {
            fields.push(
                "details",
                format!("Details are at most {DETAILS_MAX_CHARS} characters"),
            );
        }

        fields.into_result()
    }
}

/// A report as the queue and the create response render it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportResponse {
    /// Primary key.
    pub id: Uuid,
    /// Who filed it.
    pub reporter_id: Uuid,
    /// What kind of thing it names.
    pub subject_type: ReportSubject,
    /// The account or broadcast it names.
    pub subject_id: Uuid,
    /// The short reason.
    pub reason: String,
    /// The reporter's own words, if any.
    pub details: Option<String>,
    /// Where it stands.
    pub status: ReportStatus,
    /// When it was filed.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When an operator resolved it; absent while open.
    #[serde(with = "time::serde::rfc3339::option")]
    pub resolved_at: Option<OffsetDateTime>,
}

impl From<Report> for ReportResponse {
    fn from(report: Report) -> Self {
        Self {
            id: report.id,
            reporter_id: report.reporter_id,
            subject_type: report.subject_type,
            subject_id: report.subject_id,
            reason: report.reason,
            details: report.details,
            status: report.status,
            created_at: report.created_at,
            resolved_at: report.resolved_at,
        }
    }
}

/// `GET /moderation/reports`.
#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    /// Only reports in this state; the whole table when absent.
    pub status: Option<ReportStatus>,
    /// Cursor and page size.
    #[serde(flatten)]
    pub pagination: CursorParams,
}

/// `POST /moderation/reports/{id}` — an operator's decision.
#[derive(Debug, Deserialize)]
pub struct ResolveReportRequest {
    /// `reviewed` or `dismissed`. `open` is not a resolution.
    pub status: ReportStatus,
}

/// One row of the caller's block list.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedUserResponse {
    /// The blocked account.
    pub id: Uuid,
    /// Its display name.
    pub full_name: String,
    /// When the block was made.
    #[serde(with = "time::serde::rfc3339")]
    pub blocked_at: OffsetDateTime,
}

impl From<BlockedUser> for BlockedUserResponse {
    fn from(user: BlockedUser) -> Self {
        Self {
            id: user.id,
            full_name: user.full_name,
            blocked_at: user.blocked_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(reason: &str, details: Option<&str>) -> CreateReportRequest {
        CreateReportRequest {
            subject_type: ReportSubject::User,
            subject_id: Uuid::from_u128(1),
            reason: reason.to_owned(),
            details: details.map(str::to_owned),
        }
    }

    #[test]
    fn a_reason_is_required_and_bounded() {
        assert!(request("", None).validate().is_err());
        assert!(request("   ", None).validate().is_err());
        assert!(
            request(&"a".repeat(REASON_MAX_CHARS + 1), None)
                .validate()
                .is_err()
        );
        assert!(request("harassment", None).validate().is_ok());
    }

    #[test]
    fn details_are_optional_but_bounded() {
        assert!(request("spam", None).validate().is_ok());
        assert!(
            request("spam", Some(&"a".repeat(DETAILS_MAX_CHARS + 1)))
                .validate()
                .is_err()
        );
        assert!(
            request("spam", Some(&"a".repeat(DETAILS_MAX_CHARS)))
                .validate()
                .is_ok(),
            "the bound is inclusive"
        );
    }

    #[test]
    fn the_wire_uses_the_values_the_database_constrains() {
        assert_eq!(ReportSubject::User.as_str(), "user");
        assert_eq!(ReportSubject::Broadcast.as_str(), "broadcast");
        assert_eq!(ReportStatus::Open.as_str(), "open");
        assert_eq!(ReportStatus::Reviewed.as_str(), "reviewed");
        assert_eq!(ReportStatus::Dismissed.as_str(), "dismissed");
        for status in [
            ReportStatus::Open,
            ReportStatus::Reviewed,
            ReportStatus::Dismissed,
        ] {
            assert_eq!(ReportStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(ReportSubject::parse("nonsense"), None);
    }
}
