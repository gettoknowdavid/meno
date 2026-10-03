//! Canonical time type.
//!
//! Meno uses `time::OffsetDateTime` everywhere. `chrono` is confined to the Apalis job
//! payloads, which require it; converting at that one boundary is cheap, but letting it
//! leak into services is not.
//!
//! The project carries **both** `time::OffsetDateTime` and `chrono::DateTime<Utc>` on
//! `master`, and two time types in application logic is a reliable source of conversion
//! bugs at the sqlx boundary — a `chrono` value silently bound to a `TIMESTAMPTZ` column
//! compiles fine and is wrong by a timezone offset. Centralising the choice is the fix.
//!
//! `chrono` remains in `[workspace.dependencies]` for exactly one reason:
//! `apalis-postgres` stores its schedule in `chrono` types. `infrastructure/jobs/`
//! converts at that boundary and nowhere else.

use time::OffsetDateTime;

/// The current instant, in UTC.
#[must_use]
pub fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// True when `ts` is in the past. Used by broadcast scheduling validation.
#[must_use]
pub fn is_past(ts: OffsetDateTime) -> bool {
    ts < now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_utc() {
        assert_eq!(now().offset(), time::UtcOffset::UTC);
    }

    #[test]
    fn is_past_distinguishes_yesterday_from_tomorrow() {
        assert!(is_past(now() - time::Duration::days(1)));
        assert!(!is_past(now() + time::Duration::days(1)));
    }
}
