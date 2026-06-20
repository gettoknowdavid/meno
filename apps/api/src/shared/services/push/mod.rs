//! Firebase Cloud Messaging (FCM v1 HTTP API) push notification service.
//!
//! ## Why no dedicated FCM crate?
//! The FCM v1 API is a single JSON POST per message. A `reqwest` client +
//! a Google `OAuth2` token request is all that is needed — no wrapper crate
//! required and no additional dependency surface area.
//!
//! ## Token lifecycle
//! Google `OAuth2` access tokens expire after 1 hour. We cache the token in
//! memory and refresh it ~60 seconds before expiry. The service-account
//! JSON is expected either as a file path in `FIREBASE_SERVICE_ACCOUNT_PATH`
//! or inline JSON in `FIREBASE_SERVICE_ACCOUNT_JSON`.
//!
//! ## Rate limiting and circuit breaking
//! The existing `CircuitBreaker` is reused. FCM returns 429 or 500 on
//! overload; both trip the breaker. A 404 (`UNREGISTERED`) means the
//! device token has rotated — the caller should delete it.

mod dto;
pub mod error;

use crate::config::Config;
use crate::modules::notifications::repository::NotificationRepo;
use crate::shared::services::livekit::circuit_breaker::CircuitBreaker;
use crate::shared::services::push::dto::{
    FcmAndroidConfig, FcmAndroidNotification, FcmApnsAlert, FcmApnsAps, FcmApnsConfig,
    FcmApnsPayload, FcmEnvelope, FcmMessage, FcmNotification, GoogleTokenResponse,
};
use crate::shared::services::push::error::PushError;
use futures_util::{StreamExt, stream};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{error, instrument, warn};
use uuid::Uuid;

/// Summary returned by `send_multicast`.
#[derive(Debug, Default)]
pub struct MulticastResult {
    /// (`user_id`, error) pairs for deliveries that failed.
    /// Callers should queue a cleanup job for `TokenInvalid` entries.
    pub failed: Vec<(Uuid, PushError)>,

    pub succeeded: usize,
}

/// Internal Cache Token
struct CachedToken {
    access_token: String,

    /// Wall-clock instant at which this token expires on Google's side.
    expires_at: Instant,
}

#[derive(Clone)]
pub struct PushNotificationService {
    http: Client,
    project_id: String,
    service_account_json: String,
    token_cache: Arc<Mutex<Option<CachedToken>>>,
    breaker: Arc<CircuitBreaker>,
}
impl PushNotificationService {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client for FCM");

        Self {
            http: client,
            project_id: config.firebase_project_id.clone(),
            service_account_json: config.firebase_service_account_json.clone(),
            token_cache: Arc::new(Mutex::new(None)),
            breaker: CircuitBreaker::new(3, 60),
        }
    }

    /// Send a push notification to a single device token.
    ///
    /// Returns `Ok(())` on success.
    /// Returns `Err(PushError::TokenInvalid)` when FCM says the token is
    /// no longer registered — the caller should delete it from `general_settings`.
    #[instrument(name = "push.send", skip_all, fields(project_id = tracing::field::Empty))]
    pub async fn send(
        &self,
        device_token: &str,
        title: &str,
        body: &str,
        image: Option<String>,
        data: HashMap<String, String>,
    ) -> Result<(), PushError> {
        self.breaker
            .check()
            .await
            .map_err(|_| PushError::CircuitOpen)?;

        let access_token = self.get_access_token().await?;
        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.project_id
        );

        let envelope = FcmEnvelope {
            message: Self::build_message(Some(device_token.to_owned()), title, body, image, data),
        };

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&access_token)
            .json(&envelope)
            .send()
            .await?;

        let status = resp.status().as_u16();

        match status {
            200..=299 => {
                self.breaker.on_success().await;
                Ok(())
            }
            404 => {
                // UNREGISTERED — token is stale. Don't trip the breaker.
                warn!("FCM token no longer valid (404) for one device");
                Err(PushError::TokenInvalid)
            }
            429 => {
                self.breaker.on_failure().await;
                Err(PushError::RateLimited)
            }
            _ => {
                self.breaker.on_failure().await;
                let err_body = resp.text().await.unwrap_or_default();
                error!(status, body = %err_body, "FCM send failed");
                Err(PushError::SendFailed(status))
            }
        }
    }

    /// Fan-out push to many devices with bounded concurrency (50 parallel FCM calls).
    ///
    /// Failed sends are collected and returned; they do NOT propagate as errors
    /// so a single bad token doesn't abort the whole batch.
    pub async fn send_multicast(
        &self,
        tokens: Vec<(Uuid, String)>,
        title: &str,
        body: &str,
        image: Option<String>,
        deep_link: &str,
    ) -> MulticastResult {
        let total = tokens.len();
        tracing::debug!(total, "FCM multicast starting");

        // Own the strings once before building the stream so the closures
        // below can clone cheaply without borrowing `title`/`body`/`deep_link`
        // (which would require lifetime annotations on the stream).
        let title_owned = title.to_owned();
        let body_owned = body.to_owned();
        let image_owned = image.clone();
        let deep_link_owned = deep_link.to_owned();

        let result_stream = stream::iter(tokens).map(|(user_id, token)| {
            let svc = self.clone();
            let title = title_owned.clone();
            let body = body_owned.clone();
            let image = image_owned.clone();
            let deep_link = deep_link_owned.clone();

            async move {
                let mut data = HashMap::new();
                data.insert("deep_link".to_string(), deep_link);

                let result = svc.send(&token, &title, &body, image, data).await;
                (user_id, result)
            }
        });

        // Drive up to 50 futures concurrently; collect results as they finish.
        let mut pinned = Box::pin(result_stream.buffer_unordered(50));
        let mut result = MulticastResult::default();

        while let Some((user_id, res)) = pinned.next().await {
            match res {
                Ok(()) => result.succeeded += 1,
                Err(e) => result.failed.push((user_id, e)),
            }
        }

        if !result.failed.is_empty() {
            warn!(
                succeeded = result.succeeded,
                failed = result.failed.len(),
                "FCM multicast partially failed"
            );
        }

        result
    }

    /// Convenience wrapper used by `NotificationService.notify()`.
    ///
    /// Checks `push_notifications` setting and fetches the token from DB.
    /// Fires and forgets — never blocks the caller.
    pub async fn send_to_user_if_enabled(
        &self,
        user_id: Uuid,
        repo: &Arc<dyn NotificationRepo>,
        title: &str,
        body: &str,
        image: Option<String>,
        deep_link: &str,
    ) {
        // Fetch push token (also acts as the push_notifications guard —
        // `get_push_token` only returns tokens when `push_notifications = true`
        // in the batch query, but for single sends we check the column directly).
        let Ok(Some(token)) = repo.get_push_token(user_id).await else {
            return;
        };

        let mut data = HashMap::new();
        data.insert("deep_link".to_string(), deep_link.to_owned());
        data.insert("user_id".to_string(), user_id.to_string());

        match self.send(&token, title, body, image, data).await {
            Ok(()) => {}
            Err(PushError::TokenInvalid) => {
                // Token rotated — clean it up asynchronously.
                let repo = repo.clone();
                tokio::spawn(async move {
                    let _ = repo.clear_push_token(user_id).await;
                });
            }
            Err(e) => {
                warn!(
                    user_id = %user_id,
                    error = %e,
                    "Push notification failed (non-fatal)"
                );
            }
        }
    }

    /// Returns a valid Google `OAuth2` bearer token, refreshing if needed.
    async fn get_access_token(&self) -> Result<String, PushError> {
        let mut cache = self.token_cache.lock().await;

        // Return cached token if it has more than 60 s remaining.
        if let Some(ref cached) = *cache
            && cached.expires_at > Instant::now() + Duration::from_mins(1)
        {
            return Ok(cached.access_token.clone());
        }

        let token = self.fetch_google_token().await?;
        let expires_at = Instant::now() + Duration::from_secs(token.expires_in.saturating_sub(60));
        let access_token = token.access_token.clone();

        *cache = Some(CachedToken {
            access_token: access_token.clone(),
            expires_at,
        });

        Ok(access_token)
    }

    /// Exchange the service-account JSON for a short-lived bearer token.
    ///
    /// Uses a JWT signed with the service account's private key to request
    /// a token from `https://oauth2.googleapis.com/token`.
    ///
    /// Required scope: `https://www.googleapis.com/auth/firebase.messaging`
    async fn fetch_google_token(&self) -> Result<GoogleTokenResponse, PushError> {
        // Parse the service-account JSON.
        let sa: Value = serde_json::from_str(&self.service_account_json)
            .map_err(|e| PushError::TokenFetch(format!("Invalid service-account JSON: {e}")))?;

        let client_email = sa["client_email"]
            .as_str()
            .ok_or_else(|| PushError::TokenFetch("Missing client_email".into()))?
            .to_owned();

        let private_key_pem = sa["private_key"]
            .as_str()
            .ok_or_else(|| PushError::TokenFetch("Missing private_key".into()))?
            .to_owned();

        let token_uri = sa["token_uri"]
            .as_str()
            .unwrap_or("https://oauth2.googleapis.com/token")
            .to_owned();

        // Build a JWT assertion (RS256).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let jwt = build_service_account_jwt(&client_email, &private_key_pem, &token_uri, now)
            .map_err(|e| PushError::TokenFetch(format!("JWT build failed: {e}")))?;

        // POST to Google token endpoint.
        let resp = self
            .http
            .post(&token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth2:grant-type:jwt-bearer"),
                ("assertion", jwt.as_str()),
            ])
            .send()
            .await
            .map_err(|e| PushError::TokenFetch(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(PushError::TokenFetch(format!(
                "Token endpoint returned {}",
                resp.status()
            )));
        }

        resp.json::<GoogleTokenResponse>()
            .await
            .map_err(|e| PushError::TokenFetch(e.to_string()))
    }

    /// Build a single FCM v1 message body.
    fn build_message(
        token: Option<String>,
        title: &str,
        body: &str,
        image: Option<String>,
        data: HashMap<String, String>,
    ) -> FcmMessage {
        let mut apns_headers = HashMap::new();
        apns_headers.insert("apns-priority".to_string(), "10".to_string());

        FcmMessage {
            token,
            notification: FcmNotification {
                title: title.to_owned(),
                body: body.to_owned(),
                image: image.clone(),
            },
            data,
            android: FcmAndroidConfig {
                priority: "high".to_string(),
                notification: FcmAndroidNotification {
                    channel_id: "meno_default".to_string(),
                    click_action: "FLUTTER_NOTIFICATION_CLICK".to_string(),
                },
            },
            apns: FcmApnsConfig {
                headers: apns_headers,
                payload: FcmApnsPayload {
                    aps: FcmApnsAps {
                        alert: FcmApnsAlert {
                            title: title.to_owned(),
                            body: body.to_owned(),
                        },
                        badge: None,
                        sound: "default".to_string(),
                        content_available: 1,
                    },
                },
            },
        }
    }
}

// ==================== JWT HELPER ====================
/// Build an RS256-signed JWT assertion for the Google service-account `OAuth2` flow.
///
/// Requires the `jsonwebtoken` crate which is already in your dependency tree.
fn build_service_account_jwt(
    client_email: &str,
    private_key_pem: &str,
    token_uri: &str,
    now_secs: u64,
) -> Result<String, anyhow::Error> {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use serde_json::json;

    let claims = json!({
        "iss": client_email,
        "scope": "https://www.googleapis.com/auth/firebase.messaging",
        "aud": token_uri,
        "iat": now_secs,
        "exp": now_secs + 3600,
    });

    let key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
        .map_err(|e| anyhow::anyhow!("Invalid RSA PEM: {e}"))?;

    let header = Header::new(Algorithm::RS256);
    let token =
        encode(&header, &claims, &key).map_err(|e| anyhow::anyhow!("JWT encode error: {e}"))?;

    Ok(token)
}
