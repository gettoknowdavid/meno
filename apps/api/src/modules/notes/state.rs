#[derive(Clone)]
pub struct NotesState {
    pub service: crate::modules::notes::service::DynNotesService,
}

impl NotesState {
    pub fn new(db: sqlx::PgPool, jobs: crate::jobs::Jobs) -> Self {
        let repo = std::sync::Arc::new(crate::modules::notes::repository::NotesRepository::new(
            db.clone(),
        ));

        let service = crate::modules::notes::service::NotesService::new(
            std::sync::Arc::clone(&repo),
            db,
            jobs,
        );

        Self { service }
    }
}
