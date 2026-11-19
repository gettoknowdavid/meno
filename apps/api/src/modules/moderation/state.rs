//! The moderation module's state: where the concrete adapter is chosen (§5.6).
//!
//! Pool → [`PgModerationRepo`], and nothing else: reports and blocks are two tables in the
//! database this application already has. The module's tests build the same state from
//! [`InMemoryModerationRepo`] through [`ModerationState::from_parts`], which is what lets
//! the whole rule set run with no Postgres.
//!
//! Construction is infallible for the same reason [`crate::modules::notes::state`]'s is:
//! the pool is already connected, so a `Result` that can only be `Ok` would make every
//! caller write the same dead match.

use std::sync::Arc;

use super::repository::{ModerationRepo, PgModerationRepo};
use super::service::ModerationService;

/// The moderation module as the router sees it.
#[derive(Clone)]
pub struct ModerationState {
    /// The rules. A handler reaches through this and nothing else.
    pub service: Arc<ModerationService>,
}

impl std::fmt::Debug for ModerationState {
    /// Names the field and stops: the service holds a pool.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModerationState")
            .field("service", &" [moderation rules]")
            .finish()
    }
}

/// The raw resources [`ModerationState::new`] builds an adapter from.
pub struct Wiring {
    /// The application's pool.
    pub pool: sqlx::PgPool,
}

impl ModerationState {
    /// Build the module from raw resources.
    #[must_use]
    pub fn new(wiring: Wiring) -> Self {
        let repo: Arc<dyn ModerationRepo> = Arc::new(PgModerationRepo::new(wiring.pool));
        Self::from_parts(repo)
    }

    /// Build the module from its trait object — what `new` funnels into, and what tests
    /// call with the double.
    #[must_use]
    pub fn from_parts(repo: Arc<dyn ModerationRepo>) -> Self {
        Self {
            service: Arc::new(ModerationService::new(repo)),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The wiring's one property: a module built from the double is usable with no
    //! infrastructure.

    use super::*;
    use crate::modules::moderation::repository::InMemoryModerationRepo;

    #[test]
    fn the_debug_impl_names_the_field_and_nothing_else() {
        let state = ModerationState::from_parts(
            Arc::new(InMemoryModerationRepo::new()) as Arc<dyn ModerationRepo>
        );
        assert_eq!(
            format!("{state:?}"),
            "ModerationState { service: \" [moderation rules]\" }"
        );
    }
}
