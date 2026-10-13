//! What the notes module needs from storage, and nothing else.
//!
//! Same split as the other modules: [`pg`] is Postgres, [`memory`] is the §10 double,
//! and [`NotesRepo`] is the only name the service knows. `state.rs` chooses between them
//! (§5.6), so neither the service nor a handler can reach for `sqlx::PgPool`.
//!
//! # Errors
//!
//! Every method returns [`MenoError`]. A `sqlx::Error` never crosses this boundary: it
//! becomes [`MenoError::Internal`] with the operation named, so driver text reaches the
//! log and not the client (§4.2).
//!
//! # `master`'s repository, and what changed
//!
//! - **It was generic over the executor, which made it un-doubleable.**
//!   `master`'s methods took `E: sqlx::Executor` so the service could drive a transaction
//!   through them — and a value with a generic `Executor` parameter cannot be a trait
//!   object, so there was no way to substitute an in-memory repository. The port moves
//!   each transaction *inside* the adapter: [`NotesRepo::apply_sync_batch`] and
//!   [`NotesRepo::delete_folder`] own their own transactions, and every other method is
//!   a single statement on the pool. That is what makes the §10 double possible, and it
//!   is why the whole service suite runs with no Postgres.
//! - **It took the wire DTO as an argument.** `move_notes_to_folder` took
//!   `&MoveNotesToFolderRequest`, so a change to the request body could silently change a
//!   statement. It now takes the values it actually needs.
//! - **The batch is one call.** `master` read \"is this broadcast live\" — no: read the
//!   note, then wrote it, per mutation, with the transaction held in the service. Here
//!   the repository resolves the whole batch atomically and returns the outcomes, and the
//!   service turns them into the wire shape.

pub mod pg;

// The in-memory repository is a real implementation of this module's trait, not test
// scaffolding: it backs the `tests/` integration suite, which links `meno_api` as an
// external crate where `#[cfg(test)]` is false. Gated on the feature so a production
// build still leaves it out.
#[cfg(any(test, feature = "test-support"))]
pub mod memory;

#[cfg(any(test, feature = "test-support"))]
pub use memory::InMemoryNotesRepo;
pub use pg::PgNotesRepo;

use async_trait::async_trait;
use meno_core::Error as MenoError;
use time::OffsetDateTime;
use uuid::Uuid;

use super::model::{Folder, Note, ResolvedMutation, SyncBatch};

/// Whether a folder's notes should be deleted or unfiled when it is deleted.
///
/// One enum rather than a `bool` so the call site cannot swap the meaning: `Orphan` is
/// the safe branch and `Delete` the destructive one, and the names say which is which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesOnFolderDelete {
    /// Soft-delete every live note in the folder.
    Delete,
    /// Unfile every live note in the folder, leaving it live.
    Orphan,
}

/// The notes module's storage.
#[async_trait]
pub trait NotesRepo: Send + Sync + std::fmt::Debug {
    // ── notes ─────────────────────────────────────────────────────────────

    /// Insert a note.
    async fn create_note(&self, input: &CreateNoteInput<'_>) -> Result<Note, MenoError>;

    /// Apply a patch, guarded on `base_version` and `deleted_at IS NULL`.
    ///
    /// `Ok(None)` when nothing matched, so the caller can explain why.
    async fn update_note_by_version(
        &self,
        input: &UpdateNoteInput<'_>,
    ) -> Result<Option<Note>, MenoError>;

    /// Soft-delete a note, guarded on `base_version`.
    async fn soft_delete_note(
        &self,
        id: Uuid,
        creator_id: Uuid,
        base_version: i32,
    ) -> Result<Option<Note>, MenoError>;

    /// A live note by id, or `None`.
    async fn find_note_by_id(&self, id: Uuid, creator_id: Uuid) -> Result<Option<Note>, MenoError>;

    /// One page of notes, oldest-filtered and ordered by the filter.
    async fn find_notes(&self, filter: &NoteFilter) -> Result<Vec<Note>, MenoError>;

    /// File a note into a folder, guarded on ownership in the statement.
    async fn add_note_to_folder(
        &self,
        creator_id: Uuid,
        note_id: Uuid,
        folder_id: Uuid,
    ) -> Result<Option<Note>, MenoError>;

    /// Unfile a note that is currently in `folder_id`.
    async fn remove_note_from_folder(
        &self,
        creator_id: Uuid,
        note_id: Uuid,
        folder_id: Uuid,
    ) -> Result<Option<Note>, MenoError>;

    /// Move a batch of notes to a folder (or out of one) in one statement.
    async fn move_notes_to_folder(
        &self,
        creator_id: Uuid,
        folder_id: Option<Uuid>,
        note_ids: &[Uuid],
    ) -> Result<Vec<Note>, MenoError>;

    /// Notes changed at-or-after `cursor`, ascending — a forward sync replay.
    async fn find_notes_changed_since(
        &self,
        creator_id: Uuid,
        cursor: Option<(OffsetDateTime, Uuid)>,
        limit: i64,
    ) -> Result<Vec<Note>, MenoError>;

    // ── folders ───────────────────────────────────────────────────────────

    /// Insert a folder.
    async fn create_folder(&self, input: &CreateFolderInput<'_>) -> Result<Folder, MenoError>;

    /// Apply a patch, guarded on `base_version`.
    async fn update_folder_by_version(
        &self,
        input: &UpdateFolderInput<'_>,
    ) -> Result<Option<Folder>, MenoError>;

    /// Soft-delete a folder and, in the same transaction, delete or unfile its notes.
    ///
    /// One call because the two writes are one fact: a folder whose notes still point at
    /// it — as `master`'s delete left them — is a dangling reference no read repairs.
    async fn delete_folder(
        &self,
        id: Uuid,
        creator_id: Uuid,
        base_version: i32,
        notes: NotesOnFolderDelete,
    ) -> Result<Option<Folder>, MenoError>;

    /// A live folder by id, or `None`.
    async fn find_folder_by_id(
        &self,
        id: Uuid,
        creator_id: Uuid,
    ) -> Result<Option<Folder>, MenoError>;

    /// One page of folders.
    async fn find_folders(&self, filter: &FolderFilter) -> Result<Vec<Folder>, MenoError>;

    /// Folders changed at-or-after `cursor`, ascending.
    async fn find_folders_changed_since(
        &self,
        creator_id: Uuid,
        cursor: Option<(OffsetDateTime, Uuid)>,
        limit: i64,
    ) -> Result<Vec<Folder>, MenoError>;

    // ── sync ──────────────────────────────────────────────────────────────

    /// Apply a whole push in one transaction, returning one outcome per mutation.
    ///
    /// Folders are applied before notes so a note can reference a folder created in the
    /// same batch. Per-item validation failures come back as
    /// [`super::model::MutationResolution::Rejected`] and do not roll the batch back; a
    /// driver failure does.
    async fn apply_sync_batch(
        &self,
        creator_id: Uuid,
        batch: &SyncBatch,
    ) -> Result<Vec<ResolvedMutation>, MenoError>;

    // ── maintenance ───────────────────────────────────────────────────────

    /// Hard-delete notes soft-deleted before `cutoff`. Returns how many.
    async fn purge_deleted_notes_older_than(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<u64, MenoError>;

    /// Hard-delete folders soft-deleted before `cutoff`. Returns how many.
    async fn purge_deleted_folders_older_than(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<u64, MenoError>;
}

/// The filter a page of notes is read with, already decoded.
#[derive(Debug, Clone)]
pub struct NoteFilter {
    /// Only this creator's notes.
    pub creator_id: Uuid,
    /// Only notes in this folder.
    pub folder_id: Option<Uuid>,
    /// Only notes with this pinned flag.
    pub pinned: Option<bool>,
    /// Full-text keywords, already trimmed.
    pub keywords: Option<String>,
    /// The page size, plus the one probe row.
    pub limit_plus_one: i64,
    /// The exclusive keyset boundary from the previous page.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
}

/// The filter a page of folders is read with.
#[derive(Debug, Clone)]
pub struct FolderFilter {
    /// Only this creator's folders.
    pub creator_id: Uuid,
    /// Only folders with this pinned flag.
    pub pinned: Option<bool>,
    /// Full-text keywords, already trimmed.
    pub keywords: Option<String>,
    /// The page size, plus the one probe row.
    pub limit_plus_one: i64,
    /// The exclusive keyset boundary from the previous page.
    pub cursor: Option<(OffsetDateTime, Uuid)>,
}

/// The fields a new `notes` row needs.
#[derive(Debug, Clone, Copy)]
pub struct CreateNoteInput<'a> {
    /// The client-chosen id.
    pub id: Uuid,
    /// Title.
    pub title: &'a str,
    /// Body.
    pub content: &'a str,
    /// Pinned flag.
    pub pinned: bool,
    /// Folder, if any.
    pub folder_id: Option<Uuid>,
    /// Owner.
    pub creator_id: Uuid,
}

/// The fields an `UPDATE notes` needs.
#[derive(Debug, Clone, Copy)]
pub struct UpdateNoteInput<'a> {
    /// New title, or `None` to leave it.
    pub title: Option<&'a str>,
    /// New body, or `None` to leave it.
    pub content: Option<&'a str>,
    /// New pinned flag, or `None` to leave it.
    pub pinned: Option<bool>,
    /// The folder to write. Written unconditionally, as `master` did.
    pub folder_id: Option<Uuid>,
    /// The note to update.
    pub note_id: Uuid,
    /// Owner.
    pub creator_id: Uuid,
    /// Optimistic-concurrency token.
    pub base_version: i32,
}

/// The fields a new `folders` row needs.
#[derive(Debug, Clone, Copy)]
pub struct CreateFolderInput<'a> {
    /// The client-chosen id.
    pub id: Uuid,
    /// Title.
    pub title: &'a str,
    /// Pinned flag.
    pub pinned: bool,
    /// Owner.
    pub creator_id: Uuid,
}

/// The fields an `UPDATE folders` needs.
#[derive(Debug, Clone, Copy)]
pub struct UpdateFolderInput<'a> {
    /// New title, or `None` to leave it.
    pub title: Option<&'a str>,
    /// New pinned flag, or `None` to leave it.
    pub pinned: Option<bool>,
    /// The folder to update.
    pub folder_id: Uuid,
    /// Owner.
    pub creator_id: Uuid,
    /// Optimistic-concurrency token.
    pub base_version: i32,
}
