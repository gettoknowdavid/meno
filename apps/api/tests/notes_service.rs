//! §10 **service** layer: the notes and folders rules, against the in-memory double.
//!
//! # What this suite proves, and what it cannot
//!
//! Every assertion runs through a real [`NotesService`] with [`InMemoryNotesRepo`] behind
//! it. So it proves the *rules*: optimistic concurrency, what a guarded write reports when
//! it misses, how a sync push resolves a whole batch, and what deleting a folder does to
//! its notes. It does **not** run a line of SQL — `repository::pg` is covered by
//! `notes_repository_pg.rs`, which needs a live Postgres.
//!
//! # The regression this file exists for
//!
//! `master` parsed `shouldDeleteNotes` and then ignored it, so deleting a folder left every
//! note in it pointing at a soft-deleted parent. Two tests below name that.

// The panicking lints are denied workspace-wide but explicitly allowed in tests/ — see
// clippy.toml.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(feature = "test-support")]

use std::sync::Arc;

use meno_api::modules::notes::dto::{
    CreateFolderRequest, CreateNoteRequest, DeleteFolderRequest, MoveNotesToFolderRequest,
    NotesQuery, NotesSyncPushRequest, NotesSyncQuery, UpdateNoteRequest,
};
use meno_api::modules::notes::model::{
    EntityType, Folder, FolderMutation, Note, NoteMutation, SyncMutation,
};
use meno_api::modules::notes::repository::{InMemoryNotesRepo, NotesRepo};
use meno_api::modules::notes::service::NotesService;
use meno_core::{Cursor, CursorParams, ErrorCode};
use time::OffsetDateTime;
use uuid::Uuid;

// ── fixtures ──────────────────────────────────────────────────────────────

fn ada() -> Uuid {
    Uuid::from_u128(0x1)
}

fn grace() -> Uuid {
    Uuid::from_u128(0x2)
}

fn instant(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000 + seconds).expect("a plausible instant")
}

struct Harness {
    service: NotesService,
    repo: Arc<InMemoryNotesRepo>,
}

fn harness() -> Harness {
    let repo = Arc::new(InMemoryNotesRepo::new());
    let service = NotesService::new(Arc::clone(&repo) as Arc<dyn NotesRepo>);
    Harness { service, repo }
}

/// A live note with chosen `created_at` and `folder_id`, written straight into the double.
fn seed_note(
    h: &Harness,
    id: u128,
    creator: Uuid,
    created_at: OffsetDateTime,
    folder_id: Option<Uuid>,
) -> Uuid {
    let id = Uuid::from_u128(id);
    h.repo.seed_note(Note {
        id,
        title: "seeded".to_owned(),
        content: "body".to_owned(),
        pinned: false,
        folder_id,
        creator_id: creator,
        version: 1,
        created_at,
        updated_at: created_at,
        deleted_at: None,
    });
    id
}

/// A live folder with a chosen `created_at`.
fn seed_folder(h: &Harness, id: u128, creator: Uuid, created_at: OffsetDateTime) -> Uuid {
    let id = Uuid::from_u128(id);
    h.repo.seed_folder(Folder {
        id,
        title: "seeded".to_owned(),
        pinned: false,
        creator_id: creator,
        version: 1,
        created_at,
        updated_at: created_at,
        deleted_at: None,
    });
    id
}

/// A create request for a new note, and the id it will take.
fn create(id: u128, title: &str) -> (CreateNoteRequest, Uuid) {
    let id = Uuid::from_u128(id);
    (
        CreateNoteRequest {
            id,
            title: title.to_owned(),
            content: "body".to_owned(),
            pinned: false,
            folder_id: None,
        },
        id,
    )
}

fn page_of(limit: i64) -> NotesQuery {
    NotesQuery {
        pagination: CursorParams {
            cursor: None,
            limit: Some(limit),
        },
        ..Default::default()
    }
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

#[tokio::test]
async fn a_note_is_created_at_version_one_and_reads_back() {
    let h = harness();
    let (request, id) = create(1, "Groceries");

    let created = h
        .service
        .create_note(ada(), &request)
        .await
        .expect("created");

    assert_eq!(created.id, id);
    assert_eq!(created.title, "Groceries");
    assert_eq!(created.version, 1);
    assert!(!created.deleted);

    let found = h
        .repo
        .find_note_by_id(id, ada())
        .await
        .expect("the lookup succeeds")
        .expect("the note exists");
    assert_eq!(found.version, 1);
}

#[tokio::test]
async fn an_update_with_the_current_version_lands_and_bumps_the_version() {
    let h = harness();
    let (request, id) = create(1, "Groceries");
    h.service
        .create_note(ada(), &request)
        .await
        .expect("created");

    let updated = h
        .service
        .update_note(
            id,
            ada(),
            &UpdateNoteRequest {
                title: Some("Shopping".to_owned()),
                content: None,
                pinned: Some(true),
                folder_id: None,
                base_version: 1,
            },
        )
        .await
        .expect("the current version matches");

    assert_eq!(updated.title, "Shopping");
    assert!(updated.pinned);
    assert_eq!(updated.version, 2);
}

#[tokio::test]
async fn a_stale_update_is_a_version_conflict_and_changes_nothing() {
    let h = harness();
    let (request, id) = create(1, "Groceries");
    h.service
        .create_note(ada(), &request)
        .await
        .expect("created");

    let failure = h
        .service
        .update_note(
            id,
            ada(),
            &UpdateNoteRequest {
                title: Some("too late".to_owned()),
                content: None,
                pinned: None,
                folder_id: None,
                base_version: 99,
            },
        )
        .await
        .expect_err("a stale base_version conflicts");

    assert_eq!(failure.code(), ErrorCode::VersionConflict);
    let stored = h.repo.note(id).expect("still stored");
    assert_eq!(stored.title, "Groceries", "the body must be untouched");
    assert_eq!(stored.version, 1);
}

#[tokio::test]
async fn an_update_of_a_note_that_does_not_exist_is_not_found() {
    let h = harness();
    let failure = h
        .service
        .update_note(
            Uuid::from_u128(0xdead),
            ada(),
            &UpdateNoteRequest {
                title: Some("ghost".to_owned()),
                content: None,
                pinned: None,
                folder_id: None,
                base_version: 1,
            },
        )
        .await
        .expect_err("nothing to update");

    assert_eq!(failure.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn deleting_a_note_soft_deletes_it_and_a_second_delete_is_not_found() {
    let h = harness();
    let (request, id) = create(1, "Groceries");
    h.service
        .create_note(ada(), &request)
        .await
        .expect("created");

    let deleted = h.service.delete_note(id, ada(), 1).await.expect("deleted");
    assert!(deleted.deleted);
    assert!(h.repo.note(id).expect("row survives").deleted_at.is_some());

    let again = h
        .service
        .delete_note(id, ada(), 2)
        .await
        .expect_err("a deleted note is gone");
    assert_eq!(again.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn a_note_belonging_to_somebody_else_is_not_found() {
    let h = harness();
    let id = seed_note(&h, 1, grace(), instant(0), None);

    let failure = h
        .service
        .delete_note(id, ada(), 1)
        .await
        .expect_err("not this caller's note");
    assert_eq!(failure.code(), ErrorCode::NotFound);
    assert!(h.repo.note(id).expect("still stored").deleted_at.is_none());
}

// ── notes: pagination ─────────────────────────────────────────────────────

#[tokio::test]
async fn notes_page_newest_first_without_losing_a_row() {
    let h = harness();
    let oldest = seed_note(&h, 1, ada(), instant(0), None);
    let middle = seed_note(&h, 2, ada(), instant(100), None);
    let newest = seed_note(&h, 3, ada(), instant(200), None);

    let first = h
        .service
        .get_notes(ada(), &page_of(2))
        .await
        .expect("page one");
    assert_eq!(
        first.data.iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![newest, middle],
        "newest first"
    );
    assert!(first.has_next_page);

    let cursor = first.next_cursor.clone().expect("a cursor");
    let second = h
        .service
        .get_notes(
            ada(),
            &NotesQuery {
                pagination: CursorParams {
                    cursor: Some(cursor),
                    limit: Some(2),
                },
                ..Default::default()
            },
        )
        .await
        .expect("page two");

    assert_eq!(
        second.data.iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![oldest]
    );
    assert!(
        !second.has_next_page,
        "the cursor must not advertise a missing page"
    );

    let mut seen: Vec<Uuid> = first.data.iter().map(|n| n.id).collect();
    seen.extend(second.data.iter().map(|n| n.id));
    seen.sort();
    let mut all = vec![oldest, middle, newest];
    all.sort();
    assert_eq!(seen, all, "the pages partition the feed");
}

#[tokio::test]
async fn an_undecodable_cursor_is_a_400_not_a_panic() {
    let h = harness();
    let failure = h
        .service
        .get_notes(
            ada(),
            &NotesQuery {
                pagination: CursorParams {
                    cursor: Some(Cursor("not-a-cursor".to_owned())),
                    limit: Some(10),
                },
                ..Default::default()
            },
        )
        .await
        .expect_err("a bad cursor is the client's to fix");

    assert_eq!(failure.code(), ErrorCode::InvalidCursor);
}

// ── folders ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_note_can_be_filed_unfiled_and_moved() {
    let h = harness();
    let (request, note_id) = create(1, "Groceries");
    h.service
        .create_note(ada(), &request)
        .await
        .expect("created");
    let folder_id = Uuid::from_u128(0x10);
    h.service
        .create_folder(
            ada(),
            &CreateFolderRequest {
                id: folder_id,
                title: "House".to_owned(),
                pinned: false,
            },
        )
        .await
        .expect("folder created");

    let filed = h
        .service
        .add_note_to_folder(ada(), note_id, folder_id)
        .await
        .expect("filed");
    assert_eq!(filed.folder_id, Some(folder_id));
    assert_eq!(filed.version, 2);

    // Filing again is idempotent: the current row, no redundant version bump.
    let again = h
        .service
        .add_note_to_folder(ada(), note_id, folder_id)
        .await
        .expect("already filed");
    assert_eq!(again.version, 2);

    let unfiled = h
        .service
        .remove_note_from_folder(ada(), note_id, folder_id)
        .await
        .expect("unfiled");
    assert_eq!(unfiled.folder_id, None);

    let moved = h
        .service
        .move_notes_to_folder(
            ada(),
            &MoveNotesToFolderRequest {
                note_ids: vec![note_id],
                folder_id: Some(folder_id),
            },
        )
        .await
        .expect("moved");
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].folder_id, Some(folder_id));
}

#[tokio::test]
async fn filing_into_a_folder_that_does_not_exist_is_not_found() {
    let h = harness();
    let (request, note_id) = create(1, "Groceries");
    h.service
        .create_note(ada(), &request)
        .await
        .expect("created");

    let failure = h
        .service
        .add_note_to_folder(ada(), note_id, Uuid::from_u128(0xbeef))
        .await
        .expect_err("no such folder");
    assert_eq!(failure.code(), ErrorCode::NotFound);
}

// ── deleting a folder acts on its notes (the regression) ──────────────────

#[tokio::test]
async fn deleting_a_folder_unfiles_its_notes_by_default() {
    // `master` ignored `shouldDeleteNotes` entirely, leaving notes pointing at a
    // soft-deleted folder. The default is the safe branch: unfile.
    let h = harness();
    let folder_id = seed_folder(&h, 0x10, ada(), instant(0));
    let note_id = seed_note(&h, 1, ada(), instant(1), Some(folder_id));

    h.service
        .delete_folder(
            folder_id,
            ada(),
            &DeleteFolderRequest {
                base_version: 1,
                should_delete_notes: false,
            },
        )
        .await
        .expect("deleted");

    let note = h.repo.note(note_id).expect("the note survives");
    assert_eq!(note.folder_id, None, "the note is unfiled, not deleted");
    assert!(note.deleted_at.is_none());
}

#[tokio::test]
async fn deleting_a_folder_with_should_delete_notes_deletes_them() {
    let h = harness();
    let folder_id = seed_folder(&h, 0x10, ada(), instant(0));
    let note_id = seed_note(&h, 1, ada(), instant(1), Some(folder_id));

    h.service
        .delete_folder(
            folder_id,
            ada(),
            &DeleteFolderRequest {
                base_version: 1,
                should_delete_notes: true,
            },
        )
        .await
        .expect("deleted");

    let note = h
        .repo
        .note(note_id)
        .expect("the row survives the soft delete");
    assert!(
        note.deleted_at.is_some(),
        "the note is soft-deleted with the folder"
    );
}

#[tokio::test]
async fn a_stale_folder_delete_is_a_version_conflict() {
    let h = harness();
    let folder_id = seed_folder(&h, 0x10, ada(), instant(0));

    let failure = h
        .service
        .delete_folder(
            folder_id,
            ada(),
            &DeleteFolderRequest {
                base_version: 99,
                should_delete_notes: false,
            },
        )
        .await
        .expect_err("stale");
    assert_eq!(failure.code(), ErrorCode::VersionConflict);
    assert!(
        h.repo
            .folder(folder_id)
            .expect("still there")
            .deleted_at
            .is_none()
    );
}

// ── sync: pull ────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_pull_replays_changes_ascending_and_echoes_the_cursor_when_idle() {
    let h = harness();
    let first = seed_note(&h, 1, ada(), instant(0), None);
    let second = seed_note(&h, 2, ada(), instant(100), None);

    let page = h
        .service
        .sync_pull(ada(), &NotesSyncQuery::default())
        .await
        .expect("pull");
    assert_eq!(
        page.notes.iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![first, second]
    );
    assert!(!page.notes_has_more);
    let cursor = page.notes_next_cursor.clone().expect("a cursor");

    // Caught up: the same cursor comes back, and no rows.
    let idle = h
        .service
        .sync_pull(
            ada(),
            &NotesSyncQuery {
                notes_cursor: Some(cursor),
                folders_cursor: None,
                limit: None,
            },
        )
        .await
        .expect("pull again");
    assert!(idle.notes.is_empty());
    assert_eq!(
        idle.notes_next_cursor.map(|c| c.0),
        page.notes_next_cursor.map(|c| c.0),
        "the cursor is echoed, not dropped"
    );
}

// ── sync: push ────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_create_then_delete_offline_is_acknowledged_without_persisting() {
    let h = harness();
    let id = Uuid::from_u128(1);

    let response = h
        .service
        .sync_push(
            ada(),
            NotesSyncPushRequest {
                mutations: vec![SyncMutation::Note(NoteMutation {
                    deleted: true,
                    ..note_mutation(id, None, instant(0))
                })],
            },
        )
        .await
        .expect("push");

    assert_eq!(response.results.len(), 1);
    assert_eq!(response.results[0].id, id);
    assert_eq!(response.results[0].entity_type, EntityType::Note);
    assert!(response.results[0].server_entity.is_none());
    assert_eq!(h.repo.stored_note_count(), 0, "nothing was written");
}

#[tokio::test]
async fn a_versioned_push_that_misses_returns_the_server_copy() {
    let h = harness();
    let id = seed_note(&h, 1, ada(), instant(0), None);

    let response = h
        .service
        .sync_push(
            ada(),
            NotesSyncPushRequest {
                mutations: vec![SyncMutation::Note(note_mutation(
                    id,
                    Some(99),
                    instant(100),
                ))],
            },
        )
        .await
        .expect("push");

    let json = serde_json::to_value(&response.results[0]).expect("serialises");
    assert_eq!(json["outcome"], "CONFLICT");
    assert_eq!(json["serverEntity"]["id"], id.to_string());
    assert_eq!(json["serverEntity"]["title"], "seeded");
}

#[tokio::test]
async fn a_create_collision_resolves_last_write_wins() {
    let h = harness();
    let id = seed_note(&h, 1, ada(), instant(0), None);

    // The client's clock is later than the server's row, so its snapshot wins.
    let response = h
        .service
        .sync_push(
            ada(),
            NotesSyncPushRequest {
                mutations: vec![SyncMutation::Note(note_mutation(id, None, instant(1000)))],
            },
        )
        .await
        .expect("push");

    let json = serde_json::to_value(&response.results[0]).expect("serialises");
    assert_eq!(json["outcome"], "APPLIED");
    let stored = h.repo.note(id).expect("stored");
    assert_eq!(stored.title, "offline", "the later write won");
}

#[tokio::test]
async fn one_bad_mutation_does_not_fail_the_batch() {
    let h = harness();
    let good = Uuid::from_u128(2);

    let response = h
        .service
        .sync_push(
            ada(),
            NotesSyncPushRequest {
                mutations: vec![
                    SyncMutation::Note(NoteMutation {
                        title: "t".repeat(200),
                        ..note_mutation(Uuid::from_u128(1), None, instant(0))
                    }),
                    SyncMutation::Note(note_mutation(good, None, instant(0))),
                ],
            },
        )
        .await
        .expect("push");

    assert_eq!(response.results.len(), 2);
    assert_eq!(
        serde_json::to_value(&response.results[0]).expect("serialises")["outcome"],
        "REJECTED"
    );
    assert_eq!(
        serde_json::to_value(&response.results[1]).expect("serialises")["outcome"],
        "APPLIED"
    );
    assert_eq!(h.repo.stored_note_count(), 1, "the good mutation landed");
}

#[tokio::test]
async fn a_note_referencing_a_folder_the_caller_does_not_own_is_unfiled() {
    let h = harness();
    let theirs = seed_folder(&h, 0x11, grace(), instant(0));
    let id = Uuid::from_u128(1);

    let response = h
        .service
        .sync_push(
            ada(),
            NotesSyncPushRequest {
                mutations: vec![SyncMutation::Note(NoteMutation {
                    folder_id: Some(theirs),
                    ..note_mutation(id, None, instant(0))
                })],
            },
        )
        .await
        .expect("push");

    assert_eq!(
        serde_json::to_value(&response.results[0]).expect("serialises")["outcome"],
        "APPLIED"
    );
    assert_eq!(
        h.repo.note(id).expect("stored").folder_id,
        None,
        "an unowned folder must not be attached"
    );
}

#[tokio::test]
async fn results_come_back_in_submission_order() {
    let h = harness();
    // A note first, then a folder — even though folders are applied before notes.
    let folder_id = Uuid::from_u128(0x10);
    let note_id = Uuid::from_u128(1);

    let response = h
        .service
        .sync_push(
            ada(),
            NotesSyncPushRequest {
                mutations: vec![
                    SyncMutation::Note(note_mutation(note_id, None, instant(0))),
                    SyncMutation::Folder(FolderMutation {
                        id: folder_id,
                        title: "House".to_owned(),
                        pinned: false,
                        deleted: false,
                        base_version: None,
                        client_updated_at: instant(0),
                    }),
                ],
            },
        )
        .await
        .expect("push");

    assert_eq!(response.results[0].id, note_id);
    assert_eq!(response.results[1].id, folder_id);
}
