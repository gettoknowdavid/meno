//! Push notifications over Firebase Cloud Messaging (FCM v1 HTTP API).
//!
//! Ported from `apps/api/src/shared/services/push/` on `master` (`903c3ba`).
//!
//! # Layering
//!
//! This is an **adapter** (§5.6): it owns a Google credential and translates it into
//! Meno's vocabulary. Callers depend on [`PushSender`], never on [`FcmPushSender`], so
//! notification code is testable against [`NoopPushSender`] with no Firebase project and
//! no network — which §10's unit tier needs and `master` could not provide.
//!
//! Module map:
//!
//! - [`model`] — [`PushMessage`], [`PushTarget`], [`MulticastResult`], [`NotifyOutcome`].
//!   Ours, not FCM's.
//! - [`dto`] — the FCM v1 wire format, the service-account key, and the mapping from an
//!   FCM response to a [`PushError`].
//! - [`token`] — the Google `OAuth2` exchange and its cache.
//! - [`fcm`] — the sender itself.
//! - [`error`] — [`PushError`] and [`TokenStoreError`].
//!
//! # Why push is optional (§4.6)
//!
//! On `master` a missing `FIREBASE_SERVICE_ACCOUNT_PATH` **stopped the whole process
//! from booting**, even though only notifications needed it. `Config` now gates push on
//! `PUSH_ENABLED`, and [`sender_from_config`] returns [`NoopPushSender`] when it is off,
//! so every call site is unchanged and every call is a cheap no-op.
//!
//! The no-op is a real implementation rather than a `cfg`-gated stub: a build with push
//! disabled must still compile the notification jobs, which now log `Skipped` rather
//! than failing.
//!
//! # What is deliberately not here
//!
//! There is no `send_to_user_if_enabled` reaching into a repository. `master` took
//! `&Arc<dyn NotificationRepo>` into the adapter and did the token lookup, the payload
//! assembly and the stale-token cleanup inside it — three responsibilities, one of them
//! domain logic, inside an infrastructure adapter. Here the adapter is told *which* user
//! through the narrow [`PushTokenStore`] (§5.4) and still owns only "get a message onto
//! a device, and report what happened".

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use uuid::Uuid;

use crate::config::Config;
use crate::infrastructure::push::error::{PushError, TokenStoreError};
use crate::infrastructure::push::model::DATA_USER_ID;

pub mod dto;
pub mod error;
mod fcm;
pub mod model;
pub mod token;

#[cfg(test)]
mod test_key;

pub use error::{PushError as FcmError, SERVICE_NAME as FCM_SERVICE_NAME};
pub use fcm::{FcmPushSender, MAX_CONCURRENT_SENDS, PushEndpoints};
pub use model::{
    DATA_DEEP_LINK, DATA_USER_ID as DATA_RECIPIENT, MulticastResult, NotifyOutcome, PushMessage,
    PushTarget,
};
pub use token::AccessTokenProvider;

/// Anything that can deliver a push notification to a device.
///
/// The seam §5.6 asks for: one real implementation ([`FcmPushSender`]) and one inert
/// one ([`NoopPushSender`]). Object-safe, so application state holds
/// `Arc<dyn PushSender>` and a deployment can swap implementations without a call site
/// changing.
#[async_trait]
pub trait PushSender: Send + Sync {
    /// Deliver one notification to one device token.
    ///
    /// # Errors
    ///
    /// Any [`PushError`]. [`PushError::TokenInvalid`] is the one that demands action:
    /// the token must be deleted, because no retry will ever succeed for that device.
    async fn send(&self, device_token: &str, message: PushMessage) -> Result<(), PushError>;

    /// Deliver one notification to many devices, collecting per-device failures.
    ///
    /// Never returns an error: one dead token in a large subscriber list must not cost
    /// the rest of the notifications.
    async fn send_multicast(
        &self,
        targets: Vec<PushTarget>,
        message: PushMessage,
    ) -> MulticastResult;

    /// Whether this sender actually delivers anything.
    ///
    /// [`NoopPushSender`] returns `false`, which is what lets [`Self::notify_user`]
    /// skip the device-token lookup as well as the send — a disabled integration must
    /// not cost a database query per notification.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Deliver a notification to whatever device `user_id` last registered, and delete
    /// the token if FCM says it is dead.
    ///
    /// The default body is the whole flow `master` had inline in
    /// `PushNotificationService::send_to_user_if_enabled`, minus the repository
    /// dependency. It is a *default* method rather than an inherent method on
    /// [`FcmPushSender`] so every adapter inherits the same token-cleanup behaviour
    /// instead of each re-implementing the stale-token branch.
    async fn notify_user(
        &self,
        user_id: Uuid,
        store: &dyn PushTokenStore,
        mut message: PushMessage,
    ) -> NotifyOutcome {
        if !self.is_enabled() {
            return NotifyOutcome::Skipped;
        }

        let token = match store.device_token(user_id).await {
            Ok(Some(token)) => token,
            // No registered device, or the user has push notifications switched off. The
            // store collapses those two on purpose: the push layer has no opinion on
            // which, and pretending to would leak a settings decision into an adapter.
            Ok(None) => return NotifyOutcome::Skipped,
            Err(e) => {
                tracing::warn!(user_id = %user_id, error = %e, "device-token lookup failed");
                return NotifyOutcome::LookupFailed(e);
            }
        };

        // Set here as well as in `PushMessage::for_user`, so a message assembled by hand
        // still identifies its recipient.
        message
            .data
            .insert(DATA_USER_ID.to_owned(), user_id.to_string());

        match self.send(&token, message).await {
            Ok(()) => NotifyOutcome::Delivered,
            Err(error) if error.is_stale_token() => {
                // Awaited rather than detached into `tokio::spawn`, as `master` did:
                // the caller wants to know the token is gone, and fire-and-forget makes
                // that unobservable in a test. Push is off the request path (§4.9), so
                // this is on nobody's latency budget.
                if let Err(e) = store.clear_device_token(user_id).await {
                    tracing::error!(
                        user_id = %user_id,
                        error = %e,
                        "a rejected FCM token could not be cleared; it will be rejected again"
                    );
                }
                NotifyOutcome::TokenCleared
            }
            Err(error) => {
                tracing::warn!(
                    user_id = %user_id,
                    error = %error,
                    retryable = error.is_retryable(),
                    "push notification failed (non-fatal)"
                );
                NotifyOutcome::Failed(error)
            }
        }
    }
}

/// Where device tokens come from, and where they are deleted.
///
/// Deliberately narrower than the notifications repository: the push layer needs two
/// operations, and depending on the whole repository to get them is exactly what §5.4's
/// interface segregation warns about. `modules::notifications` implements this over its
/// `general_settings` queries; tests and local runs use [`InMemoryTokenStore`].
#[async_trait]
pub trait PushTokenStore: Send + Sync {
    /// The device token registered for `user_id`.
    ///
    /// `Ok(None)` covers both "no device registered" and "push is off for this user" —
    /// the two are the same thing as far as delivery is concerned.
    ///
    /// # Errors
    ///
    /// [`TokenStoreError`] if the lookup itself failed. Deliberately distinct from
    /// `Ok(None)` so a database outage is not silently reported as "this user has
    /// notifications switched off".
    async fn device_token(&self, user_id: Uuid) -> Result<Option<String>, TokenStoreError>;

    /// Delete `user_id`'s device token.
    ///
    /// Called only after FCM has rejected the token, so an implementation may be
    /// pessimistic: it is never called speculatively.
    ///
    /// # Errors
    ///
    /// [`TokenStoreError`] if the token could not be removed.
    async fn clear_device_token(&self, user_id: Uuid) -> Result<(), TokenStoreError>;
}

/// The sender used when `PUSH_ENABLED` is false.
///
/// §4.6's requirement, and the reason this is a *type* rather than an `if enabled { … }`
/// at six call sites: with the no-op wired in at construction, every caller keeps
/// calling [`PushSender`] and the disabled case cannot be forgotten.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopPushSender;

#[async_trait]
impl PushSender for NoopPushSender {
    async fn send(&self, _device_token: &str, _message: PushMessage) -> Result<(), PushError> {
        Ok(())
    }

    async fn send_multicast(
        &self,
        _targets: Vec<PushTarget>,
        _message: PushMessage,
    ) -> MulticastResult {
        MulticastResult::default()
    }

    fn is_enabled(&self) -> bool {
        false
    }
}

/// A [`PushTokenStore`] held in memory.
///
/// §5.6's "in-memory test double", and the store a local worker run uses when there is
/// no database. Public rather than test-only so integration tests under `apps/api/tests`
/// can use it too.
#[derive(Clone, Debug, Default)]
pub struct InMemoryTokenStore {
    tokens: Arc<Mutex<HashMap<Uuid, String>>>,
}

impl InMemoryTokenStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A store holding one `user_id → token` pair.
    #[must_use]
    pub fn with(self, user_id: Uuid, token: impl Into<String>) -> Self {
        self.lock().insert(user_id, token.into());
        self
    }

    /// The tokens still registered.
    ///
    /// The assertion every test needs after a stale token has been cleaned up.
    #[must_use]
    pub fn tokens(&self) -> HashMap<Uuid, String> {
        self.lock().clone()
    }

    /// `Mutex` poisoning means some *other* task panicked while holding this lock. The
    /// map is a plain `HashMap` that cannot be left half-updated, so recovering is
    /// strictly better than propagating a panic out of a test double.
    fn lock(&self) -> MutexGuard<'_, HashMap<Uuid, String>> {
        self.tokens.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl PushTokenStore for InMemoryTokenStore {
    async fn device_token(&self, user_id: Uuid) -> Result<Option<String>, TokenStoreError> {
        Ok(self.lock().get(&user_id).cloned())
    }

    async fn clear_device_token(&self, user_id: Uuid) -> Result<(), TokenStoreError> {
        self.lock().remove(&user_id);
        Ok(())
    }
}

/// Build the push sender for a validated configuration.
///
/// The single place that chooses between the real adapter and the no-op, so
/// `PUSH_ENABLED` is honoured in exactly one spot and both binaries — web and worker —
/// agree on it.
///
/// # Errors
///
/// [`PushError`] if push is *enabled* but unusable: a malformed service-account key or
/// an unusable private key. That is a startup failure, not something to discover when a
/// broadcast goes live, so `bootstrap` should propagate it and exit rather than silently
/// degrade to the no-op — dropping every notification is worse than refusing to boot.
pub fn sender_from_config(config: &Config) -> Result<Arc<dyn PushSender>, PushError> {
    match &config.push {
        Some(settings) => Ok(Arc::new(FcmPushSender::new(settings)?)),
        None => {
            tracing::info!("push notifications are disabled (PUSH_ENABLED is not true)");
            Ok(Arc::new(NoopPushSender))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the push module's seams: [`PushSender`], [`PushTokenStore`],
    //! [`NoopPushSender`], [`InMemoryTokenStore`] and [`sender_from_config`].
    //!
    //! These are the tests §10 asks for and `master` could not have had: notification
    //! behaviour with no Firebase project, no network and no database. Each one runs in
    //! microseconds, so they can stay in the ordinary unit tier.
    //!
    //! [`PushSender`]: super::PushSender
    //! [`PushTokenStore`]: super::PushTokenStore
    //! [`NoopPushSender`]: super::NoopPushSender
    //! [`InMemoryTokenStore`]: super::InMemoryTokenStore
    //! [`sender_from_config`]: super::sender_from_config

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::*;
    use crate::config::{Config, MapSource, PushSettings, Secret};

    // ── doubles ─────────────────────────────────────────────────────────────────

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// A [`PushSender`] that records what it was asked to deliver and can be told to fail.
    ///
    /// The "in-memory test double" §5.6 asks for. Recording the *message* is what lets a
    /// test assert the payload, which is the part of a push that actually regresses — the
    /// HTTP status code in a mock response is not.
    #[derive(Clone, Default)]
    struct RecordingSender {
        sent: Arc<Mutex<Vec<(String, PushMessage)>>>,
        outcome: Option<Result<(), PushError>>,
    }

    impl RecordingSender {
        fn ok() -> Self {
            Self::default()
        }

        fn failing(error: PushError) -> Self {
            Self {
                sent: Arc::default(),
                outcome: Some(Err(error)),
            }
        }

        fn sent(&self) -> Vec<(String, PushMessage)> {
            self.sent
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    #[async_trait]
    impl PushSender for RecordingSender {
        async fn send(&self, device_token: &str, message: PushMessage) -> Result<(), PushError> {
            self.sent
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((device_token.to_owned(), message));
            self.outcome.clone().unwrap_or(Ok(()))
        }

        async fn send_multicast(
            &self,
            targets: Vec<PushTarget>,
            message: PushMessage,
        ) -> MulticastResult {
            let mut result = MulticastResult::default();
            for target in targets {
                match self.send(&target.device_token, message.clone()).await {
                    Ok(()) => result.succeeded += 1,
                    Err(e) => result.failed.push((target.user_id, e)),
                }
            }
            result
        }
    }

    /// An [`InMemoryTokenStore`] that counts lookups and can be made to fail.
    #[derive(Clone, Default)]
    struct CountingStore {
        inner: InMemoryTokenStore,
        lookups: Arc<AtomicUsize>,
        fail_lookup: bool,
        fail_clear: bool,
    }

    impl CountingStore {
        fn holding(token: &str) -> Self {
            Self {
                inner: InMemoryTokenStore::new().with(id(1), token),
                ..Self::default()
            }
        }

        fn lookups(&self) -> usize {
            self.lookups.load(Ordering::SeqCst)
        }

        fn broken_lookup() -> Self {
            Self {
                fail_lookup: true,
                ..Self::default()
            }
        }

        fn broken_cleanup() -> Self {
            Self {
                inner: InMemoryTokenStore::new().with(id(1), "device-token"),
                fail_clear: true,
                ..Self::default()
            }
        }
    }

    #[async_trait]
    impl PushTokenStore for CountingStore {
        async fn device_token(&self, user_id: Uuid) -> Result<Option<String>, TokenStoreError> {
            self.lookups.fetch_add(1, Ordering::SeqCst);
            if self.fail_lookup {
                return Err(TokenStoreError::Lookup("pool timed out".to_owned()));
            }
            self.inner.device_token(user_id).await
        }

        async fn clear_device_token(&self, user_id: Uuid) -> Result<(), TokenStoreError> {
            if self.fail_clear {
                return Err(TokenStoreError::Cleanup("deadlock".to_owned()));
            }
            self.inner.clear_device_token(user_id).await
        }
    }

    fn message() -> PushMessage {
        PushMessage::for_user("Ada", "The broadcast has started", id(1), "/broadcasts/1")
    }

    /// A source satisfying every required variable, plus push.
    fn push_source() -> MapSource {
        MapSource::new()
            .with("ENV", "dev")
            .with("DATABASE_URL", "postgres://u:p@localhost:5432/meno")
            .with("REDIS_URL", "redis://localhost:6379")
            .with("JWT_SECRET", "a-real-secret-value")
            .with("JWT_REFRESH_SECRET", "another-real-secret-value")
            .with("CORS_ORIGINS", "https://app.example.com")
            .with("PUSH_ENABLED", "true")
            .with("FIREBASE_PROJECT_ID", "meno-test")
            .with(
                "FIREBASE_SERVICE_ACCOUNT_JSON",
                &super::test_key::service_account_json("https://oauth2.googleapis.com/token"),
            )
    }

    fn push_settings() -> PushSettings {
        PushSettings {
            project_id: "meno-test".to_owned(),
            service_account_json: Secret::new(super::test_key::service_account_json(
                "https://oauth2.googleapis.com/token",
            )),
        }
    }

    // ── the no-op (§4.6) ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_disabled_push_never_fails_and_says_it_is_disabled() {
        // §4.6's actual requirement: with `PUSH_ENABLED=false` the notification jobs must
        // still run, and must not report a failure for a notification nobody was expecting.
        let sender = NoopPushSender;

        assert!(!sender.is_enabled());
        assert!(sender.send("device-token", message()).await.is_ok());

        let result = sender
            .send_multicast(vec![PushTarget::new(id(1), "device-token")], message())
            .await;
        assert!(result.is_complete_success());
    }

    #[tokio::test]
    async fn a_disabled_push_does_not_even_read_the_device_token() {
        // The optimisation `is_enabled` exists for: a disabled integration must not cost a
        // database round trip per notification, which for a 100k-subscriber fan-out is
        // 100k wasted queries.
        let store = CountingStore::holding("device-token");

        let outcome = NoopPushSender.notify_user(id(1), &store, message()).await;

        assert_eq!(outcome, NotifyOutcome::Skipped);
        assert_eq!(store.lookups(), 0, "the store must not be touched");
    }

    #[tokio::test]
    async fn a_disabled_push_leaves_the_registered_token_alone() {
        // It must not "clean up" a token it never sent to — that would delete every user's
        // device token the first time push is turned off.
        let store = CountingStore::holding("device-token");

        let _ = NoopPushSender.notify_user(id(1), &store, message()).await;

        assert_eq!(store.inner.tokens().len(), 1);
    }

    // ── notify_user, the default trait body ─────────────────────────────────────

    #[tokio::test]
    async fn a_registered_user_receives_the_notification() {
        let sender = RecordingSender::ok();
        let store = CountingStore::holding("device-token");

        let outcome = sender.notify_user(id(1), &store, message()).await;

        assert_eq!(outcome, NotifyOutcome::Delivered);
        let sent = sender.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(
            sent[0].0, "device-token",
            "the registered token must be used"
        );
    }

    #[tokio::test]
    async fn the_delivered_message_identifies_its_recipient() {
        // The regression `master` carried: some call sites inserted `deep_link` without
        // `user_id`, so the client could not attribute the push.
        let sender = RecordingSender::ok();
        let store = CountingStore::holding("device-token");

        // A message assembled by hand, without `PushMessage::for_user`.
        let _ = sender
            .notify_user(id(1), &store, PushMessage::new("Ada", "Live now"))
            .await;

        let sent = sender.sent();
        assert_eq!(
            sent[0].1.data.get(DATA_USER_ID).map(String::as_str),
            Some(id(1).to_string().as_str()),
            "notify_user must stamp the recipient even for a hand-built message"
        );
    }

    #[tokio::test]
    async fn a_user_without_a_device_is_skipped_not_failed() {
        // No token means push is off for that user, which is a normal outcome — reporting
        // it as a failure would page someone every time a user clears their data.
        let sender = RecordingSender::ok();
        let store = CountingStore::default();

        let outcome = sender.notify_user(id(1), &store, message()).await;

        assert_eq!(outcome, NotifyOutcome::Skipped);
        assert!(sender.sent().is_empty());
        assert_eq!(store.lookups(), 1);
    }

    #[tokio::test]
    async fn a_broken_lookup_is_not_reported_as_a_quiet_user() {
        // The reason `TokenStoreError` exists rather than `Option`: "the database was
        // briefly unreachable" and "this user has no device" must not look the same, or a
        // database outage shows up as push silently working.
        let sender = RecordingSender::ok();
        let store = CountingStore::broken_lookup();

        let outcome = sender.notify_user(id(1), &store, message()).await;

        assert!(
            matches!(outcome, NotifyOutcome::LookupFailed(_)),
            "a failed lookup must be distinguishable from a user with no device: {outcome:?}"
        );
        assert!(sender.sent().is_empty());
    }

    #[tokio::test]
    async fn a_rejected_token_is_deleted_and_reported() {
        // The whole point of `is_stale_token`: FCM says the token is gone, so the row in
        // `general_settings` must be deleted or every future notification for that user
        // fails the same way forever.
        let sender = RecordingSender::failing(PushError::TokenInvalid);
        let store = CountingStore::holding("device-token");

        let outcome = sender.notify_user(id(1), &store, message()).await;

        assert_eq!(outcome, NotifyOutcome::TokenCleared);
        assert!(
            store.inner.tokens().is_empty(),
            "a token FCM rejected must be deleted"
        );
    }

    #[tokio::test]
    async fn a_transient_failure_leaves_the_token_in_place() {
        // The other half of the same rule, and the one that is easy to get wrong: deleting
        // a token because FCM was briefly unavailable logs every user out of push.
        for transient in [
            PushError::RateLimited,
            PushError::CircuitOpen,
            PushError::SendFailed { status: 503 },
            PushError::Transport("connection reset".to_owned()),
            PushError::TokenFetch("bad key".to_owned()),
        ] {
            let sender = RecordingSender::failing(transient.clone());
            let store = CountingStore::holding("device-token");

            let outcome = sender.notify_user(id(1), &store, message()).await;

            assert_eq!(
                outcome,
                NotifyOutcome::Failed(transient.clone()),
                "{transient:?} must surface as a failure"
            );
            assert_eq!(
                store.inner.tokens().len(),
                1,
                "{transient:?} must not delete a working token"
            );
        }
    }

    #[tokio::test]
    async fn a_cleanup_failure_still_reports_the_token_as_gone() {
        // The send already failed irrecoverably, so the notification is lost either way.
        // Reporting `Failed` here would make a broken `DELETE` look like an FCM problem.
        let sender = RecordingSender::failing(PushError::TokenInvalid);
        let store = CountingStore::broken_cleanup();

        let outcome = sender.notify_user(id(1), &store, message()).await;

        assert_eq!(outcome, NotifyOutcome::TokenCleared);
    }

    // ── the in-memory store ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn the_in_memory_store_round_trips_and_deletes() {
        let store = InMemoryTokenStore::new().with(id(1), "device-token");

        assert_eq!(
            store.device_token(id(1)).await.expect("lookup"),
            Some("device-token".to_owned())
        );
        assert_eq!(store.device_token(id(2)).await.expect("lookup"), None);

        store.clear_device_token(id(1)).await.expect("clear");
        assert_eq!(store.device_token(id(1)).await.expect("lookup"), None);
        assert!(store.tokens().is_empty());
    }

    // ── the factory (§4.6) ──────────────────────────────────────────────────────

    #[test]
    fn push_is_a_no_op_when_the_flag_is_off() {
        // §4.6: the whole point of the change. On `master` a missing Firebase variable
        // stopped the process booting; here it produces a working, inert sender.
        let source = MapSource::new()
            .with("ENV", "dev")
            .with("DATABASE_URL", "postgres://u:p@localhost:5432/meno")
            .with("REDIS_URL", "redis://localhost:6379")
            .with("JWT_SECRET", "a-real-secret-value")
            .with("JWT_REFRESH_SECRET", "another-real-secret-value")
            .with("CORS_ORIGINS", "https://app.example.com");

        let config = Config::from_source(&source).expect("push is optional");
        assert!(config.push.is_none());

        let sender = sender_from_config(&config).expect("the no-op always builds");
        assert!(!sender.is_enabled());
    }

    #[test]
    fn an_enabled_push_builds_the_real_adapter() {
        let config = Config::from_source(&push_source()).expect("valid push config");

        let sender = sender_from_config(&config).expect("builds");

        assert!(sender.is_enabled(), "a configured sender must be enabled");
    }

    #[test]
    fn an_enabled_but_broken_push_fails_startup_rather_than_going_silent() {
        // The other half of §4.6: "optional" must not become "silently broken". Degrading
        // to the no-op here would drop every notification with no signal at all, which is
        // strictly worse than refusing to boot.
        let source = push_source().with("FIREBASE_SERVICE_ACCOUNT_JSON", "{not json");
        let config = Config::from_source(&source).expect("config itself is valid");

        assert!(
            sender_from_config(&config).is_err(),
            "a broken service-account key must be a startup error, not a silent no-op"
        );
    }

    #[test]
    fn the_no_op_and_the_real_adapter_are_both_usable_as_one_trait_object() {
        // The seam itself. Application state holds `Arc<dyn PushSender>`, so a deployment
        // can swap the implementation without a call site changing — and the no-op really
        // is reachable through the same pointer.
        fn enabled(sender: &Arc<dyn PushSender>) -> bool {
            sender.is_enabled()
        }

        let noop: Arc<dyn PushSender> = Arc::new(NoopPushSender);
        let real: Arc<dyn PushSender> =
            Arc::new(FcmPushSender::new(&push_settings()).expect("builds"));

        assert!(!enabled(&noop));
        assert!(enabled(&real));
    }

    #[tokio::test]
    async fn the_no_op_satisfies_send_through_a_trait_object() {
        // The no-op must answer every trait method, not just report itself disabled —
        // a caller that guards on `is_enabled()` and one that does not must both work.
        let sender: Arc<dyn PushSender> = Arc::new(NoopPushSender);

        sender
            .send("device-token", message())
            .await
            .expect("a disabled push never fails a send");

        let result = sender
            .send_multicast(vec![PushTarget::new(id(1), "device-token")], message())
            .await;
        assert!(result.is_complete_success());
    }
}
