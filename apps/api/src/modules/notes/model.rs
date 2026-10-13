//! The notes module's rows, and the mutation language its offline sync speaks.
//!
//! # What changed from `master`
//!
//! - **The limits live next to the columns they mirror.** `master` spelled the title and
//!   content ceilings out inside `validator` attributes in `dto.rs`, so the SQL column
//!   widths and the validation messages could drift apart. They are `const`s here, and
//!   `repository::pg` reads the same columns they describe.
//! - **`is_deleted` is derived from `deleted_at`.** `master` kept a `deleted_at` column
//!   and also a DTO-level `deleted` boolean computed at the edge; the row is the fact,
//!   and it is exposed once.
//! - **The sync mutation types are the module's own.** They were DTOs on `master`, so the
//!   repository had to reach into the wire layer to describe a batch. They are values the
//!   module reasons about, so they live here and `dto` re-exports them for the boundary.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// The longest title the API accepts, in characters.
///
/// Mirror of `notes.title VARCHAR(100)` and `folders.title VARCHAR(100)`: a title that
/// passes validation must not be refused by the database with a 500.
pub const MAX_TITLE_CHARS: usize = 100;

/// The longest note body the API accepts, in characters.
///
/// Mirror of `notes.content VARCHAR(10000)`.
pub const MAX_CONTENT_CHARS: usize = 10_000;

/// The most mutations a single `POST /notes/sync` may carry.
///
/// A ceiling rather than a policy: the whole batch is applied under one transaction, and
/// an unbounded batch is an unbounded lock. `master` used the same number.
pub const MAX_MUTATIONS_PER_PUSH: usize = 200;

/// The most notes one `PUT /notes` (move) may address at once.
///
/// Mirror of the `noteIds` validation rule, and of the batch size the move statement is
/// sized for.
pub const MAX_NOTES_PER_MOVE: usize = 200;

/// A note as the module reads it.
///
/// `FromRow`-able directly: every column a note has is a column this type carries, so
/// there is no join to flatten into it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Note {
    /// Primary key. The client chooses it, so an offline-created note keeps its id.
    pub id: Uuid,
    /// Title, 0–[`MAX_TITLE_CHARS`] characters.
    pub title: String,
    /// Body, 0–[`MAX_CONTENT_CHARS`] characters.
    pub content: String,
    /// Whether the note is pinned to the top of a feed.
    pub pinned: bool,
    /// The folder it is filed in, if any.
    pub folder_id: Option<Uuid>,
    /// Who owns it.
    pub creator_id: Uuid,
    /// Optimistic-concurrency counter. Incremented by every write.
    pub version: i32,
    /// When it was created.
    pub created_at: OffsetDateTime,
    /// When it was last written, maintained by the `notes_set_updated_at` trigger.
    pub updated_at: OffsetDateTime,
    /// When it was soft-deleted, if it was.
    pub deleted_at: Option<OffsetDateTime>,
}

impl Note {
    /// Whether the note has been soft-deleted.
    ///
    /// Derived from the column rather than stored: a second field would be a second
    /// answer, and the two would eventually disagree.
    #[must_use]
    pub const fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// A folder as the module reads it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Folder {
    /// Primary key.
    pub id: Uuid,
    /// Title, 1–[`MAX_TITLE_CHARS`] characters.
    pub title: String,
    /// Whether the folder is pinned to the top of the list.
    pub pinned: bool,
    /// Who owns it.
    pub creator_id: Uuid,
    /// Optimistic-concurrency counter.
    pub version: i32,
    /// When it was created.
    pub created_at: OffsetDateTime,
    /// When it was last written, maintained by the `folders_set_updated_at` trigger.
    pub updated_at: OffsetDateTime,
    /// When it was soft-deleted, if it was.
    pub deleted_at: Option<OffsetDateTime>,
}

impl Folder {
    /// Whether the folder has been soft-deleted.
    #[must_use]
    pub const fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// Which kind of entity a sync mutation or result concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    /// A note.
    Note,
    /// A folder.
    Folder,
}

/// A note's complete desired state, as an offline client reports it.
///
/// Full-snapshot semantics: this is the whole note, not a delta. `base_version` is the
/// only concurrency control — `None` means \"this id has never reached the server\", and
/// `Some(v)` means \"apply only if the row is still at `v`\".
#[derive(Debug, Clone, Deserialize)]
pub struct NoteMutation {
    /// The client-chosen id.
    pub id: Uuid,
    /// Desired title.
    pub title: String,
    /// Desired body.
    pub content: String,
    /// Desired pinned flag.
    pub pinned: bool,
    /// Desired folder.
    pub folder_id: Option<Uuid>,
    /// Desired deleted state.
    pub deleted: bool,
    /// The last version the client saw, or `None` for a never-synced id.
    pub base_version: Option<i32>,
    /// The client's wall clock when the mutation was made.
    ///
    /// Used **only** as the last-write-wins tiebreaker when an id collides on create;
    /// clock skew is real, so it is trusted for nothing else.
    #[serde(with = "time::serde::rfc3339")]
    pub client_updated_at: OffsetDateTime,
}

/// A folder's complete desired state, as an offline client reports it.
#[derive(Debug, Clone, Deserialize)]
pub struct FolderMutation {
    /// The client-chosen id.
    pub id: Uuid,
    /// Desired title.
    pub title: String,
    /// Desired pinned flag.
    pub pinned: bool,
    /// Desired deleted state.
    pub deleted: bool,
    /// The last version the client saw, or `None` for a never-synced id.
    pub base_version: Option<i32>,
    /// The client's wall clock when the mutation was made.
    #[serde(with = "time::serde::rfc3339")]
    pub client_updated_at: OffsetDateTime,
}

/// One entry in a sync push, tagged by which entity it concerns.
///
/// Internally tagged on `entityType` so one mutation log can interleave notes and folders
/// in whatever order the client's queue holds them.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "entityType", rename_all = "snake_case")]
pub enum SyncMutation {
    /// A note mutation.
    Note(NoteMutation),
    /// A folder mutation.
    Folder(FolderMutation),
}

/// A push, split by type and carrying each item's original index.
///
/// Folders are applied before notes within one batch, so a note that references a folder
/// created in the same batch resolves — regardless of how the client interleaved the
/// array. The index is kept so results can be restored to submission order, which is what
/// lets a client zip them back against its local queue positionally.
#[derive(Debug, Default)]
pub struct SyncBatch {
    /// Folder mutations, in submission order, with their original index.
    pub folders: Vec<(usize, FolderMutation)>,
    /// Note mutations, in submission order, with their original index.
    pub notes: Vec<(usize, NoteMutation)>,
}

impl SyncBatch {
    /// Split a request's mutations into the two ordered groups.
    #[must_use]
    pub fn split(mutations: Vec<SyncMutation>) -> Self {
        let mut batch = Self::default();
        for (index, mutation) in mutations.into_iter().enumerate() {
            match mutation {
                SyncMutation::Folder(folder) => batch.folders.push((index, folder)),
                SyncMutation::Note(note) => batch.notes.push((index, note)),
            }
        }
        batch
    }

    /// How many mutations the batch carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.folders.len() + self.notes.len()
    }

    /// Whether the batch carries nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty() && self.notes.is_empty()
    }
}

/// The current state of an entity a mutation touched.
#[derive(Debug, Clone)]
pub enum SyncEntity {
    /// A note.
    Note(Note),
    /// A folder.
    Folder(Folder),
}

impl SyncEntity {
    /// The entity's id.
    #[must_use]
    pub fn id(&self) -> Uuid {
        match self {
            Self::Note(note) => note.id,
            Self::Folder(folder) => folder.id,
        }
    }

    /// Which kind of entity it is.
    #[must_use]
    pub const fn entity_type(&self) -> EntityType {
        match self {
            Self::Note(_) => EntityType::Note,
            Self::Folder(_) => EntityType::Folder,
        }
    }
}

/// What happened to one mutation in a batch.
#[derive(Debug, Clone)]
pub enum MutationResolution {
    /// Written (or correctly determined nothing needed writing).
    ///
    /// `entity` is `None` for a create-then-delete-while-offline mutation, which is
    /// acknowledged without a row ever existing.
    Applied {
        /// The row as written, when there is one.
        entity: Option<SyncEntity>,
    },
    /// `base_version` was stale; `entity` is the server's current copy.
    Conflict {
        /// The row the server holds now.
        entity: SyncEntity,
    },
    /// Failed per-item validation. Does not fail the rest of the batch.
    Rejected {
        /// Why it was refused.
        reason: String,
    },
}

/// One mutation's outcome, tagged with its position in the submission.
#[derive(Debug, Clone)]
pub struct ResolvedMutation {
    /// The mutation's index in the request array.
    pub index: usize,
    /// What happened to it.
    pub resolution: MutationResolution,
}

#[cfg(test)]
mod tests {
    //! The batch splitter and the derived delete flag.

    use super::*;

    fn note_mutation(id: u128) -> NoteMutation {
        NoteMutation {
            id: Uuid::from_u128(id),
            title: "t".to_owned(),
            content: "c".to_owned(),
            pinned: false,
            folder_id: None,
            deleted: false,
            base_version: None,
            client_updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn folder_mutation(id: u128) -> FolderMutation {
        FolderMutation {
            id: Uuid::from_u128(id),
            title: "t".to_owned(),
            pinned: false,
            deleted: false,
            base_version: None,
            client_updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn split_keeps_each_mutations_original_index() {
        // The index is what lets a client zip results back positionally; losing it is
        // losing the contract.
        let batch = SyncBatch::split(vec![
            SyncMutation::Note(note_mutation(1)),
            SyncMutation::Folder(folder_mutation(2)),
            SyncMutation::Note(note_mutation(3)),
        ]);

        assert_eq!(
            batch.folders.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            batch.notes.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(batch.len(), 3);
        assert!(!batch.is_empty());
    }

    #[test]
    fn an_empty_push_splits_into_empty_groups() {
        let batch = SyncBatch::split(vec![]);
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }

    #[test]
    fn is_deleted_tracks_the_column() {
        let mut note = Note {
            id: Uuid::from_u128(1),
            title: String::new(),
            content: String::new(),
            pinned: false,
            folder_id: None,
            creator_id: Uuid::from_u128(2),
            version: 1,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            deleted_at: None,
        };
        assert!(!note.is_deleted());

        note.deleted_at = Some(OffsetDateTime::UNIX_EPOCH);
        assert!(note.is_deleted());
    }

    #[test]
    fn sync_entity_reports_its_type_and_id() {
        let note = Note {
            id: Uuid::from_u128(7),
            title: String::new(),
            content: String::new(),
            pinned: false,
            folder_id: None,
            creator_id: Uuid::from_u128(2),
            version: 1,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            deleted_at: None,
        };
        let entity = SyncEntity::Note(note);
        assert_eq!(entity.id(), Uuid::from_u128(7));
        assert_eq!(entity.entity_type(), EntityType::Note);
    }
}
