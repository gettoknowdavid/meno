//! Every date on every response is RFC 3339 — enforced for the whole workspace.
//!
//! # The rule
//!
//! `time`'s default `Serialize` for `OffsetDateTime` is a nine-number tuple,
//! `[year, ordinal, hour, minute, second, nanosecond, offset_h, offset_m, offset_s]`.
//! The crate avoids strings on purpose ("strings are avoided to allow for optimal
//! representations in various binary forms"), so a response field that is just
//! `pub created_at: OffsetDateTime` puts that tuple on the wire:
//!
//! ```json
//! { "created_at": [2026, 278, 20, 51, 45, 911000000, 0, 0, 0] }
//! ```
//!
//! It compiles, it round-trips through `time`'s own `Deserialize`, and no client can
//! read it — element 1 is a *day of year*, not a month, so even a reader that guessed
//! the layout would be wrong by up to eleven months. That is exactly the shape
//! `GET /auth/sessions` shipped until the fields were annotated.
//!
//! **So: a field typed as a date and reachable from `Serialize` must carry
//! `#[serde(with = "time::serde::rfc3339")]`** (or `…::option` for an `Option`).
//!
//! # Why this is a test and not a lint or a review rule
//!
//! Three ways to stop it, and why this one:
//!
//! - **A review rule** is what the annotation on `UserResponse` already was. It held
//!   for one struct, and the two date fields added next to it missed.
//! - **A newtype wrapper** would make it a compile error, which is strictly stronger —
//!   but it changes the type of every timestamp in the codebase and needs sqlx `Type`
//!   glue to keep decoding `TIMESTAMPTZ`. Worth doing deliberately; not a change to
//!   smuggle in alongside a bug fix.
//! - **This test** fails the suite for every serialisable struct in `apps/api/src` and
//!   `crates/core/src`, present and future, with no type churn and no new dependency.
//!
//! It reads the source rather than the wire because there is no way to *reach* every
//! response: an endpoint nobody has wired up yet is exactly the one that ships the bug.
//! What it checks is the rule itself, so the failure lands on the commit that broke it
//! rather than after a client has parsed `278` as a month.
//!
//! # Scope and limits, stated plainly
//!
//! It is a source check, so it is only as good as its reading of the source. It sees
//! date types written in a field position — named fields, tuple fields and enum variant
//! fields alike — inside a struct or enum that derives `Serialize`, and it follows
//! attributes across as many lines as they span. A timestamp reached through a type
//! alias, a wrapper struct with a hand-written `impl Serialize`, or a `#[serde_as]`
//! adapter is not something it can see.
//!
//! That gap is why `auth_router.rs` additionally asserts the shape of the responses it
//! *can* reach: this test covers what it cannot, that one covers what it can.
//!
//! A field that genuinely must not be RFC 3339 opts out with `// wire-dates: allow` on
//! the field. No such field exists today.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

/// Types from `time` and `chrono` whose default `Serialize` is a tuple, not a string.
///
/// `Date` is in the list for the same reason: it serialises as `[year, ordinal]`. The
/// match is on identifier boundaries, so `Heartbeat` and `Candidate` are not caught.
const DATE_TYPES: [&str; 6] = [
    "OffsetDateTime",
    "PrimitiveDateTime",
    "UtcDateTime",
    "NaiveDateTime",
    "DateTime",
    "Date",
];

/// The annotation that makes a date a string.
const REQUIRED: &str = "rfc3339";

/// The opt-out marker, for a field that must keep another format.
const OPT_OUT: &str = "wire-dates: allow";

/// The workspace root: the nearest ancestor whose manifest declares `[workspace]`.
///
/// Walking up beats hard-coding `../..`, which silently scans the wrong tree the day
/// the crate moves.
fn workspace_root() -> PathBuf {
    let mut dir: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));

    loop {
        let manifest = dir.join("Cargo.toml");
        if let Ok(text) = fs::read_to_string(&manifest)
            && text.contains("[workspace]")
        {
            return dir.to_path_buf();
        }
        dir = dir
            .parent()
            .expect("a Cargo.toml with [workspace] exists above CARGO_MANIFEST_DIR");
    }
}

/// Every `.rs` file under a directory, sorted so a failure is reproducible.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries =
            fs::read_dir(&dir).unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }

    found.sort();
    found
}

/// Drop a leading visibility: `pub`, `pub(crate)`, `pub(super)`, `pub(in a::b)`.
fn strip_visibility(declaration: &str) -> &str {
    match declaration.strip_prefix("pub(") {
        Some(rest) => rest
            .split_once(')')
            .map_or(rest, |(_, rest)| rest)
            .trim_start(),
        None => declaration
            .strip_prefix("pub")
            .unwrap_or(declaration)
            .trim_start(),
    }
}

/// The name of a struct or enum declared on `line`, or `None`.
fn declared_item_name(line: &str) -> Option<&str> {
    let rest = strip_visibility(line.trim());
    let after_keyword = rest
        .strip_prefix("struct ")
        .or_else(|| rest.strip_prefix("enum "))?;

    Some(
        after_keyword
            .trim_start_matches(|c: char| !(c.is_alphanumeric() || c == '_'))
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default(),
    )
}

/// Whether a type mentions one of the [`DATE_TYPES`], on identifier boundaries.
fn mentions_a_date_type(ty: &str) -> bool {
    DATE_TYPES.iter().any(|name| {
        ty.match_indices(name).any(|(at, _)| {
            let before = ty[..at].chars().next_back();
            let after = ty[at + name.len()..].chars().next();
            let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '_');

            boundary(before) && boundary(after)
        })
    })
}

/// Split `name: Type` into its two halves, for a field declaration.
///
/// Rejects anything whose name is not a plain identifier, so a label, a match arm or a
/// `where` clause cannot be mistaken for a field.
fn split_field(declaration: &str) -> Option<(&str, &str)> {
    let (name, ty) = declaration.split_once(':')?;
    let name = strip_visibility(name.trim());

    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }

    // The field's own separating comma rides along on a line like
    // `pub created_at: OffsetDateTime,`, and it is not part of the type.
    Some((name, ty.trim().trim_end_matches(',').trim_end()))
}

/// The contiguous attribute, doc-comment and derive lines above a declaration.
fn attributes_above<'a>(lines: &'a [&'a str], at: usize) -> Vec<&'a str> {
    let mut block: Vec<&str> = Vec::new();
    let mut balance: i32 = 0;

    for line in lines[..at].iter().rev() {
        let trimmed = line.trim();
        let continues = balance > 0
            || trimmed.starts_with("#[")
            || trimmed.starts_with("///")
            || trimmed.starts_with("//!");

        if !continues {
            break;
        }

        block.push(trimmed);
        balance += trimmed.matches('[').count() as i32 - trimmed.matches(']').count() as i32;
    }

    block.reverse();
    block
}

/// `true` when the lines above an item show a `derive` that includes `Serialize`.
fn derives_serialize(attributes: &[&str]) -> bool {
    attributes
        .iter()
        .any(|line| line.contains("derive(") && line.contains("Serialize"))
}

/// The attributes that apply to the next field, accumulated as the scan passes them.
///
/// Forward accumulation rather than a look-behind, so a five-line
/// `#[serde(\n with = "…",\n)]` needs no special case: the bracket count keeps the
/// attribute open until it closes, and the field is matched against what was collected.
#[derive(Default)]
struct Attributes {
    text: String,
    balance: i32,
}

impl Attributes {
    fn push(&mut self, line: &str) {
        self.text.push_str(line);
        self.balance = (self.balance + line.matches('[').count() as i32
            - line.matches(']').count() as i32)
            .max(0);
    }

    /// Whether a field carrying these attributes needs no format instruction.
    fn exempts(&self, field_line: &str) -> bool {
        self.text.contains(REQUIRED) || self.text.contains(OPT_OUT) || field_line.contains(OPT_OUT)
    }

    fn clear(&mut self) {
        self.text.clear();
        self.balance = 0;
    }
}

/// One date field that would serialise as a tuple.
#[derive(Debug)]
struct Violation {
    /// `path:line`, so the message is a click target rather than prose.
    at: String,
    /// The type carrying the field.
    owner: String,
    /// The field's name, or its position for a tuple field.
    field: String,
    /// The type as written.
    ty: String,
}

/// Record `declaration` as a violation if it is a date the wire cannot read.
fn inspect_field(
    violations: &mut Vec<Violation>,
    path: &str,
    owner: &str,
    line_no: usize,
    declaration: &str,
    attributes: &Attributes,
) {
    let Some((name, ty)) = split_field(declaration) else {
        return;
    };
    if !mentions_a_date_type(ty) || attributes.exempts(declaration) {
        return;
    }

    violations.push(Violation {
        at: format!("{path}:{line_no}"),
        owner: owner.to_owned(),
        field: name.to_owned(),
        ty: ty.to_owned(),
    });
}

/// The fields of a struct-variant written inline, as `rustfmt` leaves a short one:
/// `Immediate { at: OffsetDateTime }`.
fn inline_fields(line: &str) -> Vec<&str> {
    match (line.find('{'), line.rfind('}')) {
        (Some(open), Some(close)) if open < close => line[open + 1..close]
            .split(',')
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Every date field in every serialisable struct and enum variant under `src`.
fn date_fields_without_a_format(root: &Path, src: &str) -> Vec<Violation> {
    let mut violations = Vec::new();

    for path in rust_files(&root.join(src)) {
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let lines: Vec<&str> = text.lines().collect();

        for (at, line) in lines.iter().enumerate() {
            let Some(owner) = declared_item_name(line) else {
                continue;
            };
            if !derives_serialize(&attributes_above(&lines, at)) {
                continue;
            }
            if owner.is_empty() {
                continue;
            }

            let path = path.display().to_string();

            // A tuple struct has no body to walk: `struct Timestamp(OffsetDateTime)`.
            if let Some(open) = line.find('(') {
                let close = line.rfind(')').unwrap_or(line.len());
                if line[..open].find('{').is_none_or(|brace| brace > open) {
                    for (position, field) in line[open + 1..close].split(',').enumerate() {
                        let field = field.trim();
                        let ty = strip_visibility(field);
                        if mentions_a_date_type(ty) {
                            // A tuple field has no name, so a segment without one is
                            // reported by position — which is how a reader locates it.
                            let name = field
                                .split_once(':')
                                .map_or("", |(name, _)| strip_visibility(name.trim()));
                            violations.push(Violation {
                                at: format!("{path}:{}", at + 1),
                                owner: owner.to_owned(),
                                field: if name.is_empty() {
                                    position.to_string()
                                } else {
                                    name.to_owned()
                                },
                                ty: ty.to_owned(),
                            });
                        }
                    }
                    continue;
                }
            }

            let mut attributes = Attributes::default();
            let mut depth = line.matches('{').count() as i32;

            for (offset, body) in lines[at + 1..].iter().enumerate() {
                let line_no = at + 2 + offset;
                let trimmed = body.trim();

                if attributes.balance > 0 || trimmed.starts_with("#[") {
                    attributes.push(trimmed);
                    continue;
                }
                if trimmed.starts_with("//") || trimmed.is_empty() {
                    continue;
                }

                inspect_field(&mut violations, &path, owner, line_no, trimmed, &attributes);
                // A struct variant short enough for `rustfmt` keeps its fields on one
                // line, and that line has no field name in the position a scan expects.
                for field in inline_fields(trimmed) {
                    inspect_field(
                        &mut violations,
                        &path,
                        owner,
                        line_no,
                        field,
                        &Attributes::default(),
                    );
                }

                attributes.clear();
                depth += body.matches('{').count() as i32 - body.matches('}').count() as i32;
                if depth <= 0 {
                    break;
                }
            }
        }
    }

    violations
}

/// Every unannotated date field in the crate that serves the responses, and the shared
/// primitives those responses are built from.
#[test]
fn no_response_field_serialises_a_date_as_a_tuple() {
    let root = workspace_root();

    let violations: Vec<Violation> = ["apps/api/src", "crates/core/src"]
        .iter()
        .flat_map(|src| date_fields_without_a_format(&root, src))
        .collect();

    assert!(
        violations.is_empty(),
        "{} date field(s) would reach a client as `time`'s nine-number tuple instead of an \
         RFC 3339 string.\n\n{}\n\nAdd `#[serde(with = \"time::serde::rfc3339\")]` above each \
         (or `#[serde(with = \"time::serde::rfc3339::option\")]` for an `Option`). If a field \
         genuinely cannot be RFC 3339, mark it with `{OPT_OUT}` and say why — \
         `crates/core/src/time.rs` explains why the default is unreadable.",
        violations.len(),
        violations
            .iter()
            .map(|v| format!("  {} — {}::{}: {}", v.at, v.owner, v.field, v.ty))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

/// The guard is run against a fixture holding one of every shape it must get right.
#[test]
fn the_guard_reports_the_bugs_and_ignores_the_rest() {
    // A guard that cannot fail is worse than none: it looks like coverage and catches
    // nothing. This runs the real scanner over a fixture built to be wrong in every way
    // it has to catch, and correct in every way it must not.
    let support = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support");
    let found = date_fields_without_a_format(&support, "wire_dates_fixture");

    let reported: Vec<(String, String)> = found
        .iter()
        .map(|v| (format!("{}::{}", v.owner, v.field), v.ty.clone()))
        .collect();

    assert_eq!(
        reported,
        vec![
            (
                "Unannotated::bare_at".to_owned(),
                "OffsetDateTime".to_owned()
            ),
            (
                "Unannotated::bare_maybe".to_owned(),
                "Option<OffsetDateTime>".to_owned(),
            ),
            (
                "Unannotated::bare_vec".to_owned(),
                "Vec<OffsetDateTime>".to_owned(),
            ),
            (
                "Unannotated::multiline".to_owned(),
                "DateTime<Utc>".to_owned(),
            ),
            ("Wrapped::0".to_owned(), "OffsetDateTime".to_owned()),
            ("Scheduled::at".to_owned(), "OffsetDateTime".to_owned()),
        ],
        "the scanner must report exactly the unannotated fields, in source order",
    );

    // Reported positions must be real lines, or the failure message points nowhere.
    let line_of = |field: &str| {
        found
            .iter()
            .find(|v| v.field == field)
            .map(|v| v.at.clone())
            .unwrap_or_default()
    };
    assert!(
        line_of("bare_at").ends_with("wire_dates_fixture.rs:17"),
        "bare_at should be reported at line 17, got {}",
        line_of("bare_at"),
    );
}
