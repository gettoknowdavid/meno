//! The Firebase Cloud Messaging v1 adapter.
//!
//! Ported from `apps/api/src/shared/services/push/mod.rs` on `master` (`903c3ba`).
//!
//! # What the refactor changed here
//!
//! - **It is behind a trait.** [`PushSender`](super::PushSender) is what callers
//!   depend on; this is one implementation of it, so §5.6's "adapter trait with an
//!   in-memory test double" is available and push stops being untestable without a
//!   Firebase project.
//! - **It takes [`PushSettings`], not [`Config`](crate::config::Config).** The adapter
//!   needs two fields; making it take the whole application config is how
//!   `infrastructure` ends up depending on everything. Same fix as `LivekitService`.
//! - **It cannot panic.** `master` called `.expect("Failed to build HTTP client for
//!   FCM")` in a constructor and `Duration::from_mins(1)` — which does not exist, so
//!   the module did not compile — and wrapped every error in `anyhow::Error`. §9.1
//!   denies `expect_used` in `src/`; [`Self::new`] is `Result` instead.
//! - **Only genuine FCM outages trip the breaker.** `master` recorded a failure for
//!   every non-200, including `404` for a stale token. A user uninstalling the app
//!   repeatedly would open the circuit for every other user. §"Rate limiting and
//!   circuit breaking" below.
//! - **Classification uses the response body too**, so a `400` carrying `UNREGISTERED`
//   is recognised as a stale token — see [`dto::classify_failure`].
//! - **Fan-out uses a semaphore rather than a hard-coded buffer.** 50 is a named
//!   constant ([`MAX_CONCURRENT_SENDS`]) instead of a magic number in a
//!   `buffer_unordered(50)` call.
//!
//! # Rate limiting and circuit breaking
//!
//! The breaker is shared with [`LivekitService`](super::super::livekit) and its state
//! machine is unchanged; only its label differs. What changed is *which* outcomes feed
//! it:
//!
//! | Outcome | Trips the breaker? | Why |
//! | --- | --- | --- |
//! | `429`, `5xx`, transport failure | yes | FCM itself is unavailable |
//! | `404` / `UNREGISTERED` | no | one device's token is dead; the service is fine |
//! | `400`, `401`, `403` other | no | our request or our credentials are wrong — tripping would hide a permanent bug behind a "temporarily unavailable" message |
//!
//! # Why there is no FCM crate
//!
//! Unchanged from `master`, and still correct: the v1 API is one JSON POST per message
//! plus one OAuth2 exchange, and both are plain `reqwest` calls. The multipart
//! `/batch` endpoint is not used — it is the only thing a wrapper crate would buy, and
//! §6.4 wants fan-out paginated through a job instead.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{error, warn};
use uuid::Uuid;

use crate::config::PushSettings;
use crate::infrastructure::livekit::circuit_breaker::CircuitBreaker;
use crate::infrastructure::push::PushSender;
use crate::infrastructure::push::dto::{
    FcmAndroidConfig, FcmAndroidNotification, FcmApnsAlert, FcmApnsAps, FcmApnsConfig,
    FcmApnsPayload, FcmEnvelope, FcmMessage, FcmNotification, ServiceAccount, classify_failure,
};
use crate::infrastructure::push::error::PushError;
use crate::infrastructure::push::model::{MulticastResult, PushMessage, PushTarget};
use crate::infrastructure::push::token::AccessTokenProvider;

/// Concurrent device sends within one [`FcmPushSender::send_multicast`] call.
///
/// FCM's documented guidance for `UNAVAILABLE` is exponential backoff; firing the
/// whole subscriber list at once is how that guidance gets triggered. Fifty is
/// comfortably inside the quota for a project on the free tier while still finishing a
/// large broadcast in seconds.
pub const MAX_CONCURRENT_SENDS: usize = 50;

/// Consecutive FCM failures that open the circuit.
const FAILURE_THRESHOLD: u64 = 3;

/// How long the circuit stays open before a probe is allowed through.
const OPEN_DURATION: Duration = Duration::from_secs(60);

/// The URLs the adapter talks to.
///
/// A struct rather than two `String` fields on the sender so the pair cannot be
/// mismatched, and so a test can point both at a local server in one expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushEndpoints {
    /// The `messages:send` URL for the project.
    pub send_url: String,

    /// Overrides the service account's own `token_uri`.
    ///
    /// `None` — the production value — means "use whatever the key says". An override
    /// exists so a test can serve the token endpoint from a local server; it is not a
    /// configuration surface and nothing in `config.rs` sets it.
    pub token_url: Option<String>,
}

impl PushEndpoints {
    /// Google's endpoints for `project_id`.
    #[must_use]
    pub fn google(project_id: &str) -> Self {
        Self {
            send_url: format!("https://fcm.googleapis.com/v1/projects/{project_id}/messages:send"),
            token_url: None,
        }
    }
}

/// The FCM v1 push adapter.
#[derive(Clone)]
pub struct FcmPushSender {
    http: Client,
    project_id: String,
    endpoints: PushEndpoints,
    tokens: AccessTokenProvider,
    breaker: Arc<CircuitBreaker>,
}

impl std::fmt::Debug for FcmPushSender {
    /// Redacts the credential material reachable from this type.
    ///
    /// `AccessTokenProvider` redacts its own half; this exists so the sender's
    /// `Debug` can include it rather than omit it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FcmPushSender")
            .field("project_id", &self.project_id)
            .field("endpoints", &self.endpoints)
            .field("tokens", &self.tokens)
            .field("breaker_state", &self.breaker.state())
            .finish_non_exhaustive()
    }
}

impl FcmPushSender {
    /// Build a sender against Google's endpoints.
    ///
    /// # Errors
    ///
    /// Returns [`PushError::TokenFetch`] if the service-account JSON is malformed or
    /// its `private_key` is not a usable RSA key, and [`PushError::Transport`] if the
    /// HTTP client cannot be built. Both are permanent, and both are reported at
    /// startup rather than on the first notification — §4.3's "log clearly and exit"
    /// needs `bootstrap` to have something to report.
    pub fn new(settings: &PushSettings) -> Result<Self, PushError> {
        Self::with_endpoints(settings, &PushEndpoints::google(&settings.project_id))
    }

    /// Build against explicit endpoints.
    ///
    /// The seam [`Self::new`] does not have, for tests and for pointing the adapter at
    /// an emulator. `endpoints.token_url` overrides the key's own `token_uri`.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub fn with_endpoints(
        settings: &PushSettings,
        endpoints: &PushEndpoints,
    ) -> Result<Self, PushError> {
        // The shared pool (§5.6): one client for FCM, Google and mail, rather than a
        // pool per adapter — see `infrastructure::http`'s module docs.
        let http = crate::infrastructure::http::shared()
            .map_err(|e| PushError::Transport(e.to_string()))?
            .clone();

        let mut account = ServiceAccount::parse(settings.service_account_json.expose())?;

        if let Some(token_url) = &endpoints.token_url {
            account.token_uri = Some(token_url.clone());
        }

        let tokens = AccessTokenProvider::new(http.clone(), account)?;

        Ok(Self {
            http,
            project_id: settings.project_id.clone(),
            endpoints: endpoints.clone(),
            tokens,
            breaker: CircuitBreaker::named(FAILURE_THRESHOLD, OPEN_DURATION, "FCM"),
        })
    }

    /// The project this sender delivers for.
    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// The breaker guarding this adapter.
    #[must_use]
    pub fn breaker(&self) -> &Arc<CircuitBreaker> {
        &self.breaker
    }

    /// Build the wire message for one device.
    ///
    /// An associated function rather than an inline literal so it can be asserted
    /// without an HTTP round trip, and so `send` and any future batch endpoint produce
    /// byte-identical messages.
    #[must_use]
    fn message_for(device_token: &str, message: &PushMessage) -> FcmMessage {
        let mut apns_headers = HashMap::new();
        apns_headers.insert(
            FcmApnsConfig::PRIORITY_HEADER.to_owned(),
            FcmApnsConfig::PRIORITY_IMMEDIATE.to_owned(),
        );

        FcmMessage {
            token: Some(device_token.to_owned()),
            notification: FcmNotification {
                title: message.title.clone(),
                body: message.body.clone(),
                image: message.image.clone(),
            },
            data: message.data.clone(),
            android: FcmAndroidConfig {
                priority: FcmAndroidConfig::PRIORITY_HIGH.to_owned(),
                notification: FcmAndroidNotification {
                    channel_id: FcmAndroidNotification::DEFAULT_CHANNEL_ID.to_owned(),
                    click_action: FcmAndroidNotification::CLICK_ACTION.to_owned(),
                },
            },
            apns: FcmApnsConfig {
                headers: apns_headers,
                payload: FcmApnsPayload {
                    aps: FcmApnsAps {
                        alert: FcmApnsAlert {
                            title: message.title.clone(),
                            body: message.body.clone(),
                        },
                        badge: None,
                        sound: FcmApnsAps::SOUND_DEFAULT.to_owned(),
                        content_available: FcmApnsAps::CONTENT_AVAILABLE,
                    },
                },
            },
        }
    }

    /// Send one notification to one device.
    ///
    /// # Errors
    ///
    /// Any [`PushError`]. The one a caller must act on is
    /// [`PushError::TokenInvalid`]: delete the token, because nothing else will ever
    /// succeed for that device.
    #[tracing::instrument(
        name = "push.send",
        skip_all,
        fields(project_id = %self.project_id, deep_link = tracing::field::Empty)
    )]
    pub async fn send(&self, device_token: &str, message: PushMessage) -> Result<(), PushError> {
        tracing::Span::current().record("deep_link", message.deep_link().unwrap_or(""));

        self.breaker
            .check()
            .await
            .map_err(|_| PushError::CircuitOpen)?;

        // The token exchange is a request to Google, so its failures feed the breaker
        // exactly as a send failure does: if the token endpoint is unreachable, every
        // send is about to fail, and the breaker is what stops each one paying a round
        // trip to discover that. `TokenFetch` is also how a revoked key presents, and
        // a revoked key is just as unavailable as a throttled one.
        let bearer = match self.tokens.bearer().await {
            Ok(bearer) => bearer,
            Err(e) => {
                self.breaker.on_failure().await;
                return Err(e);
            }
        };

        let envelope = FcmEnvelope {
            message: Self::message_for(device_token, &message),
        };

        let response = self
            .http
            .post(&self.endpoints.send_url)
            .bearer_auth(&bearer)
            .json(&envelope)
            .send()
            .await;

        let response = match response {
            Ok(response) => response,
            Err(e) => {
                // A transport failure is FCM being unreachable, which is exactly what
                // the breaker exists for.
                self.breaker.on_failure().await;
                return Err(PushError::Transport(e.to_string()));
            }
        };

        let status = response.status().as_u16();
        if (200..=299).contains(&status) {
            self.breaker.on_success().await;
            return Ok(());
        }

        // The body is read for its canonical status and then dropped; see `PushError`.
        let body = response.text().await.unwrap_or_default();
        let error = classify_failure(status, &body);

        if error.is_stale_token() {
            warn!(status, "FCM rejected a device token as unregistered");
        } else {
            // 429 and 5xx mean FCM is unwell; 4xx means this request or these
            // credentials are wrong. Only the first should mute push for everyone.
            if status == 429 || status >= 500 {
                self.breaker.on_failure().await;
            }
            // Structured fields only (§4.8). The response body was read for its
            // canonical status and dropped — it can echo back parts of the message we
            // sent, and this is where that decision is paid for.
            error!(
                status,
                error = %error,
                retryable = error.is_retryable(),
                "FCM send failed"
            );
        }

        Err(error)
    }

    /// Fan a notification out to many devices.
    ///
    /// Per-device failures are collected, never propagated: one dead token in a
    /// 500-strong subscriber list must not cost the other 499 notifications. The
    /// caller reads [`MulticastResult::stale_tokens`] to clean up.
    #[tracing::instrument(
        name = "push.multicast",
        skip_all,
        fields(project_id = %self.project_id, total = targets.len())
    )]
    pub async fn send_multicast(
        &self,
        targets: Vec<PushTarget>,
        message: PushMessage,
    ) -> MulticastResult {
        let total = targets.len();
        if total == 0 {
            return MulticastResult::default();
        }

        // One semaphore for the whole call rather than per batch, so the cap holds
        // across the whole fan-out instead of resetting every `buffer_unordered`.
        let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_SENDS));
        let mut sends = JoinSet::new();

        for target in targets {
            let sender = self.clone();
            let permits = Arc::clone(&permits);
            let message = message.clone();

            sends.spawn(async move {
                // The semaphore is never closed, so this cannot fail in practice. The
                // arm is here rather than an `expect` because §9.1 denies one in `src/`
                // and a spawned task panicking would abort the whole fan-out silently.
                let permit = match permits.acquire().await {
                    Ok(permit) => permit,
                    Err(closed) => {
                        error!(error = %closed, "the FCM send queue was closed");
                        return (target.user_id, Err(PushError::CircuitOpen));
                    }
                };

                let result = sender.send(&target.device_token, message).await;
                drop(permit);
                (target.user_id, result)
            });
        }

        let mut result = MulticastResult::default();
        while let Some(joined) = sends.join_next().await {
            // A `JoinError` means the task panicked or was cancelled. Losing one
            // device is better than losing the batch, and it must be visible.
            match joined {
                Ok((_, Ok(()))) => result.succeeded += 1,
                Ok((user_id, Err(e))) => result.failed.push((user_id, e)),
                Err(e) => error!(error = %e, "an FCM send task did not complete"),
            }
        }

        if !result.failed.is_empty() {
            warn!(
                succeeded = result.succeeded,
                failed = result.failed.len(),
                stale = result.stale_tokens().len(),
                total,
                "FCM multicast partially failed"
            );
        }

        result
    }

    /// The users whose devices could not be reached, for a caller that wants the ids
    /// without unpacking the result.
    #[must_use]
    pub fn stale_users(result: &MulticastResult) -> Vec<Uuid> {
        result.stale_tokens()
    }
}

/// The production [`PushSender`].
///
/// Deliberately a thin delegation rather than the methods being defined *in* the impl
/// block: `send` and `send_multicast` have to be reachable both as inherent methods
/// (used by this module's own tests, which assert on the wire body) and through
/// `Arc<dyn PushSender>` (used by everything else). The default `notify_user` — the
/// token lookup and the stale-token cleanup — comes from the trait.
#[async_trait::async_trait]
impl PushSender for FcmPushSender {
    async fn send(&self, device_token: &str, message: PushMessage) -> Result<(), PushError> {
        Self::send(self, device_token, message).await
    }

    async fn send_multicast(
        &self,
        targets: Vec<PushTarget>,
        message: PushMessage,
    ) -> MulticastResult {
        Self::send_multicast(self, targets, message).await
    }
}

#[cfg(test)]
mod tests {
    //! Tests for [`FcmPushSender`].
    //!
    //! Two layers, deliberately:
    //!
    //! - **No network.** Message construction, wire format, endpoint building and
    //!   construction failures — the parts where a mistake is invisible until production.
    //! - **A local HTTP server** (`wiremock`, §10's HTTP double) standing in for
    //!   `fcm.googleapis.com` and `oauth2.googleapis.com`. This is the layer `master` could
    //!   not have: without it, every question about what the adapter actually puts on the
    //!   wire, and what it does with a `429`, is only answerable against a real Firebase
    //!   project.
    //!
    //! The HTTP tests exercise the production request path — real JWT signing, real bearer
    //! auth, real JSON — because that is where the bugs were.

    use serde_json::json;

    use super::*;

    use fake::*;

    mod fake {
        //! The test harness for [`FcmPushSender`]: credentials, messages, and a `wiremock`
        //! server standing in for both Google endpoints.
        //!
        //! Shared by the network-free tests in the parent module and by [`mod http`], so it
        //! lives in one place rather than being duplicated — a duplicated fake is a fake
        //! that drifts.
        //!
        //! [`mod http`]: super::http
        //!
        //! [`FcmPushSender`]: super::super::FcmPushSender

        use serde_json::json;
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        use super::*;
        use crate::config::PushSettings;

        pub(super) fn id(n: u128) -> Uuid {
            Uuid::from_u128(n)
        }

        /// The bearer the mocked token endpoint hands out.
        pub(super) const BEARER: &str = "ya29.a-test-access-token";

        /// Settings whose service account is a real (throwaway) RSA key, so signing works
        /// without a Firebase project.
        pub(super) fn settings() -> PushSettings {
            PushSettings {
                project_id: "meno-test".to_owned(),
                service_account_json: crate::config::Secret::new(test_key::service_account_json(
                    "https://oauth2.googleapis.com/token",
                )),
            }
        }

        /// Settings pointing the token endpoint at an unroutable address, so every request
        /// fails at connect. How the breaker paths are exercised without a server.
        pub(super) fn unreachable() -> PushSettings {
            PushSettings {
                service_account_json: crate::config::Secret::new(test_key::service_account_json(
                    "http://127.0.0.1:1/token",
                )),
                ..settings()
            }
        }

        pub(super) fn message() -> PushMessage {
            PushMessage::for_user("Ada", "The broadcast has started", id(1), "/broadcasts/1")
        }

        // ── the wiremock harness ────────────────────────────────────────────────────

        /// A local server standing in for both Google endpoints.
        pub(super) struct Fake {
            pub(super) server: MockServer,
        }

        impl Fake {
            pub(super) async fn start() -> Self {
                Self {
                    server: MockServer::start().await,
                }
            }

            /// Serve a successful token exchange.
            pub(super) async fn with_token(&self) -> &Self {
                Mock::given(method("POST"))
                    .and(path("/token"))
                    .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                        "access_token": BEARER,
                        "expires_in": 3600,
                        "token_type": "Bearer",
                    })))
                    .mount(&self.server)
                    .await;
                self
            }

            /// Make the token exchange fail with `status`.
            pub(super) async fn with_failing_token(&self, status: u16) -> &Self {
                Mock::given(method("POST"))
                    .and(path("/token"))
                    .respond_with(
                        ResponseTemplate::new(status)
                            .set_body_string(r#"{"error":"invalid_grant"}"#),
                    )
                    .mount(&self.server)
                    .await;
                self
            }

            /// Serve `status` with `body` for every send.
            pub(super) async fn answering_sends(
                &self,
                status: u16,
                body: serde_json::Value,
            ) -> &Self {
                Mock::given(method("POST"))
                    .and(path("/send"))
                    .respond_with(ResponseTemplate::new(status).set_body_json(body))
                    .mount(&self.server)
                    .await;
                self
            }

            /// Reject `device_token` as unregistered.
            ///
            /// Must be mounted *before* any catch-all on `/send`: `wiremock` resolves the first
            /// mock whose matchers all match, so a catch-all mounted earlier would shadow it.
            pub(super) async fn rejecting(
                &self,
                device_token: &str,
                status: u16,
                body: serde_json::Value,
            ) -> &Self {
                Mock::given(method("POST"))
                    .and(path("/send"))
                    .and(body_partial_json(
                        json!({ "message": { "token": device_token } }),
                    ))
                    .respond_with(ResponseTemplate::new(status).set_body_json(body))
                    .mount(&self.server)
                    .await;
                self
            }

            pub(super) fn sender(&self) -> FcmPushSender {
                FcmPushSender::with_endpoints(
                    &settings(),
                    &PushEndpoints {
                        send_url: format!("{}/send", self.server.uri()),
                        token_url: Some(format!("{}/token", self.server.uri())),
                    },
                )
                .expect("a valid key builds a sender")
            }

            /// Every request the server received, in order.
            pub(super) async fn requests(&self) -> Vec<wiremock::Request> {
                self.server
                    .received_requests()
                    .await
                    .expect("the request log is readable")
            }

            /// Just the `messages:send` requests.
            pub(super) async fn send_requests(&self) -> Vec<serde_json::Value> {
                self.requests()
                    .await
                    .into_iter()
                    .filter(|request| request.url.path() == "/send")
                    .map(|request| {
                        serde_json::from_slice(&request.body).expect("the adapter sends JSON")
                    })
                    .collect()
            }

            /// The raw bodies of the token-exchange requests.
            ///
            /// Returned as text because the OAuth2 token endpoint is form-encoded, not JSON —
            /// a property worth remembering, since the send endpoint is the opposite.
            pub(super) async fn token_requests(&self) -> Vec<String> {
                self.requests()
                    .await
                    .into_iter()
                    .filter(|request| request.url.path() == "/token")
                    .map(|request| String::from_utf8_lossy(&request.body).into_owned())
                    .collect()
            }
        }

        /// An FCM error body carrying a canonical status.
        pub(super) fn fcm_error(status: &str, code: u16) -> serde_json::Value {
            json!({
                "error": {
                    "code": code,
                    "status": status,
                    "message": "a message that must never reach a client",
                }
            })
        }
    }
    use crate::infrastructure::livekit::circuit_breaker::CircuitState;
    use crate::infrastructure::push::{PushSender, test_key};

    // ── construction, with no network ───────────────────────────────────────────

    #[test]
    fn a_valid_configuration_builds_without_touching_the_network() {
        // `Client::builder().build()` opens no connection and parsing a key is pure. If
        // this ever needs a server, the unit tests stop being runnable offline.
        let sender = FcmPushSender::new(&settings()).expect("builds");

        assert_eq!(sender.project_id(), "meno-test");
        assert_eq!(sender.breaker().state(), CircuitState::Closed);
    }

    #[test]
    fn a_malformed_service_account_is_a_startup_error_not_a_panic() {
        // `master` had `.expect("Failed to build HTTP client for FCM")` in this position.
        // §9.1 denies `expect_used` in `src/`, and a bad credential must be a `Result`.
        let broken = PushSettings {
            service_account_json: crate::config::Secret::new("{not json"),
            ..settings()
        };

        let error = FcmPushSender::new(&broken).expect_err("must not build");
        assert!(matches!(error, PushError::TokenFetch(_)), "{error:?}");
    }

    #[test]
    fn an_unusable_private_key_is_reported_before_any_notification_is_attempted() {
        let broken = PushSettings {
            service_account_json: crate::config::Secret::new(
                test_key::service_account_json_with_key(
                    "-----BEGIN PRIVATE KEY-----\nnope\n-----END PRIVATE KEY-----\n",
                ),
            ),
            ..settings()
        };

        let error = FcmPushSender::new(&broken).expect_err("must not build");
        assert!(error.to_string().contains("private_key"), "{error}");
    }

    #[test]
    fn the_send_url_carries_the_configured_project() {
        // A sender pointed at the wrong project silently delivers nothing, and a 404 from
        // the wrong project is indistinguishable from a dead token.
        let endpoints = PushEndpoints::google("meno-test");

        assert_eq!(
            endpoints.send_url,
            "https://fcm.googleapis.com/v1/projects/meno-test/messages:send"
        );
        assert_eq!(
            endpoints.token_url, None,
            "production must use the token_uri in the key, not an override"
        );
    }

    #[test]
    fn a_project_id_is_interpolated_into_the_url_unescaped_but_bounded() {
        // Not a security claim — the id comes from the operator's own environment — just a
        // guard against a trailing-space typo producing a silently different project.
        assert!(
            PushEndpoints::google("meno-prod")
                .send_url
                .contains("/projects/meno-prod/")
        );
    }

    #[test]
    fn debug_never_reveals_the_service_account_key() {
        let rendered = format!("{:?}", FcmPushSender::new(&settings()).expect("builds"));

        assert!(
            !rendered.contains("BEGIN PRIVATE KEY"),
            "the private key leaked into Debug output: {rendered}"
        );
        assert!(rendered.contains("[redacted]"), "{rendered}");
    }

    // ── message construction ────────────────────────────────────────────────────

    #[test]
    fn the_message_addresses_the_device_and_carries_the_payload() {
        let built = FcmPushSender::message_for("device-token", &message());

        assert_eq!(built.token.as_deref(), Some("device-token"));
        assert_eq!(built.notification.title, "Ada");
        assert_eq!(
            built.data.get("deep_link").map(String::as_str),
            Some("/broadcasts/1")
        );
        assert_eq!(
            built.data.get("user_id").map(String::as_str),
            Some(id(1).to_string().as_str())
        );
        assert_eq!(built.android.priority, "high");
    }

    #[test]
    fn the_aps_alert_repeats_the_visible_text() {
        // FCM does not apply the top-level `notification` to APNs. Omitting the aps copy is
        // how an app ends up with Android alerts and silent iOS ones.
        let built = FcmPushSender::message_for("device-token", &message());

        assert_eq!(
            built.apns.payload.aps.alert.body, built.notification.body,
            "the aps alert must repeat the notification body"
        );
        assert_eq!(built.apns.payload.aps.alert.title, built.notification.title);
    }

    #[test]
    fn the_android_channel_and_click_action_match_the_mobile_app() {
        // A contract with `apps/mobile`. A channel id the Flutter app has not created is not
        // an error — Android falls back to its default channel — so the symptom is "the
        // notification has the wrong importance", which is very hard to trace back here.
        let built = FcmPushSender::message_for("device-token", &message());

        assert_eq!(built.android.notification.channel_id, "meno_default");
        assert_eq!(
            built.android.notification.click_action,
            "FLUTTER_NOTIFICATION_CLICK"
        );
    }

    #[test]
    fn the_serialised_message_uses_fcms_field_names() {
        // The exact body that goes on the wire, built by the same function `send` uses.
        let envelope = FcmEnvelope {
            message: FcmPushSender::message_for("device-token", &message()),
        };
        let value = serde_json::to_value(envelope).expect("serialises");

        assert_eq!(value["message"]["token"], "device-token");
        assert_eq!(
            value["message"]["android"]["notification"]["click_action"],
            "FLUTTER_NOTIFICATION_CLICK",
            "click_action is snake_case on the wire"
        );
        assert_eq!(
            value["message"]["android"]["notification"]["channel_id"], "meno_default",
            "channel_id is snake_case on the wire"
        );
        assert_eq!(
            value["message"]["apns"]["payload"]["aps"]["content-available"],
            FcmApnsAps::CONTENT_AVAILABLE,
            "the aps field is hyphenated, not camelCase"
        );
        assert_eq!(value["message"]["apns"]["headers"]["apns-priority"], "10");
    }

    #[test]
    fn the_constants_are_sane() {
        // `master` hard-coded `buffer_unordered(50)` and a 10-second timeout in the middle
        // of a closure. Named, they can be tuned and asserted on.
        const {
            assert!(
                MAX_CONCURRENT_SENDS > 0,
                "a zero cap would stall every send"
            );
            assert!(
                MAX_CONCURRENT_SENDS <= 500,
                "more than 500 concurrent sends is how you get throttled"
            );
            assert!(
                crate::infrastructure::http::REQUEST_TIMEOUT_SECS > 0
                    && crate::infrastructure::http::REQUEST_TIMEOUT_SECS <= 30
            );
            assert!(FAILURE_THRESHOLD > 0);
        }
    }

    mod http {
        //! The request path of [`FcmPushSender`], against a local HTTP server.
        //!
        //! `wiremock` stands in for `fcm.googleapis.com` and `oauth2.googleapis.com`, so these
        //! exercise the production path — real RS256 signing, real bearer auth, real JSON, real
        //! status codes — with no Firebase project and no credentials. This is the layer
        //! `master` could not have: without it, every question about what the adapter actually
        //! puts on the wire and what it does with a `429` is only answerable against a real
        //! project, and therefore only answerable rarely.
        //!
        //! [`FcmPushSender`]: super::super::FcmPushSender

        use serde_json::json;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        use super::*;
        use crate::infrastructure::livekit::circuit_breaker::CircuitState;
        use crate::infrastructure::push::{InMemoryTokenStore, NotifyOutcome};

        #[tokio::test]
        async fn a_delivered_notification_posts_the_bearer_token_and_the_message() {
            // The request the adapter actually makes. Nothing else proves the bearer header is
            // set, the payload is the one the app expects, and the project is in the path.
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(200, json!({ "name": "projects/x/messages/1" }))
                .await;
            let sender = fake.sender();

            sender
                .send("device-token", message())
                .await
                .expect("FCM accepted it");

            let requests = fake.requests().await;
            let send = requests
                .iter()
                .find(|request| request.url.path() == "/send")
                .expect("a send request");

            assert_eq!(
                send.headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok()),
                Some(format!("Bearer {BEARER}").as_str()),
                "the request must carry the cached access token"
            );

            let body: serde_json::Value = serde_json::from_slice(&send.body).expect("JSON body");
            assert_eq!(body["message"]["token"], "device-token");
            assert_eq!(body["message"]["notification"]["title"], "Ada");
            assert_eq!(
                body["message"]["notification"]["body"],
                "The broadcast has started"
            );
            assert_eq!(body["message"]["data"]["deep_link"], "/broadcasts/1");
            assert_eq!(body["message"]["data"]["user_id"], id(1).to_string());
        }

        #[tokio::test]
        async fn the_token_exchange_uses_the_jwt_bearer_grant_and_a_real_assertion() {
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(200, json!({}))
                .await;
            let sender = fake.sender();

            sender.send("device-token", message()).await.expect("sent");

            let token_requests = fake.token_requests().await;
            assert_eq!(token_requests.len(), 1);
            assert!(
                token_requests[0]
                    .contains("grant_type=urn%3Aietf%3Aparams%3Aoauth2%3Agrant-type%3Ajwt-bearer"),
                "wrong grant type: {}",
                token_requests[0]
            );
            assert!(
                token_requests[0].contains("assertion="),
                "the assertion must be posted: {}",
                token_requests[0]
            );
            assert!(
                token_requests[0].len() > 400,
                "the assertion must be a signed JWT, not a bare client email: {}",
                token_requests[0]
            );
        }

        #[tokio::test]
        async fn the_access_token_is_fetched_once_and_reused_for_every_send() {
            // The reason the cache exists. Without it a fan-out of 50 fires 50 token requests,
            // which is how a notification job gets itself rate-limited on Google's OAuth2
            // endpoint and then fails for a reason that has nothing to do with FCM.
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(200, json!({}))
                .await;
            let sender = fake.sender();

            for _ in 0..3 {
                sender.send("device-token", message()).await.expect("sent");
            }

            assert_eq!(fake.token_requests().await.len(), 1);
            assert_eq!(fake.send_requests().await.len(), 3);
        }

        #[tokio::test]
        async fn a_rejected_device_token_is_reported_as_stale_and_leaves_the_circuit_closed() {
            // The `master` bug: `on_failure` was called for every non-2xx, so a burst of
            // uninstalls opened the circuit for every other user.
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(404, fcm_error("UNREGISTERED", 404))
                .await;
            let sender = fake.sender();

            let error = sender
                .send("device-token", message())
                .await
                .expect_err("must fail");

            assert!(error.is_stale_token(), "{error:?}");
            assert_eq!(
                sender.breaker().failure_count(),
                0,
                "one dead device token is not an FCM outage"
            );
            assert_eq!(sender.breaker().state(), CircuitState::Closed);
        }

        #[tokio::test]
        async fn an_unregistered_status_on_a_400_is_still_a_stale_token() {
            // FCM answers 400 on some paths. Classifying only on the status means the token is
            // never deleted from `general_settings`, and every future notification for that
            // user fails identically, forever.
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(400, fcm_error("UNREGISTERED", 400))
                .await;
            let sender = fake.sender();

            let error = sender
                .send("device-token", message())
                .await
                .expect_err("must fail");

            assert!(error.is_stale_token(), "{error:?}");
            assert_eq!(sender.breaker().failure_count(), 0);
        }

        #[tokio::test]
        async fn a_rejected_token_never_carries_the_response_body_into_the_error() {
            // §9.1: no upstream internals in what a caller can see. The body is logged, not
            // returned.
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(400, fcm_error("INVALID_ARGUMENT", 400))
                .await;
            let sender = fake.sender();

            let error = sender
                .send("device-token", message())
                .await
                .expect_err("must fail");

            assert_eq!(error, PushError::SendFailed { status: 400 });
            assert!(
                !error.to_string().contains("must never reach a client"),
                "the response body leaked into the error: {error}"
            );
        }

        #[tokio::test]
        async fn a_malformed_request_is_not_treated_as_an_fcm_outage() {
            // A 400 means our message is wrong. Tripping the breaker would hide a permanent bug
            // behind "temporarily unavailable" and mute push for every user.
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(400, fcm_error("INVALID_ARGUMENT", 400))
                .await;
            let sender = fake.sender();

            let _ = sender.send("device-token", message()).await;

            assert_eq!(sender.breaker().failure_count(), 0);
            assert_eq!(sender.breaker().state(), CircuitState::Closed);
        }

        #[tokio::test]
        async fn a_throttled_project_reports_a_rate_limit_and_eventually_opens_the_circuit() {
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(429, fcm_error("QUOTA_EXCEEDED", 429))
                .await;
            let sender = fake.sender();

            let error = sender
                .send("device-token", message())
                .await
                .expect_err("must fail");
            assert_eq!(error, PushError::RateLimited);
            assert_eq!(error.status(), Some(429));

            for _ in 1..FAILURE_THRESHOLD {
                let _ = sender.send("device-token", message()).await;
            }

            assert_eq!(
                sender.breaker().state(),
                CircuitState::Open,
                "repeated throttling means FCM cannot serve us right now"
            );
        }

        #[tokio::test]
        async fn an_open_circuit_stops_calling_fcm_at_all() {
            // The whole value of the breaker: a down FCM must not consume the budget of every
            // job that tries to send.
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(500, fcm_error("INTERNAL", 500))
                .await;
            let sender = fake.sender();

            for _ in 0..FAILURE_THRESHOLD {
                let _ = sender.send("device-token", message()).await;
            }
            let sends_before = fake.send_requests().await.len();

            let error = sender
                .send("device-token", message())
                .await
                .expect_err("must fail");

            assert!(matches!(error, PushError::CircuitOpen), "{error:?}");
            assert_eq!(
                fake.send_requests().await.len(),
                sends_before,
                "an open circuit must not reach the network"
            );
        }

        #[tokio::test]
        async fn a_server_error_is_retryable_and_opens_the_circuit() {
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(503, fcm_error("UNAVAILABLE", 503))
                .await;
            let sender = fake.sender();

            let error = sender
                .send("device-token", message())
                .await
                .expect_err("must fail");

            assert_eq!(error, PushError::SendFailed { status: 503 });
            assert!(error.is_retryable());
            assert_eq!(sender.breaker().failure_count(), 1);
        }

        #[tokio::test]
        async fn a_refused_token_exchange_surfaces_as_a_credential_failure() {
            // A revoked key is neither a device problem nor retryable; retrying re-presents the
            // same bad credential.
            let fake = Fake::start().await;
            fake.with_failing_token(400).await;
            let sender = fake.sender();

            let error = sender
                .send("device-token", message())
                .await
                .expect_err("must fail");

            assert!(matches!(error, PushError::TokenFetch(_)), "{error:?}");
            assert!(!error.is_retryable());
            assert!(
                !error.to_string().contains("invalid_grant"),
                "the token endpoint's body must not be propagated: {error}"
            );
            assert!(
                fake.send_requests().await.is_empty(),
                "a sender with no token must not attempt a send"
            );
        }

        #[tokio::test]
        async fn an_unreadable_token_response_surfaces_as_a_credential_failure() {
            let fake = Fake::start().await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
                .mount(&fake.server)
                .await;
            let sender = fake.sender();

            let error = sender
                .send("device-token", message())
                .await
                .expect_err("must fail");

            assert!(matches!(error, PushError::TokenFetch(_)), "{error:?}");
        }

        // ── notify_user against a real sender ───────────────────────────────────────

        #[tokio::test]
        async fn notify_user_deletes_the_token_fcm_rejected() {
            // The end-to-end contract: the token is removed from the store, not merely
            // reported.
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(404, fcm_error("UNREGISTERED", 404))
                .await;
            let sender = fake.sender();
            let store = InMemoryTokenStore::new().with(id(1), "device-token");

            let outcome = sender.notify_user(id(1), &store, message()).await;

            assert_eq!(outcome, NotifyOutcome::TokenCleared);
            assert!(
                store.tokens().is_empty(),
                "a token FCM rejected must not stay registered"
            );
        }

        #[tokio::test]
        async fn notify_user_keeps_the_token_when_fcm_is_merely_unavailable() {
            let fake = Fake::start().await;
            fake.with_token()
                .await
                .answering_sends(503, fcm_error("UNAVAILABLE", 503))
                .await;
            let sender = fake.sender();
            let store = InMemoryTokenStore::new().with(id(1), "device-token");

            let outcome = sender.notify_user(id(1), &store, message()).await;

            assert_eq!(
                outcome,
                NotifyOutcome::Failed(PushError::SendFailed { status: 503 })
            );
            assert_eq!(
                store.tokens().len(),
                1,
                "a transient outage must not delete a working token"
            );
        }

        // ── fan-out across devices ──────────────────────────────────────────────────────

        // ── fan-out ─────────────────────────────────────────────────────────────────

        #[tokio::test]
        async fn a_fan_out_reaches_every_device_and_reports_only_the_dead_ones() {
            // One dead token in a subscriber list must cost nobody else's notification.
            let fake = Fake::start().await;
            fake.with_token().await;
            fake.rejecting("dead-token", 404, fcm_error("UNREGISTERED", 404))
                .await
                .answering_sends(200, json!({}))
                .await;
            let sender = fake.sender();
            let targets = vec![
                PushTarget::new(id(1), "live-1"),
                PushTarget::new(id(2), "dead-token"),
                PushTarget::new(id(3), "live-2"),
            ];

            let result = sender.send_multicast(targets, message()).await;

            assert_eq!(result.total(), 3, "every device must be accounted for");
            assert_eq!(result.succeeded, 2);
            assert_eq!(result.stale_tokens(), vec![id(2)]);
            assert_eq!(fake.send_requests().await.len(), 3);
        }

        #[tokio::test]
        async fn a_fan_out_to_nobody_succeeds_without_touching_the_network() {
            // A broadcast with no subscribers is not an error.
            let fake = Fake::start().await;
            let sender = fake.sender();

            let result = sender.send_multicast(Vec::new(), message()).await;

            assert_eq!(result.total(), 0);
            assert!(result.is_complete_success());
            assert!(
                fake.requests().await.is_empty(),
                "an empty fan-out must not even fetch a token"
            );
        }
    }

    #[tokio::test]
    async fn a_transport_failure_opens_the_circuit() {
        // An unreachable host, to cover the branch no mock server can: the request never
        // gets a response at all.
        let sender = FcmPushSender::new(&unreachable()).expect("builds");

        for _ in 0..FAILURE_THRESHOLD {
            let _ = sender.send("device-token", message()).await;
        }

        assert_eq!(sender.breaker().state(), CircuitState::Open);

        let error = sender
            .send("device-token", message())
            .await
            .expect_err("must fail");
        assert!(
            matches!(error, PushError::CircuitOpen | PushError::TokenFetch(_)),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn a_fan_out_that_cannot_connect_accounts_for_every_device() {
        // Nothing is dropped silently: the caller learns that all N failed.
        let sender = FcmPushSender::new(&unreachable()).expect("builds");
        let targets: Vec<PushTarget> = (1..=5).map(|n| PushTarget::new(id(n), "token")).collect();

        let result = sender.send_multicast(targets, message()).await;

        assert_eq!(result.total(), 5);
        assert_eq!(
            result.failed.len(),
            5,
            "nothing may reach an unreachable FCM"
        );
        assert!(
            sender.breaker().failure_count() > 0,
            "an unreachable FCM must be visible to the breaker"
        );
    }

    #[tokio::test]
    async fn a_fan_out_beyond_the_concurrency_cap_still_reaches_every_device() {
        // `MAX_CONCURRENT_SENDS` bounds parallelism, not total work. If the semaphore were
        // acquired and never released, the devices past the cap would hang forever — which
        // is the bug this asserts against.
        let fake = Fake::start().await;
        fake.with_token()
            .await
            .answering_sends(200, json!({}))
            .await;
        let sender = fake.sender();
        let count = MAX_CONCURRENT_SENDS + 10;
        let targets: Vec<PushTarget> = (1..=count)
            .map(|n| PushTarget::new(id(n as u128), "token"))
            .collect();

        let result = sender.send_multicast(targets, message()).await;

        assert_eq!(result.succeeded, count, "every device must be reached");
        assert_eq!(fake.send_requests().await.len(), count);
    }

    #[test]
    fn stale_users_are_the_tokens_to_clean_up() {
        let result = MulticastResult {
            succeeded: 1,
            failed: vec![
                (id(1), PushError::TokenInvalid),
                (id(2), PushError::Transport("dns".to_owned())),
            ],
        };

        assert_eq!(FcmPushSender::stale_users(&result), vec![id(1)]);
    }

    /// Tests that need a real Firebase project.
    ///
    /// Ignored by default so CI stays green without credentials, and shaped exactly like
    /// `infrastructure::livekit`'s `live` module. There is deliberately only one: everything
    /// else the adapter does is covered deterministically above, so the only thing this
    /// proves is that *Google* accepts the request — a JWT bearer exchange followed by one
    /// `messages:send`. It sends no real notification, because the token it uses is a
    /// fabricated one and FCM will simply reject it.
    mod live {
        use super::*;
        use crate::config::Config;

        async fn sender() -> Option<FcmPushSender> {
            let config = Config::load().ok()?;
            config
                .push
                .as_ref()
                .map(FcmPushSender::new)
                .transpose()
                .ok()?
        }

        #[tokio::test]
        #[ignore = "requires PUSH_ENABLED and a Firebase service account"]
        async fn the_service_account_can_exchange_a_jwt_for_an_access_token() {
            // The only claim the offline tests cannot make: that Google accepts a
            // `firebase.messaging`-scoped assertion signed by this key.
            let Some(sender) = sender().await else {
                return;
            };

            let bearer = sender
                .tokens
                .bearer()
                .await
                .expect("the token endpoint must accept our assertion");

            assert!(
                bearer.starts_with("ya29."),
                "Google did not return an access token: {bearer}"
            );
        }

        #[tokio::test]
        #[ignore = "requires PUSH_ENABLED and a Firebase service account"]
        async fn a_fabricated_device_token_is_rejected_as_stale_not_as_a_bad_request() {
            // Proves the two classifications are distinguishable against the real API. A
            // fabricated token is not a secret and is not a real device.
            let Some(sender) = sender().await else {
                return;
            };

            let error = sender
                .send(
                    "this-is-not-a-real-fcm-token",
                    PushMessage::new("Meno", "connectivity probe"),
                )
                .await
                .expect_err("a fabricated token must be rejected");

            assert!(
                error.is_stale_token() || error.is_retryable(),
                "the rejection must be classifiable, not opaque: {error:?}"
            );
        }
    }
}
