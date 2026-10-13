//! The subscribers module's state: where concrete adapters are chosen (§5.6).
//!
//! The whole file's job: pool → [`PgSubscribersRepo`]. Every consumer above sees
//! `Arc<dyn SubscribersRepo>` and the module's tests assemble it from the in-memory double
//! instead, which is what lets the whole service suite run with no Postgres.
//!
//! # One pool, and nothing else
//!
//! `master`'s `SubscribersState` took the pool, an `Arc<dyn IdentityReader>` and a
//! `NotificationState` — three collaborators for a table with two columns of interest.
//! Both extra ones are gone with the notification and with the user lookup that moved onto
//! the repository, so `Wiring` is one field.
//!
//! # Construction is infallible, and that is deliberate
//!
//! Nothing here reads a secret or dials anything: the pool is already connected. A `Result`
//! that can only be `Ok` is a worse API than none, because every caller writes the same dead
//! match on it — the same reasoning as [`crate::modules::notes::state`].

use std::sync::Arc;

use super::repository::{PgSubscribersRepo, SubscribersRepo};
use super::service::SubscribersService;

/// The subscribers module as the router sees it.
#[derive(Clone)]
pub struct SubscribersState {
    /// The rules. A handler reaches through this and nothing else.
    pub service: Arc<SubscribersService>,
}

impl std::fmt::Debug for SubscribersState {
    /// Names the field and stops: the service holds a pool.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscribersState")
            .field("service", &" [subscribers rules]")
            .finish()
    }
}

/// The raw resources [`SubscribersState::new`] builds adapters from.
///
/// One pool and nothing else: one join table, no external service.
pub struct Wiring {
    /// The application's pool.
    pub pool: sqlx::PgPool,
}

impl SubscribersState {
    /// Build the module from raw resources.
    #[must_use]
    pub fn new(wiring: Wiring) -> Self {
        let repo: Arc<dyn SubscribersRepo> = Arc::new(PgSubscribersRepo::new(wiring.pool));
        Self::from_parts(repo)
    }

    /// Build the module from its trait object — what `new` funnels into, and what tests
    /// call with the double.
    #[must_use]
    pub fn from_parts(repo: Arc<dyn SubscribersRepo>) -> Self {
        Self {
            service: Arc::new(SubscribersService::new(repo)),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The wiring's one property: a module built from the double is usable with no
    //! infrastructure.

    use super::*;
    use crate::modules::subscribers::repository::InMemorySubscribersRepo;
    use uuid::Uuid;

    fn ada() -> Uuid {
        Uuid::from_u128(1)
    }

    fn grace() -> Uuid {
        Uuid::from_u128(2)
    }

    #[tokio::test]
    async fn a_state_built_from_doubles_follows_without_postgres() {
        let repo = Arc::new(InMemorySubscribersRepo::new());
        repo.seed_user(grace(), "Grace Hopper", None, None);
        let state = SubscribersState::from_parts(Arc::clone(&repo) as Arc<dyn SubscribersRepo>);

        state
            .service
            .subscribe(ada(), grace())
            .await
            .expect("a follow can be recorded with no infrastructure");

        assert!(repo.follows(ada(), grace()));
        assert_eq!(repo.stored_follow_count(), 1);
    }

    #[test]
    fn the_debug_impl_names_the_field_and_nothing_else() {
        let state = SubscribersState::from_parts(
            Arc::new(InMemorySubscribersRepo::new()) as Arc<dyn SubscribersRepo>
        );
        assert_eq!(
            format!("{state:?}"),
            "SubscribersState { service: \" [subscribers rules]\" }"
        );
    }
}
