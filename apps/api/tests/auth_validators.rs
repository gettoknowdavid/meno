//! §10 **Contract** layer: the auth-field policy both apps share.
//!
//! # Why this suite exists
//!
//! `apps/mobile/test/_core/value_objects/validation_parity_test.dart` reads the
//! same `fixtures/auth-validation.json` this file does. The defect (audit H25)
//! was not that either side's rules were wrong in isolation, but that they
//! disagreed: a server-valid account could be refused by the client's own form,
//! and a value that arrived from the server could be rejected on its way into
//! storage. A fixture that both suites assert against is the only way to make
//! that disagreement fail a build instead of a login.
//!
//! # What this cannot prove
//!
//! That the *handlers* call these validators, or which field each message is
//! keyed by — `auth_router.rs` covers the wire, and `dto.rs`'s own tests cover
//! the per-request mapping. This suite pins the policy: the same values, the
//! same answers, in both languages.
//!
//! # Adding a case
//!
//! Put it in the JSON, not here. A case added only to this file would be a
//! second source of truth, which is exactly what the fixture exists to prevent.

// The three panicking lints are denied workspace-wide but explicitly *allowed in
// tests* — see `clippy.toml`. Every `expect` below is a fixture that would make the
// test meaningless if it failed, and its message says so.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;
use std::fs;

use meno_api::modules::auth::validators::{email_problem, full_name_problems, password_problems};
use serde::Deserialize;

/// One value the fixture says the policy accepts or rejects.
#[derive(Debug, Deserialize)]
struct Case {
    /// `email`, `password` or `full_name`.
    kind: String,
    /// The value itself, exactly as the client would submit it.
    value: String,
    /// Why the case exists, used as the assertion's failure message.
    why: String,
}

/// The shared policy file, as written by either side.
#[derive(Debug, Deserialize)]
struct Fixture {
    accepted: Vec<Case>,
    rejected: Vec<Case>,
}

/// Reads the fixture from the repository root.
///
/// `cargo test` runs with the package root (`apps/api`) as its working
/// directory, so the shared folder is two levels up.
fn fixture() -> Fixture {
    let path = "../../fixtures/auth-validation.json";
    let raw = fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("the shared auth-validation fixture must be readable at {path}: {error}")
    });
    serde_json::from_str(&raw).unwrap_or_else(|error| {
        panic!("the shared auth-validation fixture must be valid JSON: {error}")
    })
}

/// Whether the server accepts `value` as `kind`.
///
/// A problem list is a rejection; an empty one is an acceptance. Email answers
/// with a single problem rather than a list, so it is folded into the same
/// boolean shape here.
fn accepted(case: &Case) -> bool {
    match case.kind.as_str() {
        "email" => email_problem(&case.value).is_none(),
        "password" => password_problems(&case.value).is_empty(),
        "full_name" => full_name_problems(&case.value).is_empty(),
        other => panic!("unknown case kind {other:?} in the shared fixture"),
    }
}

#[test]
fn the_fixture_exercises_every_field_on_both_sides() {
    let fixture = fixture();

    for (side, cases) in [
        ("accepted", &fixture.accepted),
        ("rejected", &fixture.rejected),
    ] {
        let kinds: BTreeSet<&str> = cases.iter().map(|case| case.kind.as_str()).collect();
        assert_eq!(
            kinds,
            BTreeSet::from(["email", "password", "full_name"]),
            "the {side} list must cover every kind of value",
        );
        assert!(
            !cases.is_empty(),
            "the {side} list must not be empty, or this suite asserts nothing",
        );
    }
}

#[test]
fn every_accepted_case_is_accepted_by_the_server() {
    for case in fixture().accepted {
        assert!(
            accepted(&case),
            "{} value {:?} must be accepted: {}",
            case.kind,
            case.value,
            case.why,
        );
    }
}

#[test]
fn every_rejected_case_is_rejected_by_the_server() {
    for case in fixture().rejected {
        assert!(
            !accepted(&case),
            "{} value {:?} must be rejected: {}",
            case.kind,
            case.value,
            case.why,
        );
    }
}

/// The one password limit that is the server's alone, so it is not a shared case.
///
/// The five rules the form shows — 8+ characters, A, a, %, 123 — have no maximum, so
/// the fixture cannot state one without the client disagreeing. This bound exists to cap
/// the Argon2 allocation (§9.4), and it is asserted here, beside the fixture, so the
/// asymmetry is recorded where someone comparing the two sides will look.
#[test]
fn the_password_maximum_is_a_server_only_cost_bound() {
    assert!(password_problems(&format!("A1!{}", "a".repeat(125))).is_empty());
    assert_eq!(
        password_problems(&format!("A1!{}", "a".repeat(126))),
        vec!["A password may be at most 128 characters"]
    );
}
