//! The notes module's state: where concrete adapters are chosen (§5.6).
//!
//! The whole file's job: pool → [`PgNotesRepo`]. Every consumer above sees
//! `Arc<dyn NotesRepo>` and the module's tests assemble it from the in-memory double
//! instead, which is what lets the whole service suite run with no Postgres.
//!
//! # Construction is infallible, and that is deliberate
//!
//! Nothing here reads a secret or dials anything: the pool is already connected. A
//! `Result` that can only be `Ok` is a worse API than none, because every caller writes
//! the same dead match on it — the same reasoning as [`crate::modules::broadcast::state`].

use std::sync::Arc;

use super::repository::{NotesRepo, PgNotesRepo};
use super::service::NotesService;

/// The notes module as the router sees it.
#[derive(Clone)]
pub struct NotesState {
    /// The rules. A handler reaches through this and nothing else.
    pub service: Arc<NotesService>,
}

impl std::fmt::Debug for NotesState {
    /// Names the field and stops: the service holds a pool.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotesState")
            .field("service", &" [notes rules]")
            .finish()
    }
}

/// The raw resources [`NotesState::new`] builds adapters from.
///
/// One pool and nothing else: notes and folders are two tables and no external service.
pub struct Wiring {
    /// The application's pool.
    pub pool: sqlx::PgPool,
}

impl NotesState {
    /// Build the module from raw resources.
    #[must_use]
    pub fn new(wiring: Wiring) -> Self {
        let repo: Arc<dyn NotesRepo> = Arc::new(PgNotesRepo::new(wiring.pool));
        Self::from_parts(repo)
    }

    /// Build the module from its trait object — what `new` funnels into, and what tests
    /// call with the double.
    #[must_use]
    pub fn from_parts(repo: Arc<dyn NotesRepo>) -> Self {
        Self {
            service: Arc::new(NotesService::new(repo)),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The wiring's one property: a module built from the double is usable with no
    //! infrastructure.

    use super::*;
    use crate::modules::notes::dto::CreateNoteRequest;
    use crate::modules::notes::repository::InMemoryNotesRepo;

    #[tokio::test]
    async fn a_state_built_from_doubles_creates_a_note_without_postgres() {
        let repo = Arc::new(InMemoryNotesRepo::new());
        let state = NotesState::from_parts(Arc::clone(&repo) as Arc<dyn NotesRepo>);

        let created = state
            .service
            .create_note(
                uuid::Uuid::from_u128(1),
                &CreateNoteRequest {
                    id: uuid::Uuid::from_u128(2),
                    title: "Groceries".to_owned(),
                    content: "milk".to_owned(),
                    pinned: false,
                    folder_id: None,
                },
            )
            .await
            .expect("a note can be created with no infrastructure");

        assert_eq!(created.title, "Groceries");
        assert_eq!(repo.stored_note_count(), 1);
    }

    #[test]
    fn the_debug_impl_names_the_field_and_nothing_else() {
        let state =
            NotesState::from_parts(Arc::new(InMemoryNotesRepo::new()) as Arc<dyn NotesRepo>);
        assert_eq!(
            format!("{state:?}"),
            "NotesState { service: \" [notes rules]\" }"
        );
    }
}
