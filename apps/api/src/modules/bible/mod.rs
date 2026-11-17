//! The Bible catalogue: what scripture this deployment publishes for download.
//!
//! # Why this module exists
//!
//! The mobile app ships one translation and offers the rest as downloads. The payloads
//! themselves are static objects on the CDN — `bible/<code>/<version>.json.gz`, uploaded
//! by the `publish-bible` binary — so this module has no repository, no service and no
//! database: a translation's file never changes at runtime, and the catalogue is a
//! compile-time table ([`catalogue`]).
//!
//! # The two halves
//!
//! - [`handlers::get_translations`] — `GET /bible/translations`, public and
//!   rate-limited. It answers with the code, name, version, byte size, SHA-256 and
//!   absolute payload URL of every published payload, so a client needs exactly one
//!   request to decide what it can download and how to verify it.
//! - [`publish`] — the upload path, called by `publish-bible` rather than by a route. It
//!   refuses a file the catalogue does not describe, so the endpoint can never advertise
//!   a hash that was not what got uploaded.
//!
//! # Why the catalogue is not a database table
//!
//! A row per translation would be writable at runtime and would therefore need
//! migrations, a repository and an admin path to keep it honest — for three rows that
//! change when a release is cut. As a `const`, the published set is reviewed in the diff
//! that adds the payload, and a test asserts every entry's shape.
//!
//! # No account, no session
//!
//! Scripture is not per user: the same `asv/1.json.gz` is served to everybody, the CDN
//! read is unsigned (§3.3), and nothing here reads `AuthUser`. That is why the route is
//! public and why this module's state carries a URL base rather than a pool.

pub mod catalogue;
pub mod dto;
pub mod handlers;
pub mod publish;
pub mod state;
