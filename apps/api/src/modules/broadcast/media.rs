//! The media server, as the broadcast module is allowed to see it.
//!
//! # Why a trait and not `LivekitService` directly
//!
//! §5.5 is dependency inversion: the module states what it needs, and the adapter that
//! implements it lives below. Naming [`LivekitService`] in a service signature would
//! make the media vendor part of the domain's vocabulary, and §10's service tests would
//! need a LiveKit Cloud project to assert that a host gets a host token.
//!
//! The trait is deliberately four methods wide — mint, create room, evict, tear down —
//! because those are the only four things a broadcast is allowed to ask a media server
//! to do. Anything richer belongs behind the trait, where a test cannot accidentally
//! depend on it.
//!
//! # What a failure means
//!
//! Every method returns [`MenoError::Upstream`], so a media outage is a 503 with the
//! dependency named and no vendor text (§4.2, §4.6). A *disabled* media server is not
//! an error the caller can act on differently from a down one, so
//! [`DisabledMedia`] refuses rather than pretending to mint a token that would not work.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use super::error;
use super::model::{ParticipantRole, UserSummary};
use crate::infrastructure::livekit::{LivekitError, LivekitService};
use meno_core::Error as MenoError;

/// What the broadcast module needs from a media server (plan §5.5).
#[async_trait]
pub trait BroadcastMedia: Send + Sync + std::fmt::Debug {
    /// Mint a join token for `user` in `broadcast_id`'s room, with `role`.
    ///
    /// `role` is the *module's* role, and the adapter maps it to the vendor's grant
    /// vocabulary. Doing that mapping here is what keeps a LiveKit grant struct out of
    /// the domain.
    ///
    /// # Errors
    ///
    /// [`MenoError::Upstream`] when the media server is disabled, unreachable, or the
    /// token cannot be signed.
    async fn mint_join_token(
        &self,
        user: &UserSummary,
        broadcast_id: Uuid,
        role: ParticipantRole,
    ) -> Result<String, MenoError>;

    /// Make sure the room exists. Idempotent.
    ///
    /// # Errors
    ///
    /// [`MenoError::Upstream`] when the media server refuses.
    async fn create_room(&self, broadcast_id: Uuid) -> Result<(), MenoError>;

    /// Evict one participant from a live room.
    ///
    /// # Errors
    ///
    /// [`MenoError::Upstream`] when the media server refuses.
    async fn remove_participant(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<(), MenoError>;

    /// Tear a room down after the broadcast ended.
    ///
    /// # Errors
    ///
    /// [`MenoError::Upstream`] when the media server refuses.
    async fn end_room(&self, broadcast_id: Uuid) -> Result<(), MenoError>;
}

/// The real adapter: LiveKit, through the shared client and its circuit breaker.
#[derive(Debug, Clone)]
pub struct LivekitMedia {
    inner: Arc<LivekitService>,
}

impl LivekitMedia {
    /// Wrap the shared LiveKit client.
    #[must_use]
    pub fn new(inner: Arc<LivekitService>) -> Self {
        Self { inner }
    }
}

/// Every `LivekitError` becomes a 503 with the vendor's text in the log only.
fn upstream(context: &'static str, error: LivekitError) -> MenoError {
    tracing::warn!(%error, context, "media server call failed");
    error::media_unavailable()
}

#[async_trait]
impl BroadcastMedia for LivekitMedia {
    async fn mint_join_token(
        &self,
        user: &UserSummary,
        broadcast_id: Uuid,
        role: ParticipantRole,
    ) -> Result<String, MenoError> {
        self.inner
            .mint_token(user.id, &user.full_name, broadcast_id, role.to_livekit())
            .map_err(|e| upstream("mint_join_token", e))
    }

    async fn create_room(&self, broadcast_id: Uuid) -> Result<(), MenoError> {
        self.inner
            .create_room(broadcast_id)
            .await
            .map_err(|e| upstream("create_room", e))
    }

    async fn remove_participant(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<(), MenoError> {
        self.inner
            .remove_participant(broadcast_id, participant_id)
            .await
            .map_err(|e| upstream("remove_participant", e))
    }

    async fn end_room(&self, broadcast_id: Uuid) -> Result<(), MenoError> {
        self.inner
            .delete_room(broadcast_id)
            .await
            .map_err(|e| upstream("end_room", e))
    }
}

/// The adapter for a deployment with `LIVEKIT_ENABLED=false`.
///
/// It refuses every call with a 503 rather than minting a fake token. A token that
/// does not work is worse than an honest failure: the client would connect, publish
/// nothing, and report a bug that is really a configuration gap.
#[derive(Debug, Clone, Copy, Default)]
pub struct DisabledMedia;

#[async_trait]
impl BroadcastMedia for DisabledMedia {
    async fn mint_join_token(
        &self,
        _user: &UserSummary,
        _broadcast_id: Uuid,
        _role: ParticipantRole,
    ) -> Result<String, MenoError> {
        Err(error::media_unavailable())
    }

    async fn create_room(&self, _broadcast_id: Uuid) -> Result<(), MenoError> {
        Err(error::media_unavailable())
    }

    async fn remove_participant(
        &self,
        _broadcast_id: Uuid,
        _participant_id: Uuid,
    ) -> Result<(), MenoError> {
        Err(error::media_unavailable())
    }

    async fn end_room(&self, _broadcast_id: Uuid) -> Result<(), MenoError> {
        Err(error::media_unavailable())
    }
}

/// A media server that records what it was asked for and succeeds.
///
/// A real implementation of [`BroadcastMedia`], not test scaffolding: it backs the
/// `tests/` suite, which links `meno_api` as an external crate where `#[cfg(test)]` is
/// false. Gated on `test-support` so a production build compiles none of it.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Default)]
pub struct RecordingMedia {
    /// Every call, in order, so a test can assert what the service asked for.
    calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// Set to make the next call fail, for the §4.6 "disabled integration" path.
    fail_with: Option<String>,
}

#[cfg(any(test, feature = "test-support"))]
impl RecordingMedia {
    /// A media server that succeeds at everything.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A media server that fails every call, for the outage path.
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            calls: std::sync::Arc::default(),
            fail_with: Some("media down".to_owned()),
        }
    }

    /// What the media server was asked for, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn record(&self, call: String) -> Result<(), MenoError> {
        let mut calls = self.calls.lock().unwrap_or_else(|e| e.into_inner());
        calls.push(call);
        drop(calls);

        match &self.fail_with {
            Some(_) => Err(error::media_unavailable()),
            None => Ok(()),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
#[async_trait]
impl BroadcastMedia for RecordingMedia {
    async fn mint_join_token(
        &self,
        user: &UserSummary,
        broadcast_id: Uuid,
        role: ParticipantRole,
    ) -> Result<String, MenoError> {
        self.record(format!("mint:{broadcast_id}:{}:{role}", user.id))?;
        Ok(format!("test-token-{}-{}", broadcast_id, user.id))
    }

    async fn create_room(&self, broadcast_id: Uuid) -> Result<(), MenoError> {
        self.record(format!("create_room:{broadcast_id}"))
    }

    async fn remove_participant(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<(), MenoError> {
        self.record(format!("remove:{broadcast_id}:{participant_id}"))
    }

    async fn end_room(&self, broadcast_id: Uuid) -> Result<(), MenoError> {
        self.record(format!("end_room:{broadcast_id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> UserSummary {
        UserSummary {
            id: Uuid::from_u128(1),
            full_name: "Ada".to_owned(),
            avatar_id: None,
            avatar_url: None,
        }
    }

    #[tokio::test]
    async fn the_recording_double_reports_what_it_was_asked_for() {
        // The property that makes §10's service tests worth anything: the service's
        // media calls are observable, so "the host got a host token" is assertable
        // without a LiveKit project.
        let media = RecordingMedia::new();

        media
            .mint_join_token(&user(), Uuid::from_u128(5), ParticipantRole::Host)
            .await
            .expect("minting");
        media
            .create_room(Uuid::from_u128(5))
            .await
            .expect("creating");
        media.end_room(Uuid::from_u128(5)).await.expect("ending");

        assert_eq!(
            media.calls(),
            [
                format!("mint:{}:{}:host", Uuid::from_u128(5), user().id),
                format!("create_room:{}", Uuid::from_u128(5)),
                format!("end_room:{}", Uuid::from_u128(5)),
            ]
        );
    }

    #[tokio::test]
    async fn a_disabled_media_server_refuses_rather_than_minting_a_useless_token() {
        // §4.6: a disabled integration must not fail the flows that would use it, but
        // it must not fake them either. A token that does not work is a support ticket
        // about a bug that does not exist.
        let media = DisabledMedia;

        let error = media
            .mint_join_token(&user(), Uuid::from_u128(5), ParticipantRole::Participant)
            .await
            .expect_err("refused");

        assert_eq!(error.code(), meno_core::ErrorCode::UpstreamUnavailable);
        assert_eq!(
            meno_core::to_body(&error).http_status,
            503,
            "an unset LIVEKIT_URL is a 503, not a 400 and not a silent success"
        );
    }

    #[tokio::test]
    async fn an_outage_is_a_503_and_not_a_panic() {
        let media = RecordingMedia::unavailable();

        let error = media
            .create_room(Uuid::from_u128(5))
            .await
            .expect_err("refused");

        assert_eq!(error.code(), meno_core::ErrorCode::UpstreamUnavailable);
        assert_eq!(
            meno_core::to_body(&error).http_status,
            503,
            "the client has to be able to tell this apart from a 400"
        );
    }
}
