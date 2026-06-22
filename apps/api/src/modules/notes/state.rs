use crate::modules::notes::repository::NotesRepository;
use crate::modules::notes::service::{DynNotesService, NotesService};
use std::sync::Arc;

#[derive(Clone)]
pub struct NotesState {
    pub service: DynNotesService,
}

impl NotesState {
    pub fn new(db: sqlx::PgPool) -> Self {
        let repo = Arc::new(NotesRepository::new(db.clone()));
        let service = NotesService::new(Arc::clone(&repo), db);
        Self { service }
    }
}
