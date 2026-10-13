//! §10 **integration** layer: `PgNotesRepo` against a real Postgres.
//!
//! `notes_service.rs` runs against the in-memory double, which mirrors the SQL's
//! *semantics* — but only Postgres reads the migrations and executes the statements. The
//! two behaviours worth their weight on their own are the **batch transaction** (a push is
//! all-or-nothing, and per-item validation failures do not roll it back) and the
//! **folder-delete cascade** (`master` ignored `shouldDeleteNotes` entirely).
//!
//! `#[sqlx::test(migrations = "../../crates/db/migrations")]` applies every migration to a fresh per-test database and rolls it back,
//! so the tests are independent and need no cleanup. When `DATABASE_URL` is unset the
//! tests skip rather than fail, so a contributor with no database still gets a green run.
//!
//! ```text
//! cargo test -p meno-api --features test-support --test notes_repository_pg
//! ```

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use meno_api::modules::notes::model::{
    MutationResolution, NoteMutation, ResolvedMutation, SyncBatch, SyncMutation,
};
use meno_api::modules::notes::repository::{
    CreateFolderInput, CreateNoteInput, NoteFilter, NotesOnFolderDelete, NotesRepo, PgNotesRepo,
    UpdateNoteInput,
};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// Skip with a readable message rather than failing opaquely when `DATABASE_URL` is unset.
fn database_url() -> Option<String> {
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
}

macro_rules! with_pool {
    (|$pool:ident| $body:block) => {{
        if database_url().is_none() {
            eprintln!("skipping: set DATABASE_URL to a reachable Postgres to run this");
            return;
        }
        let $pool = $pool;
        $body
    }};
}

fn ada() -> Uuid {
    Uuid::from_u128(1)
}

fn grace() -> Uuid {
    Uuid::from_u128(2)
}

fn instant(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds).expect("a plausible instant")
}

/// Insert a live account — the fixture, not the subject under test.
async fn user(pool: &PgPool, id: Uuid, email: &str) {
    sqlx::query("INSERT INTO users (id, full_name, email, verified) VALUES ($1, $2, $3, true)")
        .bind(id)
        .bind("Ada Lovelace")
        .bind(email)
        .execute(pool)
        .await
        .expect("the fixture account is inserted");
}

async fn note_row(pool: &PgPool, id: Uuid) -> Option<(Option<Uuid>, Option<OffsetDateTime>, i32)> {
    sqlx::query_as::<_, (Option<Uuid>, Option<OffsetDateTime>, i32)>(
        "SELECT folder_id, deleted_at, version FROM notes WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .expect("the row is read")
}

fn note_mutation(
    id: Uuid,
    base_version: Option<i32>,
    client_updated_at: OffsetDateTime,
) -> NoteMutation {
    NoteMutation {
        id,
        title: "offline".to_owned(),
        content: "written on a plane".to_owned(),
        pinned: false,
        folder_id: None,
        deleted: false,
        base_version,
        client_updated_at,
    }
}

// ── notes: optimistic concurrency ─────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_note_round_trips_and_a_stale_update_writes_nothing(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "ada@example.com").await;
        let repo = PgNotesRepo::new(pool.clone());

        let created = repo
            .create_note(&CreateNoteInput {
                id: Uuid::from_u128(0x10),
                title: "Groceries",
                content: "milk",
                pinned: false,
                folder_id: None,
                creator_id: ada(),
            })
            .await
            .expect("created");
        assert_eq!(created.version, 1);

        // The matching version lands.
        let updated = repo
            .update_note_by_version(&UpdateNoteInput {
                title: Some("Shopping"),
                content: None,
                pinned: None,
                folder_id: None,
                note_id: created.id,
                creator_id: ada(),
                base_version: 1,
            })
            .await
            .expect("no driver error")
            .expect("the version matched");
        assert_eq!(updated.version, 2);
        assert_eq!(updated.title, "Shopping");

        // A stale version writes nothing.
        let missed = repo
            .update_note_by_version(&UpdateNoteInput {
                title: Some("too late"),
                content: None,
                pinned: None,
                folder_id: None,
                note_id: created.id,
                creator_id: ada(),
                base_version: 1,
            })
            .await
            .expect("no driver error");
        assert!(missed.is_none());
        assert_eq!(
            repo.find_note_by_id(created.id, ada())
                .await
                .expect("read")
                .expect("still there")
                .title,
            "Shopping"
        );
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn keyset_pagination_neither_repeats_nor_skips_a_row(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "ada@example.com").await;
        let repo = PgNotesRepo::new(pool.clone());

        for i in 0..5u128 {
            repo.create_note(&CreateNoteInput {
                id: Uuid::from_u128(i + 1),
                title: "note",
                content: "body",
                pinned: false,
                folder_id: None,
                creator_id: ada(),
            })
            .await
            .expect("created");
        }

        let mut seen = Vec::new();
        let mut cursor = None;
        loop {
            let page = repo
                .find_notes(&NoteFilter {
                    creator_id: ada(),
                    folder_id: None,
                    pinned: None,
                    keywords: None,
                    limit_plus_one: 3,
                    cursor,
                })
                .await
                .expect("a page");
            let has_more = page.len() > 2;
            let page: Vec<_> = page.into_iter().take(2).collect();
            if let Some(last) = page.last() {
                cursor = Some((last.created_at, last.id));
            }
            seen.extend(page.into_iter().map(|n| n.id));
            if !has_more {
                break;
            }
        }

        let unique: std::collections::HashSet<Uuid> = seen.iter().copied().collect();
        assert_eq!(unique.len(), seen.len(), "a row was repeated across pages");
        assert_eq!(seen.len(), 5, "a row was skipped");
    });
}

// ── deleting a folder acts on its notes (the regression) ──────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn deleting_a_folder_orphans_its_notes(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "ada@example.com").await;
        let repo = PgNotesRepo::new(pool.clone());

        let folder = repo
            .create_folder(&CreateFolderInput {
                id: Uuid::from_u128(0x10),
                title: "House",
                pinned: false,
                creator_id: ada(),
            })
            .await
            .expect("folder");
        let note = repo
            .create_note(&CreateNoteInput {
                id: Uuid::from_u128(1),
                title: "Groceries",
                content: "milk",
                pinned: false,
                folder_id: Some(folder.id),
                creator_id: ada(),
            })
            .await
            .expect("note");

        repo.delete_folder(folder.id, ada(), 1, NotesOnFolderDelete::Orphan)
            .await
            .expect("deleted")
            .expect("version matched");

        let (folder_id, deleted_at, _) = note_row(&pool, note.id).await.expect("row survives");
        assert_eq!(folder_id, None, "the note is unfiled, not deleted");
        assert!(deleted_at.is_none());
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn deleting_a_folder_with_delete_soft_deletes_its_notes(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "ada@example.com").await;
        let repo = PgNotesRepo::new(pool.clone());

        let folder = repo
            .create_folder(&CreateFolderInput {
                id: Uuid::from_u128(0x10),
                title: "House",
                pinned: false,
                creator_id: ada(),
            })
            .await
            .expect("folder");
        let note = repo
            .create_note(&CreateNoteInput {
                id: Uuid::from_u128(1),
                title: "Groceries",
                content: "milk",
                pinned: false,
                folder_id: Some(folder.id),
                creator_id: ada(),
            })
            .await
            .expect("note");

        repo.delete_folder(folder.id, ada(), 1, NotesOnFolderDelete::Delete)
            .await
            .expect("deleted")
            .expect("version matched");

        let (_, deleted_at, version) = note_row(&pool, note.id).await.expect("row survives");
        assert!(deleted_at.is_some(), "the note is soft-deleted");
        assert_eq!(version, 2, "the soft delete bumped the version");
    });
}

// ── sync ──────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_create_then_delete_offline_writes_no_row(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "ada@example.com").await;
        let repo = PgNotesRepo::new(pool.clone());
        let id = Uuid::from_u128(1);

        let batch = SyncBatch::split(vec![SyncMutation::Note(NoteMutation {
            deleted: true,
            ..note_mutation(id, None, instant(0))
        })]);

        let results = repo.apply_sync_batch(ada(), &batch).await.expect("applied");
        assert_eq!(results.len(), 1);
        assert!(matches!(
            results[0].resolution,
            MutationResolution::Applied { entity: None }
        ));
        assert!(note_row(&pool, id).await.is_none(), "nothing was written");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_versioned_push_that_misses_returns_the_server_copy(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "ada@example.com").await;
        let repo = PgNotesRepo::new(pool.clone());
        let note = repo
            .create_note(&CreateNoteInput {
                id: Uuid::from_u128(1),
                title: "seeded",
                content: "body",
                pinned: false,
                folder_id: None,
                creator_id: ada(),
            })
            .await
            .expect("note");

        let batch = SyncBatch::split(vec![SyncMutation::Note(note_mutation(
            note.id,
            Some(99),
            instant(1000),
        ))]);

        let results = repo.apply_sync_batch(ada(), &batch).await.expect("applied");
        let ResolvedMutation { resolution, .. } = &results[0];
        match resolution {
            MutationResolution::Conflict { entity } => {
                assert_eq!(entity.id(), note.id);
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_note_referencing_an_unowned_folder_is_unfiled(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "ada@example.com").await;
        user(&pool, grace(), "grace@example.com").await;
        let repo = PgNotesRepo::new(pool.clone());

        let theirs = repo
            .create_folder(&CreateFolderInput {
                id: Uuid::from_u128(0x11),
                title: "Grace's",
                pinned: false,
                creator_id: grace(),
            })
            .await
            .expect("folder");
        let id = Uuid::from_u128(1);

        let batch = SyncBatch::split(vec![SyncMutation::Note(NoteMutation {
            folder_id: Some(theirs.id),
            ..note_mutation(id, None, instant(0))
        })]);
        repo.apply_sync_batch(ada(), &batch).await.expect("applied");

        let (folder_id, _, _) = note_row(&pool, id).await.expect("row written");
        assert_eq!(folder_id, None, "an unowned folder must not be attached");
    });
}

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn a_notes_feed_is_ordered_by_creation(pool: PgPool) {
    with_pool!(|pool| {
        user(&pool, ada(), "ada@example.com").await;
        let repo = PgNotesRepo::new(pool.clone());
        for i in 0..3u128 {
            repo.create_note(&CreateNoteInput {
                id: Uuid::from_u128(i + 1),
                title: "note",
                content: "body",
                pinned: false,
                folder_id: None,
                creator_id: ada(),
            })
            .await
            .expect("created");
        }

        let page = repo
            .find_notes(&NoteFilter {
                creator_id: ada(),
                folder_id: None,
                pinned: None,
                keywords: None,
                limit_plus_one: 10,
                cursor: None,
            })
            .await
            .expect("a page");
        assert_eq!(page.len(), 3);

        let changed = repo
            .find_notes_changed_since(ada(), None, 10)
            .await
            .expect("sync pull");
        assert_eq!(changed.len(), 3, "every row is in the sync stream");
    });
}
