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
//!
//! # Serialising a date
//!
//! **A field typed as `OffsetDateTime` and destined for a response must carry
//! `#[serde(with = "time::serde::rfc3339")]`.** On an optional field that is
//! `time::serde::rfc3339::option`, which maps `None` to `null` and back.
//!
//! This is not a stylistic preference. `time`'s own `Serialize` for `OffsetDateTime`
//! deliberately avoids strings — the crate documents that it skips them "to allow for
//! optimal representations in various binary forms" — and emits a nine-number tuple
//! instead: `(year, ordinal, hour, minute, second, nanosecond, offset_h, offset_m,
//! offset_s)`. An API that returned that would answer
//!
//! ```json
//! { "created_at": [2026, 278, 20, 51, 45, 911000000, 0, 0, 0] }
//! ```
//!
//! and the second element is a **day of year**, not a month, so a client that guessed
//! the layout would still be wrong. Turning on `time`'s `serde-human-readable` feature
//! is not the fix either: that format is `2026-10-05 20:51:45.911000000 +00:00:00` —
//! a space separator, nine fractional digits and a colon-bearing offset — which
//! `Date.parse` (Dart) and `new Date(string)` (JavaScript) both reject. RFC 3339 is
//! the only format on this surface that every client already parses.
//!
//! `apps/api/tests/wire_dates.rs` fails the suite if a serialisable struct anywhere in
//! the workspace grows a date field without the annotation, so the omission is a build
//! failure rather than something a client discovers.

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

    /// The nine-number tuple `time` emits when no format is specified.
    ///
    /// Asserted rather than described, because the whole reason every response field
    /// carries `#[serde(with = "time::serde::rfc3339")]` lives in this one fact. If a
    /// future `time` release — or the `serde-human-readable` feature being switched on
    /// in `[workspace.dependencies]` — changed the default, this test fails and points
    /// at the annotations that would then be redundant, instead of the two silently
    /// disagreeing about the wire format.
    #[test]
    fn a_bare_offset_date_time_is_a_nine_number_tuple_and_nothing_else() {
        let json =
            serde_json::to_string(&OffsetDateTime::UNIX_EPOCH).expect("this type serialises");

        assert_eq!(json, "[1970,1,0,0,0,0,0,0,0]", "{json}");
        assert!(
            !json.starts_with('"'),
            "the default must stay a non-string, which is what makes the annotation \
             mandatory rather than cosmetic: {json}"
        );
    }

    /// What the annotation actually buys, at the value level.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Probe {
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
        #[serde(with = "time::serde::rfc3339::option")]
        maybe: Option<OffsetDateTime>,
    }

    #[test]
    fn the_rfc3339_adapter_emits_an_iso_8601_string_and_round_trips() {
        let probe = Probe {
            at: OffsetDateTime::from_unix_timestamp(1_770_000_000).expect("a valid instant"),
            maybe: None,
        };

        let json = serde_json::to_value(&probe).expect("this type serialises");

        assert!(json["at"].is_string(), "a date must be a string: {json}");
        assert_eq!(
            json["at"], "2026-02-02T02:40:00Z",
            "RFC 3339, so `Date.parse` and `new Date(string)` both accept it"
        );
        assert!(
            json["maybe"].is_null(),
            "an absent optional date is null, not an array: {json}"
        );

        let back: Probe = serde_json::from_value(json).expect("this type deserialises");
        assert_eq!(back.at, probe.at);
        assert_eq!(back.maybe, probe.maybe);
    }

    #[test]
    fn the_optional_adapter_keeps_a_present_date_and_rejects_a_tuple() {
        let probe = Probe {
            at: OffsetDateTime::UNIX_EPOCH,
            maybe: Some(OffsetDateTime::UNIX_EPOCH),
        };

        let json = serde_json::to_value(&probe).expect("this type serialises");
        assert!(json["maybe"].is_string(), "{json}");

        let back: Probe = serde_json::from_value(json).expect("this type deserialises");
        assert_eq!(back.maybe, probe.maybe);

        // The regression this whole module note exists for, asserted from the other
        // side: the shape the default impl produces must not be accepted as input
        // either, or a client built against a bad response would keep working while
        // every other client silently failed.
        assert!(
            serde_json::from_value::<Probe>(serde_json::json!({
                "at": [2026, 278, 20, 51, 45, 911000000, 0, 0, 0],
                "maybe": null,
            }))
            .is_err(),
            "the nine-number tuple must not be a valid date on the wire"
        );
    }
}
