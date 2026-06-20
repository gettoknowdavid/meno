use crate::modules::notes::errors::NotesError;
use crate::modules::notes::model::{Folder, Note};

#[derive(Debug, Clone)]
pub struct NotesRepository {
    db: sqlx::PgPool,
}
impl NotesRepository {
    #[must_use]
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
pub trait NotesRepo: Send + Sync + 'static {
    async fn create_note<'e, E>(
        &self,
        executor: E,
        input: &CreateNoteInput<'e>,
    ) -> Result<Note, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn update_note<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        base_version: i32,
        input: &UpdateNoteInput<'e>,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn soft_delete_note<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        base_version: i32,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn find_note_by_id(
        &self,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
    ) -> Result<Option<Note>, NotesError>;

    async fn find_notes(
        &self,
        creator_id: uuid::Uuid,
        query: &crate::modules::notes::dto::NotesQuery,
    ) -> Result<Vec<Note>, NotesError>;

    async fn add_note_to_folder<'e, E>(
        &self,
        executor: E,
        note_id: uuid::Uuid,
        creator_id: uuid::Uuid,
        folder_id: uuid::Uuid,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn remove_note_from_folder<'e, E>(
        &self,
        executor: E,
        note_id: uuid::Uuid,
        creator_id: uuid::Uuid,
        folder_id: uuid::Uuid,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn move_notes_to_folder(
        &self,
        note_ids: &[uuid::Uuid],
        creator_id: uuid::Uuid,
        folder_id: Option<uuid::Uuid>,
    ) -> Result<Vec<Note>, NotesError>;

    async fn orphan_notes_in_folder<'e, E>(
        &self,
        executor: E,
        folder_id: uuid::Uuid,
        creator_id: uuid::Uuid,
    ) -> Result<u64, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn owned_folder_ids(
        &self,
        creator_id: uuid::Uuid,
        candidate_ids: &[uuid::Uuid],
    ) -> Result<std::collections::HashSet<uuid::Uuid>, NotesError>;

    async fn create_folder<'e, E>(
        &self,
        executor: E,
        input: &CreateFolderInput<'e>,
    ) -> Result<Folder, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn update_folder<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        base_version: i32,
        input: &UpdateFolderInput<'e>,
    ) -> Result<Option<Folder>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn soft_delete_folder<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
    ) -> Result<Option<Folder>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
    ) -> Result<Option<Folder>, NotesError>;

    async fn find_folders(
        &self,
        creator_id: uuid::Uuid,
        query: &crate::modules::notes::dto::FoldersQuery,
    ) -> Result<Vec<Folder>, NotesError>;

    async fn upsert_note_if_absent<'e, E>(
        &self,
        executor: E,
        input: &UpsertNoteInput<'e>,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn apply_note_snapshot_if_version_matches<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        base_version: i32,
        snapshot: &NoteSnapshotInput<'e>,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn force_apply_note_snapshot<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        snapshot: &NoteSnapshotInput<'e>,
    ) -> Result<Note, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn find_notes_changed_since(
        &self,
        creator_id: uuid::Uuid,
        cursor: Option<(time::OffsetDateTime, uuid::Uuid)>,
        limit: i64,
    ) -> Result<Vec<Note>, NotesError>;

    async fn upsert_folder_if_absent<'e, E>(
        &self,
        executor: E,
        input: &UpsertFolderInput<'e>,
    ) -> Result<Option<Folder>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn apply_folder_snapshot_if_version_matches<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        base_version: i32,
        snapshot: &FolderSnapshotInput<'e>,
    ) -> Result<Option<Folder>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn force_apply_folder_snapshot<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        snapshot: &FolderSnapshotInput<'e>,
    ) -> Result<Folder, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>;

    async fn find_folders_changed_since(
        &self,
        creator_id: uuid::Uuid,
        cursor: Option<(time::OffsetDateTime, uuid::Uuid)>,
        limit: i64,
    ) -> Result<Vec<Folder>, NotesError>;

    async fn purge_deleted_notes_older_than(
        &self,
        cutoff: time::OffsetDateTime,
    ) -> Result<u64, NotesError>;

    async fn purge_deleted_folders_older_than(
        &self,
        cutoff: time::OffsetDateTime,
    ) -> Result<u64, NotesError>;
}

#[async_trait::async_trait]
impl NotesRepo for NotesRepository {
    async fn create_note<'e, E>(
        &self,
        executor: E,
        input: &CreateNoteInput<'e>,
    ) -> Result<Note, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Note,
            r"
            INSERT INTO notes (id, title, content, pinned, folder_id, creator_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *",
            input.id,
            input.title,
            input.content,
            input.pinned,
            input.folder_id,
            input.creator_id,
        )
        .fetch_one(executor)
        .await
        .map_err(NotesError::Database)
    }
    async fn update_note<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        base_version: i32,
        input: &UpdateNoteInput<'e>,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Note,
            r"UPDATE notes
            SET title = COALESCE($1, title),
                content = COALESCE($2, content),
                pinned = COALESCE($3, pinned),
                folder_id = $4,
                version = version + 1,
                updated_at = NOW()
            WHERE id = $5 AND creator_id = $6 AND version = $7 AND deleted_at IS NULL
            RETURNING *",
            input.title,
            input.content,
            input.pinned,
            input.folder_id,
            id,
            creator_id,
            base_version,
        )
        .fetch_optional(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn soft_delete_note<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        base_version: i32,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Note,
            r"UPDATE notes
            SET deleted_at = NOW(), version = version + 1, updated_at = NOW()
            WHERE id = $1 AND creator_id = $2 AND version = $3 AND deleted_at IS NULL
            RETURNING *",
            id,
            creator_id,
            base_version
        )
        .fetch_optional(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn find_note_by_id(
        &self,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
    ) -> Result<Option<Note>, NotesError> {
        sqlx::query_as!(
            Note,
            r"SELECT * FROM notes WHERE id = $1 AND creator_id = $2 AND deleted_at IS NULL",
            id,
            creator_id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(NotesError::Database)
    }

    async fn find_notes(
        &self,
        creator_id: uuid::Uuid,
        query: &crate::modules::notes::dto::NotesQuery,
    ) -> Result<Vec<Note>, NotesError> {
        let (cursor_ts, cursor_id) = match query.cursor() {
            None => (None, None),
            Some(c) => {
                let (ts, id) = c.to_timestamp_id().map_err(NotesError::Cursor)?;
                (Some(ts), Some(id))
            }
        };

        let mut qb = sqlx::QueryBuilder::new("SELECT * FROM notes WHERE creator_id = ");
        qb.push_bind(creator_id).push(" AND deleted_at IS NULL");

        if let Some(fid) = query.folder_id {
            qb.push(" AND folder_id = ").push_bind(fid);
        }

        if let Some(p) = query.pinned {
            qb.push(" AND pinned = ").push_bind(p);
        }

        if let Some(ref kw) = query.keywords {
            qb.push(" AND to_tsvector('english', title || ' ' || content) @@ plainto_tsquery('english', ")
                .push_bind(kw.trim())
                .push(")");
        }

        crate::shared::repository::push_cursor_condition(
            &mut qb,
            "created_at",
            "id",
            cursor_ts,
            cursor_id,
            crate::shared::pagination::Order::Desc,
        );

        crate::shared::repository::push_order_and_limit(
            &mut qb,
            "created_at",
            "id",
            crate::shared::pagination::Order::Desc,
            query.limit_plus_one(),
        );

        qb.build_query_as::<Note>()
            .fetch_all(&self.db)
            .await
            .map_err(NotesError::Database)
    }

    async fn add_note_to_folder<'e, E>(
        &self,
        executor: E,
        note_id: uuid::Uuid,
        creator_id: uuid::Uuid,
        folder_id: uuid::Uuid,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Note,
            r"UPDATE notes
            SET folder_id = $1, version = version + 1, updated_at = NOW()
            WHERE id = $2 AND creator_id = $3 AND deleted_at IS NULL
            RETURNING *",
            folder_id,
            note_id,
            creator_id,
        )
        .fetch_optional(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn remove_note_from_folder<'e, E>(
        &self,
        executor: E,
        note_id: uuid::Uuid,
        creator_id: uuid::Uuid,
        folder_id: uuid::Uuid,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Note,
            r"UPDATE notes
            SET folder_id = NULL, version = version + 1, updated_at = NOW()
            WHERE id = $1 AND creator_id = $2 AND folder_id = $3 AND deleted_at IS NULL
            RETURNING *",
            note_id,
            creator_id,
            folder_id,
        )
        .fetch_optional(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn move_notes_to_folder(
        &self,
        note_ids: &[uuid::Uuid],
        creator_id: uuid::Uuid,
        folder_id: Option<uuid::Uuid>,
    ) -> Result<Vec<Note>, NotesError> {
        sqlx::query_as!(
            Note,
            r"UPDATE notes
            SET folder_id = $1, version = version + 1, updated_at = NOW()
            WHERE ID = ANY($2) AND creator_id = $3 AND deleted_at IS NULL
            RETURNING *",
            folder_id,
            note_ids,
            creator_id,
        )
        .fetch_all(&self.db)
        .await
        .map_err(NotesError::Database)
    }

    async fn orphan_notes_in_folder<'e, E>(
        &self,
        executor: E,
        folder_id: uuid::Uuid,
        creator_id: uuid::Uuid,
    ) -> Result<u64, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let result = sqlx::query!(
            r"UPDATE notes
            SET folder_id = NULL, version = version + 1, updated_at = NOW()
            WHERE folder_id = $1 AND creator_id = $2 AND deleted_at IS NULL",
            folder_id,
            creator_id,
        )
        .execute(executor)
        .await
        .map_err(NotesError::Database)?;
        Ok(result.rows_affected())
    }

    async fn owned_folder_ids(
        &self,
        creator_id: uuid::Uuid,
        candidate_ids: &[uuid::Uuid],
    ) -> Result<std::collections::HashSet<uuid::Uuid>, NotesError> {
        if candidate_ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let rows = sqlx::query_scalar!(
            r"SELECT id FROM folders WHERE creator_id = $1 AND id = ANY($2)",
            creator_id,
            candidate_ids,
        )
        .fetch_all(&self.db)
        .await
        .map_err(NotesError::Database)?;
        Ok(rows.into_iter().collect())
    }

    async fn create_folder<'e, E>(
        &self,
        executor: E,
        input: &CreateFolderInput<'e>,
    ) -> Result<Folder, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Folder,
            r"
            INSERT INTO folders (id, title, pinned, creator_id)
            VALUES ($1, $2, $3, $4)
            RETURNING *",
            input.id,
            input.title,
            input.pinned,
            input.creator_id,
        )
        .fetch_one(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn update_folder<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        base_version: i32,
        input: &UpdateFolderInput<'e>,
    ) -> Result<Option<Folder>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Folder,
            r"UPDATE folders
            SET title = COALESCE($1, title),
                pinned = COALESCE($2, pinned),
                version = version + 1,
                updated_at = NOW()
            WHERE id = $3 AND creator_id = $4 AND version = $5 AND deleted_at IS NULL
            RETURNING *",
            input.title,
            input.pinned,
            id,
            creator_id,
            base_version,
        )
        .fetch_optional(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn soft_delete_folder<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
    ) -> Result<Option<Folder>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Folder,
            r"UPDATE folders
            SET deleted_at = NOW(), version = version + 1, updated_at = NOW()
            WHERE id = $1 AND creator_id = $2 AND deleted_at IS NULL
            RETURNING *",
            id,
            creator_id,
        )
        .fetch_optional(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
    ) -> Result<Option<Folder>, NotesError> {
        sqlx::query_as!(
            Folder,
            r"SELECT * FROM folders WHERE id = $1 AND creator_id = $2 AND deleted_at IS NULL",
            id,
            creator_id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(NotesError::Database)
    }

    async fn find_folders(
        &self,
        creator_id: uuid::Uuid,
        query: &crate::modules::notes::dto::FoldersQuery,
    ) -> Result<Vec<Folder>, NotesError> {
        let (cursor_ts, cursor_id) = match query.cursor() {
            None => (None, None),
            Some(c) => {
                let (ts, id) = c.to_timestamp_id().map_err(NotesError::Cursor)?;
                (Some(ts), Some(id))
            }
        };

        let mut qb = sqlx::QueryBuilder::new("SELECT * FROM folders WHERE creator_id = ");
        qb.push_bind(creator_id).push(" AND deleted_at IS NULL");

        if let Some(p) = query.pinned {
            qb.push(" AND pinned = ").push_bind(p);
        }

        if let Some(ref kw) = query.keywords {
            qb.push(" AND to_tsvector('english', title) @@ plainto_tsquery('english', ")
                .push_bind(kw.trim())
                .push(")");
        }

        crate::shared::repository::push_cursor_condition(
            &mut qb,
            "created_at",
            "id",
            cursor_ts,
            cursor_id,
            crate::shared::pagination::Order::Desc,
        );

        crate::shared::repository::push_order_and_limit(
            &mut qb,
            "created_at",
            "id",
            crate::shared::pagination::Order::Desc,
            query.limit_plus_one(),
        );

        qb.build_query_as::<Folder>()
            .fetch_all(&self.db)
            .await
            .map_err(NotesError::Database)
    }

    async fn upsert_note_if_absent<'e, E>(
        &self,
        executor: E,
        input: &UpsertNoteInput<'e>,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Note,
            r"INSERT INTO notes (id, title, content, pinned, folder_id, creator_id)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (id) DO NOTHING
               RETURNING *",
            input.id,
            input.snapshot.title,
            input.snapshot.content,
            input.snapshot.pinned,
            input.snapshot.folder_id,
            input.creator_id,
        )
        .fetch_optional(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn apply_note_snapshot_if_version_matches<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        base_version: i32,
        snapshot: &NoteSnapshotInput<'e>,
    ) -> Result<Option<Note>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        // No COALESCE, no `deleted_at IS NULL` guard — `version` is the only
        // concurrency check; `deleted` is just a field value here, able to
        // move forward or backward like anything else.
        sqlx::query_as!(
            Note,
            r"UPDATE notes
               SET title = $1,
                   content = $2,
                   pinned = $3,
                   folder_id = $4,
                   deleted_at = CASE WHEN $5 THEN COALESCE(deleted_at, NOW()) END,
                   version = version + 1,
                   updated_at = NOW()
               WHERE id = $6 AND creator_id = $7 AND version = $8
               RETURNING *",
            snapshot.title,
            snapshot.content,
            snapshot.pinned,
            snapshot.folder_id,
            snapshot.deleted,
            id,
            creator_id,
            base_version,
        )
        .fetch_optional(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn force_apply_note_snapshot<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        snapshot: &NoteSnapshotInput<'e>,
    ) -> Result<Note, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Note,
            r"UPDATE notes
               SET title = $1,
                   content = $2,
                   pinned = $3,
                   folder_id = $4,
                   deleted_at = CASE WHEN $5 THEN COALESCE(deleted_at, NOW()) END,
                   version = version + 1,
                   updated_at = NOW()
               WHERE id = $6 AND creator_id = $7
               RETURNING *",
            snapshot.title,
            snapshot.content,
            snapshot.pinned,
            snapshot.folder_id,
            snapshot.deleted,
            id,
            creator_id,
        )
        .fetch_one(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn find_notes_changed_since(
        &self,
        creator_id: uuid::Uuid,
        cursor: Option<(time::OffsetDateTime, uuid::Uuid)>,
        limit: i64,
    ) -> Result<Vec<Note>, NotesError> {
        let mut qb = sqlx::QueryBuilder::new("SELECT * FROM notes WHERE creator_id = ");

        qb.push_bind(creator_id);

        if let Some((ts, id)) = cursor {
            qb.push(" AND (updated_at, id) > (")
                .push_bind(ts)
                .push(", ")
                .push_bind(id)
                .push(")");
        }

        // ASC, not DESC — sync is a forward replay of "everything since X",
        // the opposite of every other newest-first feed in this app.
        qb.push(" ORDER BY updated_at ASC, id ASC LIMIT ")
            .push_bind(limit);

        qb.build_query_as::<Note>()
            .fetch_all(&self.db)
            .await
            .map_err(NotesError::Database)
    }

    async fn upsert_folder_if_absent<'e, E>(
        &self,
        executor: E,
        input: &UpsertFolderInput<'e>,
    ) -> Result<Option<Folder>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Folder,
            r"INSERT INTO folders (id, title, pinned, creator_id)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (id) DO NOTHING
               RETURNING *",
            input.id,
            input.snapshot.title,
            input.snapshot.pinned,
            input.creator_id,
        )
        .fetch_optional(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn apply_folder_snapshot_if_version_matches<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        base_version: i32,
        snapshot: &FolderSnapshotInput<'e>,
    ) -> Result<Option<Folder>, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Folder,
            r"UPDATE folders
               SET title = $1, 
                   pinned = $2,
                   deleted_at = CASE WHEN $3 THEN COALESCE(deleted_at, NOW()) END,
                   version = version + 1, 
                   updated_at = NOW()
               WHERE id = $4 AND creator_id = $5 AND version = $6
               RETURNING *",
            snapshot.title,
            snapshot.pinned,
            snapshot.deleted,
            id,
            creator_id,
            base_version,
        )
        .fetch_optional(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn force_apply_folder_snapshot<'e, E>(
        &self,
        executor: E,
        id: uuid::Uuid,
        creator_id: uuid::Uuid,
        snapshot: &FolderSnapshotInput<'e>,
    ) -> Result<Folder, NotesError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            Folder,
            r"UPDATE folders
               SET title = $1,
                   pinned = $2,
                   deleted_at = CASE WHEN $3 THEN COALESCE(deleted_at, NOW()) END,
                   version = version + 1,
                   updated_at = NOW()
               WHERE id = $4 AND creator_id = $5
               RETURNING *",
            snapshot.title,
            snapshot.pinned,
            snapshot.deleted,
            id,
            creator_id,
        )
        .fetch_one(executor)
        .await
        .map_err(NotesError::Database)
    }

    async fn find_folders_changed_since(
        &self,
        creator_id: uuid::Uuid,
        cursor: Option<(time::OffsetDateTime, uuid::Uuid)>,
        limit: i64,
    ) -> Result<Vec<Folder>, NotesError> {
        let mut qb = sqlx::QueryBuilder::new("SELECT * FROM folders WHERE creator_id = ");

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
            .fetch_all(&self.db)
            .await
            .map_err(NotesError::Database)
    }

    async fn purge_deleted_notes_older_than(
        &self,
        cutoff: time::OffsetDateTime,
    ) -> Result<u64, NotesError> {
        purge_batched(&self.db, "notes", cutoff).await
    }

    async fn purge_deleted_folders_older_than(
        &self,
        cutoff: time::OffsetDateTime,
    ) -> Result<u64, NotesError> {
        purge_batched(&self.db, "folders", cutoff).await
    }
}

/// Shared batched-delete loop — mirrors `AuthRepository::cleanup_expired_refresh_tokens`.
/// `table` is a fixed internal literal (never user input), so string interpolation
/// here carries no injection risk.
async fn purge_batched(
    db: &sqlx::PgPool,
    table: &str,
    cutoff: time::OffsetDateTime,
) -> Result<u64, NotesError> {
    let mut total: u64 = 0;

    loop {
        let sql = format!(
            r"WITH deleted AS (
                DELETE FROM {table}
                WHERE id IN (
                    SELECT id
                    FROM {table}
                    WHERE deleted_at IS NOT NULL AND deleted_at < $1
                    LIMIT 5000
                )
                RETURNING 1
            )
            SELECT COUNT(*) FROM deleted"
        );

        let n: i64 = sqlx::query_scalar(&sql)
            .bind(cutoff)
            .fetch_one(db)
            .await
            .map_err(NotesError::Database)?;

        let n = n.cast_unsigned();

        total += n;

        if n < 5000 {
            break;
        }

        tokio::task::yield_now().await;
    }

    Ok(total)
}

pub struct CreateNoteInput<'e> {
    pub id: uuid::Uuid,
    pub title: &'e str,
    pub content: &'e str,
    pub pinned: bool,
    pub folder_id: Option<uuid::Uuid>,
    pub creator_id: uuid::Uuid,
}
pub struct CreateFolderInput<'e> {
    pub id: uuid::Uuid,
    pub title: &'e str,
    pub pinned: bool,
    pub creator_id: uuid::Uuid,
}
pub struct UpdateNoteInput<'e> {
    pub title: Option<&'e str>,
    pub content: Option<&'e str>,
    pub pinned: Option<bool>,
    pub folder_id: Option<uuid::Uuid>,
}
pub struct UpdateFolderInput<'e> {
    pub title: Option<&'e str>,
    pub pinned: Option<bool>,
}

/// Full-replacement semantics — used only by sync. Never `COALESCE'd`.
pub struct NoteSnapshotInput<'e> {
    pub title: &'e str,
    pub content: &'e str,
    pub pinned: bool,
    pub folder_id: Option<uuid::Uuid>,
    pub deleted: bool,
}
pub struct UpsertNoteInput<'e> {
    pub id: uuid::Uuid,
    pub creator_id: uuid::Uuid,
    pub snapshot: &'e NoteSnapshotInput<'e>,
}

pub struct FolderSnapshotInput<'e> {
    pub title: &'e str,
    pub pinned: bool,
    pub deleted: bool,
}
pub struct UpsertFolderInput<'e> {
    pub id: uuid::Uuid,
    pub creator_id: uuid::Uuid,
    pub snapshot: &'e FolderSnapshotInput<'e>,
}
