//! The rows the moderation module reads and writes.
//!
//! # Two shapes, one queue
//!
//! A report names either an account or a broadcast, and both go in one table so an
//! operator reads one list. [`ReportSubject`] is the typed half of that decision: the
//! database constrains the two values, and this enum is what a repository can store
//! without spelling the string in three places.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

/// What a report is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportSubject {
    /// An account.
    User,
    /// A broadcast.
    Broadcast,
}

impl ReportSubject {
    /// The value stored in `moderation_reports.subject_type`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Broadcast => "broadcast",
        }
    }

    /// Parse a stored `subject_type`.
    ///
    /// `None` for anything else, so an unknown value is a row this build does not
    /// understand rather than a user report silently read as a broadcast one.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "user" => Some(Self::User),
            "broadcast" => Some(Self::Broadcast),
            _ => None,
        }
    }
}

/// Where a report stands.
///
/// Three values rather than a boolean: `open` is the queue, and an operator needs to
/// distinguish "we looked and acted" from "we looked and there was nothing in it".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    /// Waiting for an operator.
    Open,
    /// Read, and acted on.
    Reviewed,
    /// Read, and no action taken.
    Dismissed,
}

impl ReportStatus {
    /// The value stored in `moderation_reports.status`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Reviewed => "reviewed",
            Self::Dismissed => "dismissed",
        }
    }

    /// Parse a stored `status`.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "open" => Some(Self::Open),
            "reviewed" => Some(Self::Reviewed),
            "dismissed" => Some(Self::Dismissed),
            _ => None,
        }
    }
}

/// A `moderation_reports` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Primary key.
    pub id: Uuid,
    /// Who filed it.
    pub reporter_id: Uuid,
    /// What kind of thing it names.
    pub subject_type: ReportSubject,
    /// The account or broadcast it names.
    pub subject_id: Uuid,
    /// The short reason, from a fixed vocabulary the client offers.
    pub reason: String,
    /// The reporter's own words, when they added any.
    pub details: Option<String>,
    /// Where it stands.
    pub status: ReportStatus,
    /// When it was filed.
    pub created_at: OffsetDateTime,
    /// When an operator resolved it; `None` while open.
    pub resolved_at: Option<OffsetDateTime>,
}

/// The row a new report is inserted from.
#[derive(Debug, Clone)]
pub struct NewReport<'a> {
    /// Who is filing it.
    pub reporter_id: Uuid,
    /// What kind of thing it names.
    pub subject_type: ReportSubject,
    /// The account or broadcast it names.
    pub subject_id: Uuid,
    /// The short reason.
    pub reason: &'a str,
    /// The reporter's own words, if any.
    pub details: Option<&'a str>,
}

/// An account the caller has blocked, for the block list.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct BlockedUser {
    /// The blocked account.
    pub id: Uuid,
    /// Its display name, so the list is readable without a second request per row.
    pub full_name: String,
    /// When the block was made.
    pub blocked_at: OffsetDateTime,
}
