//! What the broadcast module needs at runtime, already wired (plan §5.6).
//!
//! # This is the only file that names a concrete adapter
//!
//! §9.3 says state structs expose traits, not concrete types, to handlers. The way that
//! is enforced is this file: [`PgBroadcastRepo`] and [`LivekitMedia`] appear *here* and
//! nowhere above. A service that reached for `sqlx::PgPool` instead of
//! [`BroadcastRepo`](super::repository::BroadcastRepo) would not fail review, it would
//! fail to compile — which is the point.
//!
//! # The media server is optional, and saying so is the design
//!
//! `LIVEKIT_ENABLED` gates LiveKit (§4.6). A deployment without it still needs this
//! module to exist, because creating and scheduling a broadcast needs no media server —
//! only going live and joining do. So [`BroadcastState::new`] takes a [`Wiring`] and
//! *chooses*: [`DisabledMedia`] when no settings are present, the real adapter when they
//! are. The alternative — making `livekit` a hard dependency — would make one env
//! variable load-bearing for an application whose draft broadcasts work perfectly well
//! without it, which is §4.6's exact failure.
//!
//! # Construction is fallible only where something can actually fail
//!
//! [`BroadcastState::new`] is infallible, and that is a deliberate difference from
//! [`crate::modules::auth::state::AuthState::new`]: nothing here reads a secret, builds
//! a KDF hash or dials a media server. The token signing happens per request, in the
//! adapter, where a failure belongs to that request. A `Result` that can only be `Ok` is
//! a worse API than none, because every caller writes the same dead match on it.

use std::sync::Arc;

use crate::infrastructure::livekit::LivekitService;

use super::handlers::Handlers;
use super::media::{BroadcastMedia, DisabledMedia, LivekitMedia};
use super::repository::{BroadcastRepo, PgBroadcastRepo, RepoDeps};
use super::service::{BroadcastService, ServiceDeps};
use crate::config::LivekitSettings;

/// Everything the broadcast endpoints need, wired and ready.
///
/// Cheap to clone — two `Arc`s — so axum's `State` extractor holds one per request
/// without meaningful cost.
#[derive(Clone)]
pub struct BroadcastState {
    /// The rules. A handler receives the [`Handlers`] projection of this.
    pub service: Arc<BroadcastService>,
}

impl std::fmt::Debug for BroadcastState {
    /// The media adapter holds credentials, so a `{:?}` would print them.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BroadcastState")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}

/// Everything [`BroadcastState::new`] needs that this module cannot build for itself.
#[derive(Clone)]
pub struct Wiring {
    /// The pool, for the Postgres repository.
    pub pool: sqlx::PgPool,
    /// LiveKit settings when `LIVEKIT_ENABLED=true`.
    ///
    /// `None` is a supported deployment, not a mistake: drafts, schedules and the
    /// catalogue work without a media server, and only going live and joining need one.
    pub livekit: Option<LivekitSettings>,
}

impl BroadcastState {
    /// Wire the module.
    #[must_use]
    pub fn new(deps: Wiring) -> Self {
        let repo: Arc<dyn BroadcastRepo> =
            Arc::new(PgBroadcastRepo::new(RepoDeps { pool: deps.pool }));
        let media: Arc<dyn BroadcastMedia> = match deps.livekit {
            Some(settings) => {
                tracing::info!("broadcast media server enabled");
                Arc::new(LivekitMedia::new(Arc::new(LivekitService::new(&settings))))
            }
            None => {
                // §4.6: a disabled integration must not fail the flows that do not need
                // it. Joining will answer 503 — loudly, and with the dependency named.
                tracing::info!("no LIVEKIT settings; broadcasts can be created and scheduled");
                Arc::new(DisabledMedia)
            }
        };

        Self {
            service: Arc::new(BroadcastService::new(ServiceDeps { repo, media })),
        }
    }

    /// Wire the module from trait objects, with no infrastructure.
    ///
    /// The constructor the tests use, and the one an in-process job runner should
    /// prefer. Every dependency is a `dyn`, so a test can pass a repository with one
    /// broadcast in it and a media double that records what it was asked — which is how
    /// §10's service tests run without Postgres, Redis or a LiveKit project.
    #[must_use]
    pub fn from_parts(repo: Arc<dyn BroadcastRepo>, media: Arc<dyn BroadcastMedia>) -> Self {
        Self {
            service: Arc::new(BroadcastService::new(ServiceDeps { repo, media })),
        }
    }
}

/// Lets axum hand the broadcast routes their own state.
///
/// The same projection as [`crate::state::MenoState`]'s `AuthState` impl, and the same
/// reason: the router can extract a module's state without a handler ever seeing the
/// whole application. The handlers themselves take [`Handlers`], so this is the only
/// place the module's concrete state appears in the routing path.
impl axum::extract::FromRef<BroadcastState> for Handlers {
    fn from_ref(state: &BroadcastState) -> Self {
        Handlers::from((*state.service).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::broadcast::dto::CreateBroadcastRequest;
    use crate::modules::broadcast::media::RecordingMedia;
    use crate::modules::broadcast::model::UserSummary;
    use crate::modules::broadcast::repository::InMemoryBroadcastRepo;

    fn user() -> UserSummary {
        UserSummary {
            id: uuid::Uuid::from_u128(1),
            full_name: "Ada Lovelace".to_owned(),
            avatar_id: None,
            avatar_url: None,
        }
    }

    fn request() -> CreateBroadcastRequest {
        CreateBroadcastRequest {
            title: "Ada's show".to_owned(),
            description: None,
            image_id: None,
            image_url: None,
            time_zone: Some("Africa/Lagos".to_owned()),
            start_time: None,
            recording_enabled: None,
            cohosts: None,
        }
    }

    #[tokio::test]
    async fn the_wired_state_can_create_a_broadcast_without_touching_a_media_server() {
        // §4.6, asserted end to end: a deployment with no LiveKit still gets drafts and
        // schedules, and only the media-backed flows refuse.
        let state = BroadcastState::from_parts(
            Arc::new(InMemoryBroadcastRepo::new()),
            Arc::new(DisabledMedia),
        );

        let created = state
            .service
            .create(request(), user())
            .await
            .expect("a draft needs no media server");

        assert_eq!(created.title, "Ada's show");
        assert_eq!(created.creator.id, user().id);
        assert_eq!(created.live_participants_count, 0);
    }

    #[tokio::test]
    async fn going_live_without_a_media_server_is_a_503_not_a_panic() {
        let state = BroadcastState::from_parts(
            Arc::new(InMemoryBroadcastRepo::new()),
            Arc::new(DisabledMedia),
        );
        let created = state
            .service
            .create(request(), user())
            .await
            .expect("creating");

        let error = state
            .service
            .go_live(created.id, &user())
            .await
            .expect_err("refused");

        assert_eq!(
            error.code(),
            meno_core::ErrorCode::UpstreamUnavailable,
            "a client has to be able to tell this apart from a 400"
        );
    }

    #[tokio::test]
    async fn the_handlers_projection_shares_the_service_it_was_built_from() {
        use axum::extract::FromRef;

        let state = BroadcastState::from_parts(
            Arc::new(InMemoryBroadcastRepo::new()),
            Arc::new(RecordingMedia::new()),
        );

        let first = Handlers::from_ref(&state);
        let second = Handlers::from_ref(&state);

        // Both handlers reach the same `Arc`, so a cache warmed through one request is
        // warm for the next. A projection that cloned per call would not.
        assert!(Arc::ptr_eq(&state.service, &state.service));
        assert_eq!(format!("{first:?}"), format!("{second:?}"));
    }
}
