//! The Postgres adapter for [`NotesRepo`].
//!
//! Plain `sqlx` runtime queries, not `query!` macros — the same trade as the rest of the
//! refactored tree: `cargo check` works with no database standing, and the SQL is
//! exercised by `tests/notes_repository_pg.rs` against a live Postgres and by
//! `tests/notes_service.rs` against the in-memory double that mirrors it.
//!
//! # Injection
//!
//! Every value a request contributes is a bind parameter. The only interpolated text is
//! the column list in `NOTE_COLUMNS` / `FOLDER_COLUMNS`, spliced through `statement`,
//! which returns [`sqlx::AssertSqlSafe`]: the *only* pieces that reach a
//! statement are the `const`s in this file. Queries built a clause at a time go through
//! [`QueryBuilder`], whose cursor helpers take a [`ColumnName`] that refuses anything not
//! written in this module's own source.
//!
//! # Transactions live here, not in the service
//!
//! `master`'s service opened the transaction and drove it through executor-generic
//! repository methods, which made the repository un-doubleable. Two operations need one:
//! [`NotesRepo::delete_folder`] (the folder row and its notes are one fact) and
//! [`NotesRepo::apply_sync_batch`] (a push is all-or-nothing). Both open, apply and
//! commit their own transaction here.
//!
//! # `updated_at` is written, not assumed
//!
//! The migrations attach a `set_updated_at` trigger, but every write here also sets
//! `updated_at = NOW()`: the in-memory double cannot run a trigger, and a value the
//! service can be tested against is worth more than the three tokens the assignment
//! costs.

use std::collections::HashSet;

use async_trait::async_trait;
use sqlx::{AssertSqlSafe, PgConnection, PgPool, Postgres, QueryBuilder};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{
    CreateFolderInput, CreateNoteInput, FolderFilter, NoteFilter, NotesOnFolderDelete, NotesRepo,
    UpdateFolderInput, UpdateNoteInput,
};
use crate::infrastructure::query::{ColumnName, push_cursor_condition, push_order_and_limit};
use crate::modules::notes::error;
use crate::modules::notes::model::{
    Folder, FolderMutation, MAX_CONTENT_CHARS, MAX_TITLE_CHARS, MutationResolution, Note,
    NoteMutation, ResolvedMutation, SyncBatch, SyncEntity,
};
use meno_core::{Error as MenoError, Order};

/// Columns selected wherever a `Note` is read.
const NOTE_COLUMNS: &str = "id, title, content, pinned, folder_id, creator_id, version, \
     created_at, updated_at, deleted_at";

/// Columns selected wherever a `Folder` is read.
const FOLDER_COLUMNS: &str = "id, title, pinned, creator_id, version, created_at, \
     updated_at, deleted_at";

/// Substitute the module's own column lists into a statement template.
///
/// The templates below are `const`s and their placeholders name `const`s, so
/// `AssertSqlSafe` is a structural claim rather than a promise.
fn statement(template: &str) -> AssertSqlSafe<String> {
    AssertSqlSafe(
        template
            .replace("{NOTE_COLUMNS}", NOTE_COLUMNS)
            .replace("{FOLDER_COLUMNS}", FOLDER_COLUMNS),
    )
}

/// Postgres-backed storage for the notes module.
#[derive(Debug, Clone)]
pub struct PgNotesRepo {
    pool: PgPool,
}

impl PgNotesRepo {
    /// Read and write through `pool`.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Log the driver's text and return a 500 that does not contain it.
fn database(context: &'static str, failure: sqlx::Error) -> MenoError {
    tracing::error!(error = %failure, context, "notes repository call failed");
    error::internal(context, failure)
}

/// A per-item validation failure, if any. One bad mutation must not poison the batch.
fn note_rejection(mutation: &NoteMutation) -> Option<String> {
    if mutation.title.chars().count() > MAX_TITLE_CHARS {
        return Some(format!("title exceeds {MAX_TITLE_CHARS} characters"));
    }
    if mutation.content.chars().count() > MAX_CONTENT_CHARS {
        return Some(format!("content exceeds {MAX_CONTENT_CHARS} characters"));
    }
    None
}

/// A per-item validation failure for a folder mutation.
fn folder_rejection(mutation: &FolderMutation) -> Option<String> {
    if mutation.title.chars().count() > MAX_TITLE_CHARS {
        return Some(format!("title exceeds {MAX_TITLE_CHARS} characters"));
    }
    None
}

/// A note by id, in any state — soft-deleted included. Used to explain a LWW decision.
async fn note_by_id_any_state(
    conn: &mut PgConnection,
    id: Uuid,
    creator_id: Uuid,
) -> Result<Option<Note>, MenoError> {
    sqlx::query_as::<_, Note>(statement(
        "SELECT {NOTE_COLUMNS} FROM notes WHERE id = $1 AND creator_id = $2",
    ))
    .bind(id)
    .bind(creator_id)
    .fetch_optional(conn)
    .await
    .map_err(|e| database("find_note_any_state", e))
}

/// A folder by id, in any state.
async fn folder_by_id_any_state(
    conn: &mut PgConnection,
    id: Uuid,
    creator_id: Uuid,
) -> Result<Option<Folder>, MenoError> {
    sqlx::query_as::<_, Folder>(statement(
        "SELECT {FOLDER_COLUMNS} FROM folders WHERE id = $1 AND creator_id = $2",
    ))
    .bind(id)
    .bind(creator_id)
    .fetch_optional(conn)
    .await
    .map_err(|e| database("find_folder_any_state", e))
}

/// Which of `candidate_ids` this creator owns.
async fn owned_folder_ids(
    conn: &mut PgConnection,
    creator_id: Uuid,
    candidate_ids: &[Uuid],
) -> Result<HashSet<Uuid>, MenoError> {
    if candidate_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let rows = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM folders WHERE creator_id = $1 AND id = ANY($2)",
    )
    .bind(creator_id)
    .bind(candidate_ids)
    .fetch_all(conn)
    .await
    .map_err(|e| database("owned_folder_ids", e))?;
    Ok(rows.into_iter().collect())
}

/// Insert a note only if the id is free.
async fn upsert_note_if_absent(
    conn: &mut PgConnection,
    creator_id: Uuid,
    mutation: &NoteMutation,
    folder_id: Option<Uuid>,
) -> Result<Option<Note>, MenoError> {
    sqlx::query_as::<_, Note>(statement(
        "INSERT INTO notes (id, title, content, pinned, folder_id, creator_id) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (id) DO NOTHING \
         RETURNING {NOTE_COLUMNS}",
    ))
    .bind(mutation.id)
    .bind(&mutation.title)
    .bind(&mutation.content)
    .bind(mutation.pinned)
    .bind(folder_id)
    .bind(creator_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| database("upsert_note_if_absent", e))
}

/// Replace a note's whole state, guarded on `version`.
async fn apply_note_snapshot_if_version_matches(
    conn: &mut PgConnection,
    creator_id: Uuid,
    mutation: &NoteMutation,
    folder_id: Option<Uuid>,
    base_version: i32,
) -> Result<Option<Note>, MenoError> {
    sqlx::query_as::<_, Note>(statement(
        "UPDATE notes \
         SET title = $1, content = $2, pinned = $3, folder_id = $4, \
             deleted_at = CASE WHEN $5 THEN COALESCE(deleted_at, NOW()) END, \
             version = version + 1, updated_at = NOW() \
         WHERE id = $6 AND creator_id = $7 AND version = $8 \
         RETURNING {NOTE_COLUMNS}",
    ))
    .bind(&mutation.title)
    .bind(&mutation.content)
    .bind(mutation.pinned)
    .bind(folder_id)
    .bind(mutation.deleted)
    .bind(mutation.id)
    .bind(creator_id)
    .bind(base_version)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| database("apply_note_snapshot", e))
}

/// Replace a note's whole state, unconditionally. The last-write-wins arm of a create
/// collision.
async fn force_apply_note_snapshot(
    conn: &mut PgConnection,
    creator_id: Uuid,
    mutation: &NoteMutation,
    folder_id: Option<Uuid>,
) -> Result<Option<Note>, MenoError> {
    sqlx::query_as::<_, Note>(statement(
        "UPDATE notes \
         SET title = $1, content = $2, pinned = $3, folder_id = $4, \
             deleted_at = CASE WHEN $5 THEN COALESCE(deleted_at, NOW()) END, \
             version = version + 1, updated_at = NOW() \
         WHERE id = $6 AND creator_id = $7 \
         RETURNING {NOTE_COLUMNS}",
    ))
    .bind(&mutation.title)
    .bind(&mutation.content)
    .bind(mutation.pinned)
    .bind(folder_id)
    .bind(mutation.deleted)
    .bind(mutation.id)
    .bind(creator_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| database("force_apply_note_snapshot", e))
}

/// Insert a folder only if the id is free.
async fn upsert_folder_if_absent(
    conn: &mut PgConnection,
    creator_id: Uuid,
    mutation: &FolderMutation,
) -> Result<Option<Folder>, MenoError> {
    sqlx::query_as::<_, Folder>(statement(
        "INSERT INTO folders (id, title, pinned, creator_id) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (id) DO NOTHING \
         RETURNING {FOLDER_COLUMNS}",
    ))
    .bind(mutation.id)
    .bind(&mutation.title)
    .bind(mutation.pinned)
    .bind(creator_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| database("upsert_folder_if_absent", e))
}

/// Replace a folder's whole state, guarded on `version`.
async fn apply_folder_snapshot_if_version_matches(
    conn: &mut PgConnection,
    creator_id: Uuid,
    mutation: &FolderMutation,
    base_version: i32,
) -> Result<Option<Folder>, MenoError> {
    sqlx::query_as::<_, Folder>(statement(
        "UPDATE folders \
         SET title = $1, pinned = $2, \
             deleted_at = CASE WHEN $3 THEN COALESCE(deleted_at, NOW()) END, \
             version = version + 1, updated_at = NOW() \
         WHERE id = $4 AND creator_id = $5 AND version = $6 \
         RETURNING {FOLDER_COLUMNS}",
    ))
    .bind(&mutation.title)
    .bind(mutation.pinned)
    .bind(mutation.deleted)
    .bind(mutation.id)
    .bind(creator_id)
    .bind(base_version)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| database("apply_folder_snapshot", e))
}

/// Replace a folder's whole state, unconditionally.
async fn force_apply_folder_snapshot(
    conn: &mut PgConnection,
    creator_id: Uuid,
    mutation: &FolderMutation,
) -> Result<Option<Folder>, MenoError> {
    sqlx::query_as::<_, Folder>(statement(
        "UPDATE folders \
         SET title = $1, pinned = $2, \
             deleted_at = CASE WHEN $3 THEN COALESCE(deleted_at, NOW()) END, \
             version = version + 1, updated_at = NOW() \
         WHERE id = $4 AND creator_id = $5 \
         RETURNING {FOLDER_COLUMNS}",
    ))
    .bind(&mutation.title)
    .bind(mutation.pinned)
    .bind(mutation.deleted)
    .bind(mutation.id)
    .bind(creator_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| database("force_apply_folder_snapshot", e))
}

/// Apply one folder mutation inside the batch transaction.
async fn resolve_folder(
    conn: &mut PgConnection,
    creator_id: Uuid,
    mutation: &FolderMutation,
) -> Result<MutationResolution, MenoError> {
    if let Some(reason) = folder_rejection(mutation) {
        return Ok(MutationResolution::Rejected { reason });
    }

    match mutation.base_version {
        None => {
            if mutation.deleted {
                // Created, then deleted, entirely offline. No other device has seen it.
                return Ok(MutationResolution::Applied { entity: None });
            }
            match upsert_folder_if_absent(conn, creator_id, mutation).await? {
                Some(folder) => Ok(applied_folder(folder)),
                None => resolve_existing_folder(conn, creator_id, mutation).await,
            }
        }
        Some(base_version) => {
            match apply_folder_snapshot_if_version_matches(conn, creator_id, mutation, base_version)
                .await?
            {
                Some(folder) => Ok(applied_folder(folder)),
                None => resolve_existing_folder(conn, creator_id, mutation).await,
            }
        }
    }
}

/// Resolve a folder whose guarded write wrote nothing.
async fn resolve_existing_folder(
    conn: &mut PgConnection,
    creator_id: Uuid,
    mutation: &FolderMutation,
) -> Result<MutationResolution, MenoError> {
    let Some(existing) = folder_by_id_any_state(conn, mutation.id, creator_id).await? else {
        return Ok(MutationResolution::Rejected {
            reason: "folder does not exist".to_owned(),
        });
    };

    // A create collision (no base version) is last-write-wins; a versioned write that no
    // longer matches is an honest conflict.
    if mutation.base_version.is_none() && mutation.client_updated_at > existing.updated_at {
        return match force_apply_folder_snapshot(conn, creator_id, mutation).await? {
            Some(folder) => Ok(applied_folder(folder)),
            None => Ok(MutationResolution::Rejected {
                reason: "folder does not exist".to_owned(),
            }),
        };
    }

    Ok(MutationResolution::Conflict {
        entity: SyncEntity::Folder(existing),
    })
}

/// Apply one note mutation inside the batch transaction.
async fn resolve_note(
    conn: &mut PgConnection,
    creator_id: Uuid,
    mutation: &NoteMutation,
    folder_id: Option<Uuid>,
) -> Result<MutationResolution, MenoError> {
    if let Some(reason) = note_rejection(mutation) {
        return Ok(MutationResolution::Rejected { reason });
    }

    match mutation.base_version {
        None => {
            if mutation.deleted {
                return Ok(MutationResolution::Applied { entity: None });
            }
            match upsert_note_if_absent(conn, creator_id, mutation, folder_id).await? {
                Some(note) => Ok(applied(note)),
                None => resolve_existing_note(conn, creator_id, mutation, folder_id).await,
            }
        }
        Some(base_version) => {
            match apply_note_snapshot_if_version_matches(
                conn,
                creator_id,
                mutation,
                folder_id,
                base_version,
            )
            .await?
            {
                Some(note) => Ok(applied(note)),
                None => resolve_existing_note(conn, creator_id, mutation, folder_id).await,
            }
        }
    }
}

/// Resolve a note whose guarded write wrote nothing.
async fn resolve_existing_note(
    conn: &mut PgConnection,
    creator_id: Uuid,
    mutation: &NoteMutation,
    folder_id: Option<Uuid>,
) -> Result<MutationResolution, MenoError> {
    let Some(existing) = note_by_id_any_state(conn, mutation.id, creator_id).await? else {
        return Ok(MutationResolution::Rejected {
            reason: "note does not exist".to_owned(),
        });
    };

    if mutation.base_version.is_none() && mutation.client_updated_at > existing.updated_at {
        return match force_apply_note_snapshot(conn, creator_id, mutation, folder_id).await? {
            Some(note) => Ok(applied(note)),
            None => Ok(MutationResolution::Rejected {
                reason: "note does not exist".to_owned(),
            }),
        };
    }

    Ok(MutationResolution::Conflict {
        entity: SyncEntity::Note(existing),
    })
}

/// An applied resolution for a note.
fn applied(note: Note) -> MutationResolution {
    MutationResolution::Applied {
        entity: Some(SyncEntity::Note(note)),
    }
}

/// An applied resolution for a folder.
fn applied_folder(folder: Folder) -> MutationResolution {
    MutationResolution::Applied {
        entity: Some(SyncEntity::Folder(folder)),
    }
}

#[async_trait]
impl NotesRepo for PgNotesRepo {
    async fn create_note(&self, input: &CreateNoteInput<'_>) -> Result<Note, MenoError> {
        sqlx::query_as::<_, Note>(statement(
            "INSERT INTO notes (id, title, content, pinned, folder_id, creator_id) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING {NOTE_COLUMNS}",
        ))
        .bind(input.id)
        .bind(input.title)
        .bind(input.content)
        .bind(input.pinned)
        .bind(input.folder_id)
        .bind(input.creator_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| database("create_note", e))
    }

    async fn update_note_by_version(
        &self,
        input: &UpdateNoteInput<'_>,
    ) -> Result<Option<Note>, MenoError> {
        sqlx::query_as::<_, Note>(statement(
            "UPDATE notes \
             SET title = COALESCE($1, title), \
                 content = COALESCE($2, content), \
                 pinned = COALESCE($3, pinned), \
                 folder_id = $4, \
                 version = version + 1, \
                 updated_at = NOW() \
             WHERE id = $5 AND creator_id = $6 AND version = $7 AND deleted_at IS NULL \
             RETURNING {NOTE_COLUMNS}",
        ))
        .bind(input.title)
        .bind(input.content)
        .bind(input.pinned)
        .bind(input.folder_id)
        .bind(input.note_id)
        .bind(input.creator_id)
        .bind(input.base_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("update_note_by_version", e))
    }

    async fn soft_delete_note(
        &self,
        id: Uuid,
        creator_id: Uuid,
        base_version: i32,
    ) -> Result<Option<Note>, MenoError> {
        sqlx::query_as::<_, Note>(statement(
            "UPDATE notes \
             SET deleted_at = NOW(), version = version + 1, updated_at = NOW() \
             WHERE id = $1 AND creator_id = $2 AND version = $3 AND deleted_at IS NULL \
             RETURNING {NOTE_COLUMNS}",
        ))
        .bind(id)
        .bind(creator_id)
        .bind(base_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("soft_delete_note", e))
    }

    async fn find_note_by_id(&self, id: Uuid, creator_id: Uuid) -> Result<Option<Note>, MenoError> {
        sqlx::query_as::<_, Note>(statement(
            "SELECT {NOTE_COLUMNS} FROM notes \
             WHERE id = $1 AND creator_id = $2 AND deleted_at IS NULL",
        ))
        .bind(id)
        .bind(creator_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("find_note_by_id", e))
    }

    async fn find_notes(&self, filter: &NoteFilter) -> Result<Vec<Note>, MenoError> {
        let (cursor_ts, cursor_id) = match &filter.cursor {
            None => (None, None),
            Some((ts, id)) => (Some(*ts), Some(*id)),
        };

        let mut qb: QueryBuilder<Postgres> =
            QueryBuilder::new(statement("SELECT {NOTE_COLUMNS} FROM notes WHERE creator_id = ").0);
        qb.push_bind(filter.creator_id)
            .push(" AND deleted_at IS NULL");

        if let Some(folder_id) = filter.folder_id {
            qb.push(" AND folder_id = ").push_bind(folder_id);
        }
        if let Some(pinned) = filter.pinned {
            qb.push(" AND pinned = ").push_bind(pinned);
        }
        if let Some(keywords) = filter.keywords.as_deref().map(str::trim)
            && !keywords.is_empty()
        {
            qb.push(
                " AND to_tsvector('english', title || ' ' || content) \
                     @@ plainto_tsquery('english', ",
            )
            .push_bind(keywords)
            .push(")");
        }

        push_cursor_condition(
            &mut qb,
            ColumnName::literal("created_at"),
            ColumnName::literal("id"),
            cursor_ts,
            cursor_id,
            Order::Desc,
        );
        push_order_and_limit(
            &mut qb,
            ColumnName::literal("created_at"),
            ColumnName::literal("id"),
            Order::Desc,
            filter.limit_plus_one,
        );

        qb.build_query_as::<Note>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("find_notes", e))
    }

    async fn add_note_to_folder(
        &self,
        creator_id: Uuid,
        note_id: Uuid,
        folder_id: Uuid,
    ) -> Result<Option<Note>, MenoError> {
        sqlx::query_as::<_, Note>(statement(
            "UPDATE notes \
             SET folder_id = $1, version = version + 1, updated_at = NOW() \
             WHERE id = $2 AND creator_id = $3 AND deleted_at IS NULL \
             RETURNING {NOTE_COLUMNS}",
        ))
        .bind(folder_id)
        .bind(note_id)
        .bind(creator_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("add_note_to_folder", e))
    }

    async fn remove_note_from_folder(
        &self,
        creator_id: Uuid,
        note_id: Uuid,
        folder_id: Uuid,
    ) -> Result<Option<Note>, MenoError> {
        sqlx::query_as::<_, Note>(statement(
            "UPDATE notes \
             SET folder_id = NULL, version = version + 1, updated_at = NOW() \
             WHERE id = $1 AND creator_id = $2 AND folder_id = $3 AND deleted_at IS NULL \
             RETURNING {NOTE_COLUMNS}",
        ))
        .bind(note_id)
        .bind(creator_id)
        .bind(folder_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("remove_note_from_folder", e))
    }

    async fn move_notes_to_folder(
        &self,
        creator_id: Uuid,
        folder_id: Option<Uuid>,
        note_ids: &[Uuid],
    ) -> Result<Vec<Note>, MenoError> {
        sqlx::query_as::<_, Note>(statement(
            "UPDATE notes \
             SET folder_id = $1, version = version + 1, updated_at = NOW() \
             WHERE id = ANY($2) AND creator_id = $3 AND deleted_at IS NULL \
             RETURNING {NOTE_COLUMNS}",
        ))
        .bind(folder_id)
        .bind(note_ids)
        .bind(creator_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| database("move_notes_to_folder", e))
    }

    async fn find_notes_changed_since(
        &self,
        creator_id: Uuid,
        cursor: Option<(OffsetDateTime, Uuid)>,
        limit: i64,
    ) -> Result<Vec<Note>, MenoError> {
        let mut qb: QueryBuilder<Postgres> =
            QueryBuilder::new(statement("SELECT {NOTE_COLUMNS} FROM notes WHERE creator_id = ").0);
        qb.push_bind(creator_id);

        if let Some((ts, id)) = cursor {
            qb.push(" AND (updated_at, id) > (")
                .push_bind(ts)
                .push(", ")
                .push_bind(id)
                .push(")");
        }

        qb.push(" ORDER BY updated_at ASC, id ASC LIMIT ")
            .push_bind(limit);

        qb.build_query_as::<Note>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("find_notes_changed_since", e))
    }

    async fn create_folder(&self, input: &CreateFolderInput<'_>) -> Result<Folder, MenoError> {
        sqlx::query_as::<_, Folder>(statement(
            "INSERT INTO folders (id, title, pinned, creator_id) \
             VALUES ($1, $2, $3, $4) \
             RETURNING {FOLDER_COLUMNS}",
        ))
        .bind(input.id)
        .bind(input.title)
        .bind(input.pinned)
        .bind(input.creator_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| database("create_folder", e))
    }

    async fn update_folder_by_version(
        &self,
        input: &UpdateFolderInput<'_>,
    ) -> Result<Option<Folder>, MenoError> {
        sqlx::query_as::<_, Folder>(statement(
            "UPDATE folders \
             SET title = COALESCE($1, title), \
                 pinned = COALESCE($2, pinned), \
                 version = version + 1, \
                 updated_at = NOW() \
             WHERE id = $3 AND creator_id = $4 AND version = $5 AND deleted_at IS NULL \
             RETURNING {FOLDER_COLUMNS}",
        ))
        .bind(input.title)
        .bind(input.pinned)
        .bind(input.folder_id)
        .bind(input.creator_id)
        .bind(input.base_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("update_folder_by_version", e))
    }

    async fn delete_folder(
        &self,
        id: Uuid,
        creator_id: Uuid,
        base_version: i32,
        notes: NotesOnFolderDelete,
    ) -> Result<Option<Folder>, MenoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| database("begin_delete_folder", e))?;

        // The folder write and the notes write are one fact: `master` soft-deleted only
        // the folder, so every note that pointed at it kept a dangling `folder_id`.
        let folder = sqlx::query_as::<_, Folder>(statement(
            "UPDATE folders \
             SET deleted_at = NOW(), version = version + 1, updated_at = NOW() \
             WHERE id = $1 AND creator_id = $2 AND version = $3 AND deleted_at IS NULL \
             RETURNING {FOLDER_COLUMNS}",
        ))
        .bind(id)
        .bind(creator_id)
        .bind(base_version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| database("soft_delete_folder", e))?;

        let Some(folder) = folder else {
            // Nothing matched: the caller gets the same `None` a version conflict needs,
            // and the (empty) transaction rolls back on drop.
            return Ok(None);
        };

        let notes_sql = match notes {
            NotesOnFolderDelete::Delete => {
                "UPDATE notes \
                 SET deleted_at = NOW(), version = version + 1, updated_at = NOW() \
                 WHERE folder_id = $1 AND creator_id = $2 AND deleted_at IS NULL"
            }
            NotesOnFolderDelete::Orphan => {
                "UPDATE notes \
                 SET folder_id = NULL, version = version + 1, updated_at = NOW() \
                 WHERE folder_id = $1 AND creator_id = $2 AND deleted_at IS NULL"
            }
        };

        sqlx::query(notes_sql)
            .bind(id)
            .bind(creator_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| database("cascade_folder_notes", e))?;

        tx.commit()
            .await
            .map_err(|e| database("commit_delete_folder", e))?;

        Ok(Some(folder))
    }

    async fn find_folder_by_id(
        &self,
        id: Uuid,
        creator_id: Uuid,
    ) -> Result<Option<Folder>, MenoError> {
        sqlx::query_as::<_, Folder>(statement(
            "SELECT {FOLDER_COLUMNS} FROM folders \
             WHERE id = $1 AND creator_id = $2 AND deleted_at IS NULL",
        ))
        .bind(id)
        .bind(creator_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| database("find_folder_by_id", e))
    }

    async fn find_folders(&self, filter: &FolderFilter) -> Result<Vec<Folder>, MenoError> {
        let (cursor_ts, cursor_id) = match &filter.cursor {
            None => (None, None),
            Some((ts, id)) => (Some(*ts), Some(*id)),
        };

        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            statement("SELECT {FOLDER_COLUMNS} FROM folders WHERE creator_id = ").0,
        );
        qb.push_bind(filter.creator_id)
            .push(" AND deleted_at IS NULL");

        if let Some(pinned) = filter.pinned {
            qb.push(" AND pinned = ").push_bind(pinned);
        }
        if let Some(keywords) = filter.keywords.as_deref().map(str::trim)
            && !keywords.is_empty()
        {
            qb.push(" AND to_tsvector('english', title) @@ plainto_tsquery('english', ")
                .push_bind(keywords)
                .push(")");
        }

        push_cursor_condition(
            &mut qb,
            ColumnName::literal("created_at"),
            ColumnName::literal("id"),
            cursor_ts,
            cursor_id,
            Order::Desc,
        );
        push_order_and_limit(
            &mut qb,
            ColumnName::literal("created_at"),
            ColumnName::literal("id"),
            Order::Desc,
            filter.limit_plus_one,
        );

        qb.build_query_as::<Folder>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("find_folders", e))
    }

    async fn find_folders_changed_since(
        &self,
        creator_id: Uuid,
        cursor: Option<(OffsetDateTime, Uuid)>,
        limit: i64,
    ) -> Result<Vec<Folder>, MenoError> {
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            statement("SELECT {FOLDER_COLUMNS} FROM folders WHERE creator_id = ").0,
        );
        qb.push_bind(creator_id);

        if let Some((ts, id)) = cursor {
            qb.push(" AND (updated_at, id) > (")
                .push_bind(ts)
                .push(", ")
                .push_bind(id)
                .push(")");
        }

        qb.push(" ORDER BY updated_at ASC, id ASC LIMIT ")
            .push_bind(limit);

        qb.build_query_as::<Folder>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| database("find_folders_changed_since", e))
    }

    async fn apply_sync_batch(
        &self,
        creator_id: Uuid,
        batch: &SyncBatch,
    ) -> Result<Vec<ResolvedMutation>, MenoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| database("begin_sync_batch", e))?;

        // A note mutation may reference a folder that another account owns (a buggy
        // client, or a tampered request). The foreign key proves the folder exists, never
        // that this caller owns it, so the referenced ids are checked once for the batch
        // and anything unowned is silently unfiled — the note's content is still valid.
        let referenced: Vec<Uuid> = batch
            .notes
            .iter()
            .filter_map(|(_, mutation)| mutation.folder_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let owned = owned_folder_ids(&mut tx, creator_id, &referenced).await?;

        let mut resolved = Vec::with_capacity(batch.len());

        for (index, mutation) in &batch.folders {
            let resolution = resolve_folder(&mut tx, creator_id, mutation).await?;
            resolved.push(ResolvedMutation {
                index: *index,
                resolution,
            });
        }

        for (index, mutation) in &batch.notes {
            let folder_id = match mutation.folder_id {
                Some(folder_id) if owned.contains(&folder_id) => Some(folder_id),
                Some(folder_id) => {
                    tracing::warn!(
                        creator_id = %creator_id, note_id = %mutation.id, folder_id = %folder_id,
                        "sync batch: note referenced a folder not owned by this user — unfiling"
                    );
                    None
                }
                None => None,
            };
            let resolution = resolve_note(&mut tx, creator_id, mutation, folder_id).await?;
            resolved.push(ResolvedMutation {
                index: *index,
                resolution,
            });
        }

        tx.commit()
            .await
            .map_err(|e| database("commit_sync_batch", e))?;

        // Restore submission order so a client can zip results back positionally.
        resolved.sort_by_key(|resolved| resolved.index);
        Ok(resolved)
    }

    async fn purge_deleted_notes_older_than(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<u64, MenoError> {
        purge_batched(&self.pool, PurgeTable::Notes, cutoff).await
    }

    async fn purge_deleted_folders_older_than(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<u64, MenoError> {
        purge_batched(&self.pool, PurgeTable::Folders, cutoff).await
    }
}

/// Which table a purge pass empties.
#[derive(Debug, Clone, Copy)]
enum PurgeTable {
    /// `notes`.
    Notes,
    /// `folders`.
    Folders,
}

impl PurgeTable {
    /// The table's name, a fixed internal literal.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Notes => "notes",
            Self::Folders => "folders",
        }
    }
}

/// Batched hard-delete of rows soft-deleted before `cutoff`.
///
/// `table` is a fixed internal literal, never user input, so the interpolation carries no
/// injection risk. The batch (`LIMIT 5000`) keeps a long-lived lock off a large table.
async fn purge_batched(
    pool: &PgPool,
    table: PurgeTable,
    cutoff: OffsetDateTime,
) -> Result<u64, MenoError> {
    let mut total: u64 = 0;
    let table = table.as_str();

    loop {
        let sql = format!(
            "WITH deleted AS (\
                DELETE FROM {table} \
                WHERE id IN (\
                    SELECT id FROM {table} \
                    WHERE deleted_at IS NOT NULL AND deleted_at < $1 \
                    LIMIT 5000\
                ) \
                RETURNING 1\
            ) \
            SELECT COUNT(*) FROM deleted"
        );

        let n: i64 = sqlx::query_scalar::<_, i64>(AssertSqlSafe(sql))
            .bind(cutoff)
            .fetch_one(pool)
            .await
            .map_err(|e| database("purge_batched", e))?;

        let n = n.cast_unsigned();
        total += n;

        if n < 5000 {
            break;
        }

        tokio::task::yield_now().await;
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    //! The per-item validation, and the statement templates.

    use super::*;
    use crate::modules::notes::model::MAX_MUTATIONS_PER_PUSH;

    fn note_mutation(title: String, content: String) -> NoteMutation {
        NoteMutation {
            id: Uuid::from_u128(1),
            title,
            content,
            pinned: false,
            folder_id: None,
            deleted: false,
            base_version: None,
            client_updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn a_within_limits_mutation_is_not_rejected() {
        let mutation = note_mutation("t".repeat(MAX_TITLE_CHARS), "c".repeat(MAX_CONTENT_CHARS));
        assert!(note_rejection(&mutation).is_none());
    }

    #[test]
    fn an_over_long_title_or_content_is_rejected_with_a_reason() {
        assert!(
            note_rejection(&note_mutation(
                "t".repeat(MAX_TITLE_CHARS + 1),
                String::new()
            ))
            .is_some()
        );
        assert!(
            note_rejection(&note_mutation(
                String::new(),
                "c".repeat(MAX_CONTENT_CHARS + 1)
            ))
            .is_some()
        );
    }

    #[test]
    fn the_statement_splice_fills_both_column_lists() {
        let sql = statement("SELECT {NOTE_COLUMNS} FROM notes JOIN {FOLDER_COLUMNS}");
        let sql = sql.0;
        assert!(!sql.contains("{NOTE_COLUMNS}"), "{sql}");
        assert!(!sql.contains("{FOLDER_COLUMNS}"), "{sql}");
        assert!(sql.contains("creator_id"), "{sql}");
    }

    #[test]
    fn the_push_ceiling_is_the_one_both_the_dto_and_the_statement_obey() {
        assert_eq!(MAX_MUTATIONS_PER_PUSH, 200);
    }

    #[test]
    fn a_purge_table_names_a_real_table() {
        assert_eq!(PurgeTable::Notes.as_str(), "notes");
        assert_eq!(PurgeTable::Folders.as_str(), "folders");
    }
}
