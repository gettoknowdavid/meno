//! Notes and folders: a soft-deleted, version-guarded store with offline sync.
//!
//! # What this module is
//!
//! A personal notebook. Notes can be filed into folders, pinned, searched, and — the part
//! that shapes everything else — edited *offline* and reconciled later. Every row carries
//! a `version`, every write is guarded on it, and `GET/POST /notes/sync` moves a client's
//! whole queued mutation log in one request.
//!
//! # Parts
//!
//! | File | Answers |
//! | --- | --- |
//! | [`model`] | what a note and a folder *are*, the column-mirroring limits, and the sync mutation language |
//! | [`dto`] | what the wire carries (camelCase, RFC 3339 — §9.5) |
//! | [`error`] | which taxonomy entry a refusal is (§4.2) |
//! | [`repository`] | what is stored — Postgres ([`repository::pg`]) and the §10 double ([`repository::memory`]) |
//! | [`service`] | the rules, in order |
//! | [`handlers`] | how a request becomes a response — nothing else (§9.3) |
//! | [`state`] | which adapter the trait object is bound to (§5.6) |
//!
//! # What changed from `master`, and why
//!
//! `master`'s notes module is the one this replaces (`apps/api/src/modules/notes/`, and
//! `apps/api/src/routes/notes.rs` + `folders.rs`, which do not exist any more). The
//! changes are structural rather than cosmetic:
//!
//! 1. **The repository is a trait object, not an executor-generic struct.**
//!    `master`'s methods took `E: sqlx::Executor` so the service could drive one
//!    transaction through them — which made them un-object-safe, and therefore
//!    un-doubleable. The transactions now live inside the adapter
//!    ([`repository::NotesRepo::apply_sync_batch`] and
//!    [`repository::NotesRepo::delete_folder`]),
//!    which is what lets `tests/notes_service.rs` run the whole rule set with no Postgres.
//! 2. **Deleting a folder acts on its notes.** `master` parsed `shouldDeleteNotes` and
//!    then ignored it, so a deleted folder left every note pointing at a soft-deleted
//!    parent. [`repository::NotesRepo::delete_folder`]
//!    deletes or unfiles them in the same transaction, and the default (`false`) is the
//!    safe branch — unfile.
//! 3. **A push resolves a whole batch against the database, not row by row in the
//!    service.** The last-write-wins tiebreaker and the ownership check on a referenced
//!    folder now happen where the transaction is, so there is no window between reading a
//!    note and writing it.
//! 4. **The errors are the shared taxonomy.** `master`'s ten-variant `NotesError`, its
//!    hand-written `IntoResponse`, and the server-copy-bearing 409 are gone; the 409 is a
//!    plain `VERSION_CONFLICT` and the sync path carries the server copy per item, where
//!    its only consumer reads it.
//!
//! # The layer rule
//!
//! The module reaches down to `infrastructure`, `middleware`, `types` and `meno_core`, and
//! sideways nowhere. Its two tables are its own; it joins no other module's.

pub mod dto;
pub mod error;
pub mod handlers;
pub mod model;
pub mod repository;
pub mod service;
pub mod state;
