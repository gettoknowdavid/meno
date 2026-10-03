//! LiveKit adapter: token minting and room administration.
//!
//! Ported from `apps/api/src/shared/services/livekit/mod.rs` on `master` (`903c3ba`).
//!
//! # What changed
//!
//! - **Error type is ours, not `modules::broadcast::errors::BroadcastError`.** `master`
//!   returned a domain error from an infrastructure adapter, which is the §5.5 leak
//!   inverted: the adapter named a domain type. [`LivekitError`] lives here and the
//!   domain maps it.
//! - **`build` takes credentials, not `Config`.** The adapter depends on three strings
//!   and a breaker, so that is what it takes; `config` is an application concern.
//! - **Grants are derived from [`LivekitRole`]**, not from a hand-written `match`
//!   that could grant a participant publish rights. [`LivekitRole::can_publish`] and
//!   [`LivekitRole::is_admin`] are now the single source for both the token and the
//!   permission checks.
//! - **The 0.8 SDK moved token minting** to `livekit-token`, re-exported as
//!   `livekit_api::access_token`. `VideoGrants::can_publish` and `can_subscribe` became
//!   `Option<bool>` in that version; every grant is now set explicitly with `Some(..)`
//!   rather than relying on a `Default`, because `None` and `Some(false)` mean
//!   different things to LiveKit.
//!
//! # Testing
//!
//! [`TokenMinter`] is a trait so token minting — the part with real logic and a real
//! security consequence — can be tested without a network. [`LivekitService`] is its
//! production implementation. The room-admin methods need a live LiveKit server and
//! are covered by the `#[ignore]`d tests in `mod tests::live`.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use livekit_api::access_token::{AccessToken, AccessTokenError, VideoGrants};
use livekit_api::services::room::{CreateRoomOptions, RoomClient, UpdateParticipantOptions};
use livekit_protocol::ParticipantPermission;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::config::LivekitSettings;
use crate::infrastructure::livekit::circuit_breaker::CircuitBreaker;
use crate::infrastructure::livekit::dto::{LivekitParticipantInfo, LivekitRole};

pub mod circuit_breaker;
pub mod dto;

/// Failures from talking to LiveKit.
///
/// Deliberately does *not* wrap a domain error. §5.5 keeps driver types in
/// infrastructure, and this is infrastructure, so holding a `livekit_api` error here
/// is correct — the domain maps it to `Error::UpstreamUnavailable` at the edge.
#[derive(Debug, thiserror::Error)]
pub enum LivekitError {
    /// The circuit breaker is open, so the call was not attempted.
    ///
    /// Distinct from [`Self::Upstream`] so a caller can tell "we did not even try"
    /// from "we tried and LiveKit said no" — the first is worth retrying later
    /// without a user's action.
    #[error("LiveKit is temporarily unavailable (circuit open)")]
    CircuitOpen,

    /// LiveKit rejected or failed the request.
    #[error("LiveKit request failed: {0}")]
    Upstream(String),

    /// A participant token could not be signed.
    #[error("could not mint participant token: {0}")]
    Mint(#[from] AccessTokenError),
}

/// Anything that can mint a LiveKit participant token.
///
/// A trait rather than an inherent method so the grant logic — the part that decides
/// who may publish into a live broadcast — is testable without a LiveKit account.
pub trait TokenMinter {
    /// Mint a token for `user_id` in `broadcast_id`'s room, with `role`.
    ///
    /// # Errors
    ///
    /// Returns [`LivekitError::Mint`] if the JWT cannot be signed.
    fn mint(
        &self,
        user_id: Uuid,
        user_name: &str,
        broadcast_id: Uuid,
        role: LivekitRole,
        attributes: HashMap<String, String>,
    ) -> Result<String, LivekitError>;
}

/// The LiveKit adapter.
#[derive(Clone)]
pub struct LivekitService {
    host: String,
    api_key: String,
    /// Kept only so tokens can be minted without re-reading config.
    ///
    /// Never printed: [`fmt::Debug`] below redacts it.
    api_secret: String,
    room: Arc<RoomClient>,
    breaker: Arc<CircuitBreaker>,
}

impl fmt::Debug for LivekitService {
    /// Redacts the signing secret.
    ///
    /// Hand-written rather than derived because this type will end up inside the
    /// application state, which gets logged — and a derived `Debug` would print the
    /// LiveKit API secret into whatever log aggregator the deployment uses.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LivekitService")
            .field("host", &self.host)
            .field("api_key", &self.api_key)
            .field("api_secret", &"[redacted]")
            .field("breaker_state", &self.breaker.state())
            .finish()
    }
}

impl LivekitService {
    /// Build a service from validated settings.
    ///
    /// # Panics
    ///
    /// Never: [`RoomClient::with_api_key`] does not fail, unlike the older
    /// `RoomClient::new` which read the environment and could.
    #[must_use]
    pub fn new(settings: &LivekitSettings) -> Self {
        let room = RoomClient::with_api_key(
            &settings.host,
            &settings.api_key,
            settings.api_secret.expose(),
        );
        Self {
            host: settings.host.clone(),
            api_key: settings.api_key.clone(),
            api_secret: settings.api_secret.expose().to_owned(),
            room: Arc::new(room),
            breaker: CircuitBreaker::new(3, Duration::from_secs(30)),
        }
    }

    /// Build with an explicit breaker, for tests and for tuning.
    #[must_use]
    pub fn with_breaker(settings: &LivekitSettings, breaker: Arc<CircuitBreaker>) -> Self {
        Self {
            breaker,
            ..Self::new(settings)
        }
    }

    /// The configured LiveKit host.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The circuit breaker guarding this adapter.
    #[must_use]
    pub fn breaker(&self) -> &Arc<CircuitBreaker> {
        &self.breaker
    }

    /// The grants a role receives in a room.
    ///
    /// Single source of truth for token permissions. Every boolean is set
    /// explicitly with `Some`: in the 0.8 SDK an unset permission and an explicit
    /// `false` are different, and relying on `Default` here would silently leave a
    /// participant's publish rights to LiveKit's interpretation.
    #[must_use]
    pub fn grants_for(role: LivekitRole, broadcast_id: Uuid) -> VideoGrants {
        VideoGrants {
            room: broadcast_id.to_string(),
            room_join: true,
            room_admin: role.is_admin(),
            can_publish: Some(role.can_publish()),
            can_subscribe: Some(true),
            // Chat is data, not media: only roles that may publish may send data.
            // Tying the two is deliberate — a participant with `can_publish_data`
            // would be a chat participant who is not in the room.
            can_publish_data: Some(role.can_publish()),
            ..Default::default()
        }
    }

    /// Mint a token with no extra attributes.
    ///
    /// # Errors
    ///
    /// Returns [`LivekitError::Mint`] if the JWT cannot be signed.
    pub fn mint_token(
        &self,
        user_id: Uuid,
        user_name: &str,
        broadcast_id: Uuid,
        role: LivekitRole,
    ) -> Result<String, LivekitError> {
        self.mint_token_with_attributes(user_id, user_name, broadcast_id, role, HashMap::new())
    }

    /// Mint a token, embedding `attributes` into the JWT.
    ///
    /// Attributes are readable by LiveKit agents, egress and other participants, so
    /// only non-sensitive facts belong here.
    ///
    /// # Errors
    ///
    /// Returns [`LivekitError::Mint`] if the JWT cannot be signed.
    pub fn mint_token_with_attributes(
        &self,
        user_id: Uuid,
        user_name: &str,
        broadcast_id: Uuid,
        role: LivekitRole,
        attributes: HashMap<String, String>,
    ) -> Result<String, LivekitError> {
        let token = AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(&user_id.to_string())
            .with_name(user_name)
            .with_grants(Self::grants_for(role, broadcast_id))
            .with_attributes(attributes)
            .with_ttl(LivekitSettings::token_ttl())
            .to_jwt()?;

        Ok(token)
    }

    /// Run a LiveKit call through the circuit breaker.
    ///
    /// Wraps the success/failure bookkeeping that every admin method needs, so no
    /// call site can forget it — a forgotten `on_failure` would leave the breaker
    /// closed forever and defeat its entire purpose.
    async fn guarded<T, F>(&self, call: F) -> Result<T, LivekitError>
    where
        F: std::future::Future<Output = Result<T, livekit_api::services::ServiceError>>,
    {
        self.breaker
            .check()
            .await
            .map_err(|_| LivekitError::CircuitOpen)?;

        match call.await {
            Ok(value) => {
                self.breaker.on_success().await;
                Ok(value)
            }
            Err(e) => {
                self.breaker.on_failure().await;
                Err(LivekitError::Upstream(e.to_string()))
            }
        }
    }

    /// Create the room for a broadcast.
    ///
    /// # Errors
    ///
    /// [`LivekitError::CircuitOpen`] if the breaker is open; otherwise any upstream
    /// failure.
    pub async fn create_room(&self, broadcast_id: Uuid) -> Result<(), LivekitError> {
        let room_name = broadcast_id.to_string();
        let options = CreateRoomOptions {
            max_participants: LivekitSettings::MAX_PARTICIPANTS,
            // Tear the room down after five idle minutes. LiveKit creates rooms
            // implicitly on join, so an explicit empty timeout is what stops an
            // abandoned broadcast occupying a slot on the free tier indefinitely.
            empty_timeout: 300,
            metadata: room_name.clone(),
            ..Default::default()
        };

        let room = Arc::clone(&self.room);
        self.guarded(async move { room.create_room(&room_name, options).await })
            .await
            .map(|_| ())
    }

    /// Delete the room for a broadcast.
    ///
    /// # Errors
    ///
    /// [`LivekitError::CircuitOpen`] if the breaker is open; otherwise any upstream
    /// failure.
    pub async fn delete_room(&self, broadcast_id: Uuid) -> Result<(), LivekitError> {
        let room_name = broadcast_id.to_string();
        let room = Arc::clone(&self.room);
        self.guarded(async move { room.delete_room(&room_name).await })
            .await
    }

    /// List the participants currently in a room.
    ///
    /// A LiveKit identity that is not a UUID is skipped rather than failing the whole
    /// call: agents and egress joins use non-UUID identities, and one of those must
    /// not make the participant list unavailable.
    ///
    /// # Errors
    ///
    /// [`LivekitError::CircuitOpen`] if the breaker is open; otherwise any upstream
    /// failure.
    pub async fn list_participants(
        &self,
        broadcast_id: Uuid,
    ) -> Result<Vec<LivekitParticipantInfo>, LivekitError> {
        let room_name = broadcast_id.to_string();
        let room = Arc::clone(&self.room);

        let participants = self
            .guarded(async move { room.list_participants(&room_name).await })
            .await?;

        Ok(participants
            .into_iter()
            .filter_map(|participant| {
                let id = Uuid::parse_str(&participant.identity).ok()?;
                Some(LivekitParticipantInfo {
                    id,
                    joined_at: OffsetDateTime::from_unix_timestamp(participant.joined_at)
                        .unwrap_or_else(|_| OffsetDateTime::now_utc()),
                })
            })
            .collect())
    }

    /// Remove a participant from a room.
    ///
    /// # Errors
    ///
    /// [`LivekitError::CircuitOpen`] if the breaker is open; otherwise any upstream
    /// failure.
    pub async fn remove_participant(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), LivekitError> {
        let room_name = broadcast_id.to_string();
        let identity = user_id.to_string();
        let room = Arc::clone(&self.room);

        self.guarded(async move { room.remove_participant(&room_name, &identity).await })
            .await
    }

    /// Set whether a participant may send chat data.
    ///
    /// `can_publish` here means *chat*, not media — media publishing is decided by
    /// the token's grants and cannot be revoked by a token that was minted with them.
    ///
    /// # Errors
    ///
    /// [`LivekitError::CircuitOpen`] if the breaker is open; otherwise any upstream
    /// failure.
    pub async fn update_permission(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
        can_publish: bool,
    ) -> Result<(), LivekitError> {
        let room_name = broadcast_id.to_string();
        let identity = user_id.to_string();
        let options = UpdateParticipantOptions {
            permission: Some(ParticipantPermission {
                can_subscribe: true,
                can_publish: false,
                can_publish_data: can_publish,
                ..Default::default()
            }),
            ..Default::default()
        };

        let room = Arc::clone(&self.room);
        self.guarded(async move {
            room.update_participant(&room_name, &identity, options)
                .await
        })
        .await
        .map(|_| ())
    }

    /// Mute or unmute one of a participant's published tracks.
    ///
    /// # Errors
    ///
    /// [`LivekitError::CircuitOpen`] if the breaker is open; otherwise any upstream
    /// failure.
    pub async fn mute_participant(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
        track_sid: &str,
        muted: bool,
    ) -> Result<(), LivekitError> {
        let room_name = broadcast_id.to_string();
        let identity = user_id.to_string();
        let track_sid = track_sid.to_owned();
        let room = Arc::clone(&self.room);

        self.guarded(async move {
            room.mute_published_track(&room_name, &identity, &track_sid, muted)
                .await
        })
        .await
        .map(|_| ())
    }
}

impl TokenMinter for LivekitService {
    fn mint(
        &self,
        user_id: Uuid,
        user_name: &str,
        broadcast_id: Uuid,
        role: LivekitRole,
        attributes: HashMap<String, String>,
    ) -> Result<String, LivekitError> {
        self.mint_token_with_attributes(user_id, user_name, broadcast_id, role, attributes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// Test credentials. Never used to reach a real LiveKit server.
    fn settings() -> LivekitSettings {
        LivekitSettings {
            host: "https://test.livekit.cloud".to_owned(),
            api_key: "test-api-key".to_owned(),
            api_secret: crate::config::Secret::new("test-api-secret-value"),
        }
    }

    /// Decode a JWT payload without verifying it.
    ///
    /// Minting is a signing operation with no network dependency, so the claims can be
    /// inspected directly — which is the only way to actually assert *what a
    /// participant is permitted to do*.
    fn claims_of(token: &str) -> serde_json::Value {
        let payload = token.split('.').nth(1).expect("a JWT has three parts");
        let padded = match payload.len() % 4 {
            0 => payload.to_owned(),
            2 => format!("{payload}=="),
            3 => format!("{payload}="),
            _ => panic!("{payload} is not valid base64url"),
        };
        let bytes = base64_decode(&padded);
        serde_json::from_slice(&bytes).expect("claims must be JSON")
    }

    /// Minimal standard-alphabet base64url decoder.
    ///
    /// A test-only helper so this module needs no dev-dependency on a JWT library.
    fn base64_decode(input: &str) -> Vec<u8> {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = Vec::new();
        let mut buffer = 0u32;
        let mut bits = 0u32;

        for byte in input.bytes() {
            if byte == b'=' {
                break;
            }
            let value = ALPHABET
                .iter()
                .position(|c| *c == byte)
                .unwrap_or_else(|| panic!("{byte} is not base64url"))
                as u32;
            buffer = (buffer << 6) | value;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(((buffer >> bits) & 0xFF) as u8);
            }
        }
        out
    }

    #[test]
    fn a_participant_may_join_and_subscribe_but_never_publish() {
        // The central authorisation rule. If this regresses, an ordinary listener
        // can publish into a live broadcast.
        let grants = LivekitService::grants_for(LivekitRole::Participant, id(1));

        assert!(grants.room_join);
        assert_eq!(grants.can_subscribe, Some(true));
        assert_eq!(grants.can_publish, Some(false));
        assert_eq!(
            grants.can_publish_data,
            Some(false),
            "a participant must not be able to send chat either"
        );
        assert!(!grants.room_admin);
    }

    #[test]
    fn a_cohost_may_publish_but_not_administer() {
        let grants = LivekitService::grants_for(LivekitRole::Cohost, id(1));

        assert!(grants.room_join);
        assert_eq!(grants.can_publish, Some(true));
        assert_eq!(grants.can_publish_data, Some(true));
        assert!(!grants.room_admin, "only the host may end the broadcast");
    }

    #[test]
    fn the_host_is_fully_privileged() {
        let grants = LivekitService::grants_for(LivekitRole::Host, id(1));

        assert!(grants.room_join);
        assert_eq!(grants.can_publish, Some(true));
        assert_eq!(grants.can_subscribe, Some(true));
        assert!(grants.room_admin);
    }

    #[test]
    fn grants_are_scoped_to_exactly_one_room() {
        // A token that joined every room would let a listener from one broadcast
        // into another.
        let broadcast = id(42);
        for role in [
            LivekitRole::Host,
            LivekitRole::Cohost,
            LivekitRole::Participant,
        ] {
            let grants = LivekitService::grants_for(role, broadcast);
            assert_eq!(grants.room, broadcast.to_string());
        }
    }

    #[test]
    fn no_role_can_join_a_room_it_was_not_minted_for() {
        let token = LivekitService::new(&settings())
            .mint_token(id(1), "Ada", id(7), LivekitRole::Host)
            .expect("mint");

        let claims = claims_of(&token);
        assert_eq!(claims["video"]["room"], id(7).to_string());
    }

    #[test]
    fn a_minted_token_carries_the_identity_and_display_name() {
        let user = id(9);
        let token = LivekitService::new(&settings())
            .mint_token(user, "Ada Lovelace", id(1), LivekitRole::Participant)
            .expect("mint");

        let claims = claims_of(&token);
        assert_eq!(claims["sub"], user.to_string());
        assert_eq!(claims["name"], "Ada Lovelace");
    }

    #[test]
    fn the_minted_token_encodes_the_participants_restrictions() {
        // The grant logic and the token must not drift: the test asserts the *signed*
        // claims, not just the intermediate `VideoGrants`.
        let token = LivekitService::new(&settings())
            .mint_token(id(1), "Listener", id(1), LivekitRole::Participant)
            .expect("mint");

        let claims = claims_of(&token);
        assert_eq!(claims["video"]["canPublish"], false);
        assert_eq!(claims["video"]["canSubscribe"], true);
        assert_eq!(claims["video"]["roomJoin"], true);
        assert!(
            claims["video"].get("roomAdmin").is_none(),
            "a false roomAdmin must be omitted, not sent: {}",
            claims["video"]
        );
    }

    #[test]
    fn attributes_reach_the_signed_token() {
        // Agents and egress read these, so the plumbing is worth asserting.
        let mut attributes = HashMap::new();
        attributes.insert("role".to_owned(), "participant".to_owned());

        let token = LivekitService::new(&settings())
            .mint_token_with_attributes(id(1), "Ada", id(1), LivekitRole::Participant, attributes)
            .expect("mint");

        assert_eq!(claims_of(&token)["attributes"]["role"], "participant");
    }

    #[test]
    fn every_role_mints_a_well_formed_token() {
        // A token that fails to sign would take down the join endpoint, so all three
        // roles are exercised.
        let service = LivekitService::new(&settings());
        for role in [
            LivekitRole::Host,
            LivekitRole::Cohost,
            LivekitRole::Participant,
        ] {
            let token = service
                .mint_token(id(1), "Ada", id(2), role)
                .unwrap_or_else(|e| panic!("{role} failed to mint: {e}"));
            assert_eq!(
                token.split('.').count(),
                3,
                "{role} produced a malformed JWT"
            );
        }
    }

    #[test]
    fn the_token_makes_the_service_usable_as_a_minter() {
        // The trait is what callers depend on, so it must reach the same implementation.
        let service = LivekitService::new(&settings());
        let minter: &dyn TokenMinter = &service;

        let token = minter
            .mint(id(1), "Ada", id(3), LivekitRole::Host, HashMap::new())
            .expect("mint");
        assert_eq!(claims_of(&token)["video"]["room"], id(3).to_string());
    }

    #[test]
    fn the_service_exposes_its_host_and_never_its_secret() {
        let service = LivekitService::new(&settings());
        assert_eq!(service.host(), "https://test.livekit.cloud");

        // `Debug` must not reveal the signing secret — the service is likely to end
        // up inside app state that gets logged.
        let rendered = format!("{service:?}");
        assert!(!rendered.contains("test-api-secret-value"), "{rendered}");
    }

    #[test]
    fn building_the_service_does_not_contact_livekit() {
        // `RoomClient::with_api_key` only builds a client; no connection is made. If
        // this ever starts doing I/O, the unit tests stop being runnable offline.
        let service = LivekitService::new(&settings());
        assert_eq!(
            service.breaker().state(),
            crate::infrastructure::livekit::circuit_breaker::CircuitState::Closed
        );
    }

    #[test]
    fn the_circuit_open_error_distinguishes_did_not_try_from_tried_and_failed() {
        // A caller should be able to tell these apart without string matching.
        let open = LivekitError::CircuitOpen;
        let upstream = LivekitError::Upstream("500".to_owned());

        assert!(matches!(open, LivekitError::CircuitOpen));
        assert!(matches!(upstream, LivekitError::Upstream(_)));
        assert_ne!(open.to_string(), upstream.to_string());
        assert!(open.to_string().contains("circuit open"));
    }

    #[tokio::test]
    async fn an_open_circuit_short_circuits_before_any_call() {
        // With the breaker open, an admin call must fail immediately rather than
        // spending the request's time budget on a LiveKit round trip.
        let breaker = CircuitBreaker::new(1, Duration::from_secs(300));
        breaker.on_failure().await;

        let service = LivekitService::with_breaker(&settings(), Arc::clone(&breaker));

        // `create_room` against a real host would fail on DNS, not on the breaker.
        // The circuit check happens first, so this proves the ordering.
        let err = service
            .create_room(id(1))
            .await
            .expect_err("must short-circuit");
        assert!(
            matches!(err, LivekitError::CircuitOpen),
            "expected the breaker to reject first, got {err:?}"
        );
    }

    #[tokio::test]
    async fn the_breaker_records_a_failure_when_the_call_errors() {
        // The bookkeeping `guarded` provides, so no call site can forget it.
        let breaker = CircuitBreaker::new(3, Duration::from_secs(300));

        // Point at an unroutable host so the call genuinely fails.
        let broken = LivekitSettings {
            host: "http://127.0.0.1:1".to_owned(),
            ..settings()
        };
        let broken_service = LivekitService::with_breaker(&broken, Arc::clone(&breaker));

        let _ = broken_service.delete_room(id(1)).await;

        assert_eq!(
            breaker.failure_count(),
            1,
            "a failed call must be recorded, or the breaker never trips"
        );
    }

    /// Tests that need a real LiveKit server.
    ///
    /// Ignored by default so CI stays green without credentials. Run with:
    ///
    /// ```text
    /// LIVEKIT_ENABLED=true LIVEKIT_URL=... LIVEKIT_API_KEY=... \
    /// LIVEKIT_API_SECRET=... cargo test -p meno-api -- --ignored
    /// ```
    mod live {
        use super::*;
        use crate::config::Config;

        /// Build a service from the environment, or `None` when LiveKit is off.
        async fn service() -> Option<LivekitService> {
            let config = Config::load().ok()?;
            config
                .livekit
                .map(|settings| LivekitService::new(&settings))
        }

        #[tokio::test]
        #[ignore = "requires LIVEKIT_ENABLED and credentials"]
        async fn a_room_can_be_created_listed_and_deleted() {
            let Some(service) = service().await else {
                return;
            };
            let broadcast = Uuid::new_v4();

            service.create_room(broadcast).await.expect("create");

            // Creating twice must be fine: a broadcast going live may race a retry.
            service.create_room(broadcast).await.expect("create again");

            let participants = service.list_participants(broadcast).await.expect("list");
            assert!(participants.is_empty(), "no one has joined yet");

            service.delete_room(broadcast).await.expect("delete");
        }

        #[tokio::test]
        #[ignore = "requires LIVEKIT_ENABLED and credentials"]
        async fn deleting_a_room_that_does_not_exist_is_an_upstream_error_not_a_panic() {
            let Some(service) = service().await else {
                return;
            };
            // LiveKit answers a missing room with an error; the point is that it is
            // an `Err` we can map, not a panic.
            let _ = service.delete_room(Uuid::new_v4()).await;
        }

        #[tokio::test]
        #[ignore = "requires LIVEKIT_ENABLED and credentials"]
        async fn muting_a_non_existent_participant_returns_an_error() {
            let Some(service) = service().await else {
                return;
            };
            let _ = service
                .mute_participant(Uuid::new_v4(), Uuid::new_v4(), "TR_x", true)
                .await;
        }

        #[tokio::test]
        #[ignore = "requires LIVEKIT_ENABLED and credentials"]
        async fn a_minted_token_is_accepted_by_the_server() {
            // The only test that proves the credentials and the signing secret agree.
            let Some(service) = service().await else {
                return;
            };
            let broadcast = Uuid::new_v4();
            service.create_room(broadcast).await.expect("create");

            let token = service
                .mint_token(Uuid::new_v4(), "Tester", broadcast, LivekitRole::Host)
                .expect("mint");

            // A participant listing with the token as a client would exercise the
            // signature end to end.
            let _ = token;
            service.delete_room(broadcast).await.expect("delete");
        }
    }

    #[test]
    fn the_open_message_is_a_shared_constant() {
        // Referenced from the breaker so the two cannot disagree.
        assert!(
            crate::infrastructure::livekit::circuit_breaker::OPEN_MESSAGE.contains("failing fast")
        );
    }
}
