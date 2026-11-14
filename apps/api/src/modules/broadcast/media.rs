//! The media server, as the broadcast module is allowed to see it.
//!
//! # Why a trait and not `LivekitService` directly
//!
//! §5.5 is dependency inversion: the module states what it needs, and the adapter that
//! implements it lives below. Naming [`LivekitService`] in a service signature would
//! make the media vendor part of the domain's vocabulary, and §10's service tests would
//! need a LiveKit Cloud project to assert that a host gets a host token.
//!
//! The trait is deliberately five methods wide — mint, create room, evict, tear down,
//! and ask who is in the room — because those are the only five things a broadcast is
//! allowed to ask a media server to do. Anything richer belongs behind the trait, where
//! a test cannot accidentally depend on it.
//!
//! # Why presence comes from the media server
//!
//! `broadcast_participants.left_at` is written by a client that leaves politely, and a
//! phone that crashed, lost its network or was killed never does. The room server, by
//! contrast, drops a participant the moment its connection times out — so it, and not
//! the roster table, is the only honest answer to *"is anybody in this broadcast?"*.
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

    /// The identities currently connected to `broadcast_id`'s room.
    ///
    /// An identity is the account's id, which is what a token is minted with. A room
    /// that no longer exists is an empty room, not an error: nothing is in it, which is
    /// exactly what the caller asked.
    ///
    /// # Errors
    ///
    /// [`MenoError::Upstream`] when the media server is unreachable, so a sweep that
    /// cannot see a room never mistakes "we could not ask" for "nobody is there".
    async fn room_participants(&self, broadcast_id: Uuid) -> Result<Vec<Uuid>, MenoError>;
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

/// Whether the media server said the room does not exist.
///
/// The SDK carries an HTTP/twirp code inside its error text rather than as a value this
/// adapter can match on, so the check is on the spelling. It is deliberately narrow: a
/// false negative only postpones an ending to the next sweep, while a false positive
/// would end a broadcast whose room is very much alive, so the patterns name the code
/// and the phrase rather than guessing from a 404 anywhere in the text.
fn is_missing_room(error: &LivekitError) -> bool {
    let LivekitError::Upstream(detail) = error else {
        return false;
    };
    let detail = detail.to_ascii_lowercase();

    detail.contains("not_found")
        || detail.contains("not found")
        || detail.contains("does not exist")
        || detail.contains("room not found")
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

    async fn room_participants(&self, broadcast_id: Uuid) -> Result<Vec<Uuid>, MenoError> {
        match self.inner.list_participants(broadcast_id).await {
            Ok(participants) => Ok(participants.into_iter().map(|p| p.id).collect()),
            // A room that has already been torn down has nobody in it, which is the
            // answer this method exists to give. This is not a nicety: LiveKit deletes
            // a room five idle minutes after the last participant leaves, so the room
            // of the crashed host the reaper is looking for is *gone* by the time the
            // grace period is up. Reporting "not found" as an outage would leave every
            // abandoned broadcast on air forever — the bug this whole module is for.
            Err(failure) if is_missing_room(&failure) => Ok(Vec::new()),
            // Anything else is an outage, and an outage is not an answer: a sweep must
            // not end a broadcast because it could not ask about it.
            Err(failure) => Err(upstream("room_participants", failure)),
        }
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

    async fn room_participants(&self, _broadcast_id: Uuid) -> Result<Vec<Uuid>, MenoError> {
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
    /// Who is in each room, by broadcast id. Absent means an empty room.
    presence: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<Uuid, Vec<Uuid>>>>,
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
            presence: std::sync::Arc::default(),
        }
    }

    /// Put [who] in [broadcast_id]'s room, as the room server would report them.
    pub fn set_room_participants(&self, broadcast_id: Uuid, who: &[Uuid]) {
        self.presence
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(broadcast_id, who.to_vec());
    }

    /// Take everybody out of [broadcast_id]'s room.
    pub fn clear_room(&self, broadcast_id: Uuid) {
        self.presence
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&broadcast_id);
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

    async fn room_participants(&self, broadcast_id: Uuid) -> Result<Vec<Uuid>, MenoError> {
        // A presence read is not a call the service tests are asserting; recording it
        // would make `calls()` depend on how often the reaper ran.
        if self.fail_with.is_some() {
            return Err(error::media_unavailable());
        }

        Ok(self
            .presence
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&broadcast_id)
            .cloned()
            .unwrap_or_default())
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
    async fn a_room_that_no_longer_exists_reports_nobody_rather_than_an_outage() {
        // The whole reaping rule depends on this: LiveKit deletes an empty room after
        // five minutes, so "the room is gone" is the normal state of the broadcast the
        // reaper is about to end.
        assert!(is_missing_room(&LivekitError::Upstream(
            "twirp error: code = not_found: room not found".to_owned()
        )));
        assert!(!is_missing_room(&LivekitError::Upstream(
            "connection refused".to_owned()
        )));
        assert!(!is_missing_room(&LivekitError::CircuitOpen));
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
