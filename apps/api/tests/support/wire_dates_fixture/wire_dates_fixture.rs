//! Fixture for `wire_dates.rs` — read as text, never compiled.
//!
//! Every declaration here is a shape the guard has to get right, in source order: four
//! unannotated fields it must report, two more declarations it must report from, and
//! the neighbours it must leave alone. `Uuid` is not even imported, because nothing
//! here is ever built.
//!
//! If this file is edited, the expectations in
//! `the_guard_reports_the_bugs_and_ignores_the_rest` move with it.

use chrono::{DateTime, Utc};
use time::OffsetDateTime;

#[derive(serde::Serialize)]
pub struct Unannotated {
    /// A bare date. This is the bug, and it must land on line 14.
    pub bare_at: OffsetDateTime,
    /// An optional one: the fix is `rfc3339::option`, which this is not.
    pub bare_maybe: Option<OffsetDateTime>,
    /// A collection: one violation, not nine.
    pub bare_vec: Vec<OffsetDateTime>,
    /// A multi-line attribute that names no format is still no format.
    #[serde(rename = "multiline")]
    pub multiline: DateTime<Utc>,

    /// Correctly annotated, and must be left alone.
    #[serde(with = "time::serde::rfc3339")]
    pub fixed_at: OffsetDateTime,
    /// Correctly annotated, optional.
    #[serde(with = "time::serde::rfc3339::option")]
    pub fixed_maybe: Option<OffsetDateTime>,
    /// Not a date, despite the name.
    pub not_a_date: String,
    /// Deliberate exception, with the marker that documents it.
    pub expires_at: OffsetDateTime, // wire-dates: allow
}

/// A tuple struct has no body to walk, so its fields are checked inline.
#[derive(serde::Serialize)]
pub struct Wrapped(pub OffsetDateTime, Uuid);

/// Not serialised, so not on any wire, and must not be reported.
#[derive(Debug, Clone)]
pub struct Row {
    /// A row timestamp.
    pub created_at: OffsetDateTime,
}

/// An enum's variant fields are fields, and are checked like any other.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scheduled {
    /// Reported: no format.
    Immediate { at: OffsetDateTime },
    /// Left alone: correctly annotated.
    At {
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
}
