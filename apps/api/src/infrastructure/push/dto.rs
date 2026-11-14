//! The wire types and the credential parsing for the FCM v1 HTTP API.
//!
//! Ported from `apps/api/src/shared/services/push/dto.rs` on `master` (`903c3ba`).
//!
//! # Why the field names look wrong
//!
//! FCM's REST API is generated from protocol buffers and its JSON names are *not*
//! uniform: `AndroidConfig.priority` is camelCase-adjacent lowercase, but
//! `AndroidNotification.click_action` and `channel_id` are snake_case, and inside the
//! APNs payload `Aps.content_available` is serialised as the **hyphenated**
//! `content-available`. Every one of those is a `#[serde(rename)]` on a snake_case Rust
//! field, and every one of them is load-bearing — FCM rejects the whole message with a
//! `400 INVALID_ARGUMENT` if a key is wrong, which in production looks exactly like
//! "push stopped working for everyone".
//!
//! So the names here are written out explicitly rather than by `rename_all`, and
//! `tests::the_wire_names_are_exactly_what_fcm_expects` asserts the serialised JSON key
//! by key. A well-meaning `rename_all = "camelCase"` on these structs fails that test
//! instead of the free tier.
//!
//! # Credentials
//!
//! [`ServiceAccount`] is parsed from the `FIREBASE_SERVICE_ACCOUNT_JSON` value §4.6
//! moved to being inline. It is parsed once, at construction, so a malformed key is a
//! startup failure rather than a 3 a.m. one — and its private key is a
//! [`Secret`], so it cannot reach a log line through `Debug`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::Secret;
use crate::infrastructure::push::error::PushError;

// ── service account ─────────────────────────────────────────────────────────

/// The parts of a Google service-account key this adapter needs.
#[derive(Clone, Debug)]
pub struct ServiceAccount {
    /// The `client_email`, which becomes the JWT's `iss` claim.
    pub client_email: String,

    /// The PEM-encoded RSA private key used to sign the JWT.
    ///
    /// A [`Secret`] because `ServiceAccount` is `Debug` and this type ends up in
    /// adapter state that may be logged. A derived `Debug` would print the private key.
    pub private_key: Secret,

    /// The token endpoint, from the key's `token_uri`.
    ///
    /// `Option` because it is the one field a hand-written or trimmed-down key omits;
    /// [`Self::token_uri`] falls back to [`Self::DEFAULT_TOKEN_URI`].
    pub token_uri: Option<String>,

    /// The `project_id` recorded in the key.
    ///
    /// Kept for the log line only — the URL is built from
    /// [`crate::config::PushSettings::project_id`], which is what the operator set. If
    /// the two ever disagree the key's value is the more trustworthy one, so it is
    /// worth having on hand when that happens.
    pub project_id: Option<String>,
}

impl ServiceAccount {
    /// Google's OAuth2 token endpoint, used when the key carries no `token_uri`.
    pub const DEFAULT_TOKEN_URI: &'static str = "https://oauth2.googleapis.com/token";

    /// Parse the `FIREBASE_SERVICE_ACCOUNT_JSON` value.
    ///
    /// # Errors
    ///
    /// Returns [`PushError::TokenFetch`] if the JSON is malformed or omits
    /// `client_email` or `private_key`. Called at construction rather than lazily, so a
    /// bad key is reported once at startup with a message naming the missing field,
    /// rather than once per notification with a generic HTTP failure.
    pub fn parse(json: &str) -> Result<Self, PushError> {
        let raw: RawServiceAccount = serde_json::from_str(json).map_err(|e| {
            PushError::TokenFetch(format!("service-account JSON is not valid: {e}"))
        })?;

        Ok(Self {
            client_email: required(&raw.client_email, "client_email")?,
            private_key: Secret::new(required(&raw.private_key, "private_key")?),
            token_uri: raw.token_uri,
            project_id: raw.project_id,
        })
    }

    /// The token endpoint to post to.
    ///
    /// Falls back to [`Self::DEFAULT_TOKEN_URI`] for a missing *or* blank
    /// `token_uri`. The blank case is not hypothetical: a `.env` written as
    /// `FIREBASE_SERVICE_ACCOUNT_JSON='{"token_uri": "", …}'` — or a key that was
    /// assembled by string concatenation and lost the field — would otherwise become a
    /// request to the relative URL `""`, which fails as a transport error on every
    /// send with a message that never mentions the credential.
    #[must_use]
    pub fn token_uri(&self) -> &str {
        self.token_uri
            .as_deref()
            .map(str::trim)
            .filter(|uri| !uri.is_empty())
            .unwrap_or(Self::DEFAULT_TOKEN_URI)
    }
}

/// The on-disk shape of a Google service-account key.
///
/// Separate from [`ServiceAccount`] so that a key with *extra* fields — and real ones
/// have `type`, `auth_uri`, `auth_provider_x509_cert_url`, `client_id` and
/// `private_key_id` — deserialises instead of failing, and so the required fields can
/// each produce their own message.
#[derive(Deserialize)]
struct RawServiceAccount {
    client_email: Option<String>,
    private_key: Option<String>,
    token_uri: Option<String>,
    project_id: Option<String>,
}

/// Report a missing field by name.
///
/// A `serde` attribute could make `client_email` mandatory, but then a key missing
/// `private_key` and a key that is not JSON at all produce the same opaque message.
/// §4.6's lesson is that a startup error must say which variable is wrong.
fn required(field: &Option<String>, name: &str) -> Result<String, PushError> {
    match field.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => Ok(value.to_owned()),
        _ => Err(PushError::TokenFetch(format!(
            "service-account JSON is missing `{name}`"
        ))),
    }
}

// ── OAuth2 token exchange ───────────────────────────────────────────────────

/// Google's reply to a JWT-bearer token request.
///
/// Only the two fields that matter. `token_type` and `scope` come back too and are
/// ignored deliberately: asserting on them would fail the adapter for a response that
/// worked.
#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
    /// The bearer token to send as `Authorization: Bearer …`.
    pub access_token: String,

    /// Seconds until it expires. Used as `u64` because Google always sends an integer;
    /// a `f64` would invite silent truncation.
    pub expires_in: u64,
}

// ── FCM message envelope ────────────────────────────────────────────────────

/// The top-level body of `POST /v1/projects/{project}/messages:send`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcmEnvelope {
    /// FCM takes exactly one message per request; fan-out is N requests.
    pub message: FcmMessage,
}

/// One FCM v1 message.
///
/// `token` is an `Option` because the v1 API also addresses a message to a `topic` or a
/// `condition`, and those are mutually exclusive with `token`. Only device sends exist
/// today, but the field is modelled the way the API has it so a future topic send is a
/// new constructor rather than a new wire format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcmMessage {
    /// Device registration token. Absent for topic and condition sends.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    /// The user-visible notification.
    pub notification: FcmNotification,

    /// Arbitrary string key/value pairs passed through to the client app.
    pub data: HashMap<String, String>,

    /// Android-specific presentation and delivery options.
    pub android: FcmAndroidConfig,

    /// iOS-specific delivery options.
    pub apns: FcmApnsConfig,
}

/// The visible part of a notification.
///
/// Duplicated into `apns.payload.aps.alert` rather than referenced: FCM does not apply
/// the top-level `notification` to APNs, so omitting the APNs copy is how an app ends
/// up with Android alerts and silent iOS ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcmNotification {
    /// Notification title.
    pub title: String,

    /// Notification body.
    pub body: String,

    /// URL of an image to show with the notification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// Android delivery options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcmAndroidConfig {
    /// `high` or `normal`. See [`Self::PRIORITY_HIGH`].
    pub priority: String,

    /// Android notification presentation.
    pub notification: FcmAndroidNotification,
}

impl FcmAndroidConfig {
    /// Deliver even in Doze mode.
    ///
    /// Every notification Meno sends is time-sensitive — a broadcast going live, a
    /// reply arriving — and `normal` priority is deferred by Doze until the device
    /// next wakes, which on a phone in a pocket can be hours.
    pub const PRIORITY_HIGH: &'static str = "high";
}

/// Android notification presentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcmAndroidNotification {
    /// Android notification channel id — see [`Self::DEFAULT_CHANNEL_ID`].
    ///
    /// Wire name `channel_id`; FCM uses snake_case here even though most of the v1 API
    /// is camelCase.
    #[serde(rename = "channel_id")]
    pub channel_id: String,

    /// Intent action launched when the notification is tapped.
    ///
    /// Wire name `click_action`, again snake_case.
    #[serde(rename = "click_action")]
    pub click_action: String,
}

impl FcmAndroidNotification {
    /// The channel Meno's notifications are posted to.
    ///
    /// **Must match a channel created by the Flutter app.** A channel id that the app
    /// has not created is not an error — Android silently falls back to the default
    /// channel, which is why a mismatched id looks like "the notification has the
    /// wrong colour and no importance" rather than a failure.
    pub const DEFAULT_CHANNEL_ID: &'static str = "meno_default";

    /// The action the Flutter app registers for tap handling.
    ///
    /// Paired with the `deep_link` entry in `data`: the app's notification handler
    /// reads the payload, so the constant is the contract between this crate and
    /// `apps/mobile`.
    pub const CLICK_ACTION: &'static str = "FLUTTER_NOTIFICATION_CLICK";
}

/// APNs delivery options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcmApnsConfig {
    /// APNs request headers, e.g. `apns-priority`.
    pub headers: HashMap<String, String>,

    /// The APNs payload.
    pub payload: FcmApnsPayload,
}

impl FcmApnsConfig {
    /// Header key for APNs delivery priority.
    pub const PRIORITY_HEADER: &'static str = "apns-priority";

    /// Immediate delivery priority.
    ///
    /// `10` rather than `5`: a 5 is a power-conserving delivery that the system may
    /// bundle with other pushes, and "bundled with other pushes" means the broadcast
    /// went live while the user was reading something else.
    pub const PRIORITY_IMMEDIATE: &'static str = "10";
}

/// The APNs payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcmApnsPayload {
    /// The `aps` dictionary. Named as APNs names it.
    pub aps: FcmApnsAps,
}

/// The `aps` dictionary: everything APNs itself interprets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcmApnsAps {
    /// The visible alert.
    pub alert: FcmApnsAlert,

    /// Badge number on the app icon. Absent means "leave it alone".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<i64>,

    /// Sound file name, or `default`.
    pub sound: String,

    /// `1` asks iOS to wake the app for a silent background fetch.
    ///
    /// **Hyphenated on the wire**, unlike every sibling field. The proto field is
    /// `content_available`; FCM's JSON name for it is `content-available`.
    #[serde(rename = "content-available")]
    pub content_available: i32,
}

impl FcmApnsAps {
    /// `1` in [`Self::content_available`].
    pub const CONTENT_AVAILABLE: i32 = 1;

    /// The `sound` value that plays the system default.
    pub const SOUND_DEFAULT: &'static str = "default";
}

/// The visible part of an APNs alert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcmApnsAlert {
    /// Alert title.
    pub title: String,

    /// Alert body.
    pub body: String,
}

// ── error responses ─────────────────────────────────────────────────────────

/// FCM's error body, when it sends one.
///
/// Parsed but never propagated: see [`PushError`]'s module docs for why the message
/// text is dropped. Only [`Self::status`] is used, and only to refine a classification
/// that the HTTP code alone got wrong.
#[derive(Debug, Deserialize)]
pub struct FcmErrorResponse {
    /// The `error` object every Google API error is wrapped in.
    pub error: FcmErrorDetail,
}

/// The contents of an [`FcmErrorResponse`]'s `error` object.
#[derive(Debug, Deserialize)]
pub struct FcmErrorDetail {
    /// The HTTP status, repeated inside the body.
    #[serde(default)]
    pub code: u16,

    /// The canonical status, e.g. `UNREGISTERED` or `QUOTA_EXCEEDED`.
    #[serde(default)]
    pub status: Option<String>,

    /// Human-readable detail. Parsed so `serde` does not reject the body, and unused.
    #[serde(default)]
    pub message: Option<String>,
}

/// The canonical FCM statuses this adapter distinguishes.
///
/// Named constants rather than bare strings at the comparison sites, because the whole
/// classification is a `match` on an upstream's spelling of a failure.
mod fcm_status {
    /// The registration token is no longer valid.
    pub const UNREGISTERED: &str = "UNREGISTERED";

    /// The project has exceeded its send quota.
    pub const QUOTA_EXCEEDED: &str = "QUOTA_EXCEEDED";

    /// The equivalent status in the google.rpc.Code vocabulary.
    pub const RESOURCE_EXHAUSTED: &str = "RESOURCE_EXHAUSTED";
}

/// Turn an FCM response into a [`PushError`].
///
/// The HTTP status is the primary signal and FCM's canonical status — when the body
/// carries one — refines it. Refinement matters because FCM is not consistent: a stale
/// token arrives as `404` on some paths and as `400` with `UNREGISTERED` in the body on
/// others. Classifying only on the status means an unregistered token is treated as a
/// generic send failure, its row in `general_settings` is never deleted, and the app
/// keeps pushing to it forever — the precise defect the `404`-only branch on `master`
/// had.
///
/// `body` is only inspected for the canonical status; it is not retained or logged here.
#[must_use]
pub fn classify_failure(status: u16, body: &str) -> PushError {
    let canonical = serde_json::from_str::<FcmErrorResponse>(body)
        .ok()
        .and_then(|parsed| parsed.error.status);

    let named = canonical.as_deref().and_then(|canonical| match canonical {
        fcm_status::UNREGISTERED => Some(PushError::TokenInvalid),
        fcm_status::QUOTA_EXCEEDED | fcm_status::RESOURCE_EXHAUSTED => Some(PushError::RateLimited),
        _ => None,
    });

    named.unwrap_or(match status {
        // A 404 is how FCM says "no such token" on the v1 endpoint, whatever the body.
        404 => PushError::TokenInvalid,
        429 => PushError::RateLimited,
        other => PushError::SendFailed { status: other },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A minimal but complete service-account key, as `master`'s `.env` supplied it.
    fn valid_key() -> &'static str {
        r#"{
        "type": "service_account",
        "project_id": "meno-prod",
        "private_key_id": "abc123",
        "private_key": "-----BEGIN PRIVATE KEY-----\nfake\n-----END PRIVATE KEY-----\n",
        "client_email": "firebase@meno-prod.iam.gserviceaccount.com",
        "client_id": "1234567890",
        "token_uri": "https://oauth2.googleapis.com/token"
    }"#
    }

    // ── service account ──

    #[test]
    fn a_real_service_account_key_parses() {
        let account = ServiceAccount::parse(valid_key()).expect("the key must parse");

        assert_eq!(
            account.client_email,
            "firebase@meno-prod.iam.gserviceaccount.com"
        );
        assert_eq!(account.token_uri(), ServiceAccount::DEFAULT_TOKEN_URI);
        assert_eq!(account.project_id.as_deref(), Some("meno-prod"));
    }

    #[test]
    fn the_private_key_is_a_secret_and_never_prints() {
        // The leak this prevents: `Debug` on adapter state, which is logged, exposing
        // the service account's private key to every log reader.
        let account = ServiceAccount::parse(valid_key()).expect("parse");
        let rendered = format!("{account:?}");

        assert!(
            !rendered.contains("BEGIN PRIVATE KEY"),
            "the private key leaked into Debug output: {rendered}"
        );
        assert!(rendered.contains("[redacted]"), "{rendered}");
        assert!(account.private_key.expose().contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn a_key_without_a_token_uri_falls_back_to_googles_endpoint() {
        // A trimmed-down key is still usable; only the two signing fields are
        // genuinely required.
        let json = r#"{"client_email":"a@b.com","private_key":"pem"}"#;
        let account = ServiceAccount::parse(json).expect("parse");

        assert_eq!(account.token_uri(), "https://oauth2.googleapis.com/token");
        assert_eq!(account.project_id, None);
    }

    #[test]
    fn a_missing_signing_field_is_reported_by_name() {
        // `master` failed at the first send with "Invalid service-account JSON", which
        // does not say which variable is wrong.
        for (json, missing) in [
            (r#"{"private_key":"pem"}"#, "client_email"),
            (r#"{"client_email":"a@b.com"}"#, "private_key"),
            (r#"{"client_email":"","private_key":"pem"}"#, "client_email"),
            (
                r#"{"client_email":"a@b.com","private_key":"   "}"#,
                "private_key",
            ),
        ] {
            let error = ServiceAccount::parse(json).expect_err("must be rejected");
            assert!(
                error.to_string().contains(missing),
                "expected {missing:?} to be named, got {error}"
            );
        }
    }

    #[test]
    fn malformed_json_is_rejected_without_echoing_the_input() {
        // The parse error must not quote the value, which contains the private key.
        let json = r#"{"private_key": "-----BEGIN PRIVATE KEY----- secret"#;
        let error = ServiceAccount::parse(json).expect_err("must be rejected");

        assert!(!error.to_string().contains("secret"), "{error}");
    }

    #[test]
    fn unknown_fields_in_a_real_key_are_ignored() {
        // Real Google keys carry five fields this adapter does not use. Rejecting them
        // would mean the adapter breaks when Google adds a sixth.
        let json = r#"{
        "client_email": "a@b.com",
        "private_key": "pem",
        "auth_uri": "https://accounts.google.com/o/oauth2/auth",
        "auth_provider_x509_cert_url": "https://www.googleapis.com/oauth2/v1/certs",
        "universe_domain": "googleapis.com"
    }"#;
        assert!(ServiceAccount::parse(json).is_ok());
    }

    #[test]
    fn an_empty_token_uri_does_not_produce_an_empty_endpoint() {
        // `required`-style trimming applies to `token_uri` too: an empty string must
        // fall back, not become a request to `""`.
        let json = r#"{"client_email":"a@b.com","private_key":"pem","token_uri":""}"#;
        let account = ServiceAccount::parse(json).expect("parse");

        assert_eq!(account.token_uri(), ServiceAccount::DEFAULT_TOKEN_URI);
    }

    // ── wire format ──

    /// A message wired exactly as the sender builds one, with no `data` entries — so
    /// the assertions below are about FCM's field names, not about payload contents.
    fn envelope() -> FcmEnvelope {
        let mut apns_headers = HashMap::new();
        apns_headers.insert(
            FcmApnsConfig::PRIORITY_HEADER.to_owned(),
            FcmApnsConfig::PRIORITY_IMMEDIATE.to_owned(),
        );

        FcmEnvelope {
            message: FcmMessage {
                token: Some("device-token".to_owned()),
                notification: FcmNotification {
                    title: "Ada".to_owned(),
                    body: "The broadcast has started".to_owned(),
                    image: None,
                },
                data: HashMap::new(),
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
                                title: "Ada".to_owned(),
                                body: "The broadcast has started".to_owned(),
                            },
                            badge: None,
                            sound: FcmApnsAps::SOUND_DEFAULT.to_owned(),
                            content_available: FcmApnsAps::CONTENT_AVAILABLE,
                        },
                    },
                },
            },
        }
    }

    #[test]
    fn the_wire_names_are_exactly_what_fcm_expects() {
        // The whole point of the explicit renames above. A wrong key here is not a
        // compile error or a test failure anywhere near the bug — it is an FCM
        // `400 INVALID_ARGUMENT` that silently stops push for every user.
        let value = serde_json::to_value(envelope()).expect("serialises");

        assert_eq!(
            value["message"]["android"]["notification"]["channel_id"],
            json!("meno_default"),
            "channel_id must be snake_case"
        );
        assert_eq!(
            value["message"]["android"]["notification"]["click_action"],
            json!("FLUTTER_NOTIFICATION_CLICK"),
            "click_action must be snake_case"
        );
        assert_eq!(
            value["message"]["apns"]["payload"]["aps"]["content-available"],
            json!(1),
            "the aps field is hyphenated, not camelCase"
        );
        assert_eq!(value["message"]["android"]["priority"], json!("high"));
        assert_eq!(
            value["message"]["apns"]["headers"]["apns-priority"],
            json!("10")
        );
    }

    #[test]
    fn the_wire_names_that_are_absent_stay_absent() {
        // A `null` is not the same as "not specified" to FCM: `"image": null` is a
        // malformed `Notification`, and `"token": null` on a topic send is a malformed
        // `Message`. `skip_serializing_if` is what prevents that, so it is asserted.
        let rendered = serde_json::to_string(&envelope()).expect("serialises");

        assert!(
            !rendered.contains("\"image\""),
            "an absent optional must not be sent as null: {rendered}"
        );
        assert!(
            !rendered.contains("\"badge\""),
            "an absent badge must not be sent: {rendered}"
        );
        assert!(
            !rendered.contains("null"),
            "no optional field may serialise as null: {rendered}"
        );
        assert!(
            rendered.contains("\"token\":\"device-token\""),
            "a device send must carry its token: {rendered}"
        );
    }

    #[test]
    fn a_notification_image_is_carried_when_present() {
        let mut envelope = envelope();
        envelope.message.notification.image = Some("https://cdn.example.com/a.png".to_owned());

        let value = serde_json::to_value(envelope).expect("serialises");
        assert_eq!(
            value["message"]["notification"]["image"],
            json!("https://cdn.example.com/a.png")
        );
    }

    #[test]
    fn the_aps_alert_repeats_the_visible_text() {
        // FCM does not apply the top-level `notification` to APNs. If the aps copy is
        // dropped, iOS users get a silent push and the bug reads as "Android works,
        // iPhone doesn't".
        let value = serde_json::to_value(envelope()).expect("serialises");
        let aps = &value["message"]["apns"]["payload"]["aps"]["alert"];

        assert_eq!(aps["title"], value["message"]["notification"]["title"]);
        assert_eq!(aps["body"], value["message"]["notification"]["body"]);
    }

    // ── failure classification ──

    fn error_body(status: &str) -> String {
        json!({
            "error": {
                "code": 404,
                "message": "Requested entity was not found.",
                "status": status,
            }
        })
        .to_string()
    }

    #[test]
    fn a_404_is_a_stale_token_whatever_the_body_says() {
        // `master` mapped only the status and nothing else; this is the case that made
        // a rotated token look like a generic failure.
        let error = classify_failure(404, &error_body("NOT_FOUND"));

        assert!(error.is_stale_token(), "{error:?}");
    }

    #[test]
    fn an_unregistered_status_is_a_stale_token_even_on_a_400() {
        // The inconsistency that a status-only classification misses: FCM answers
        // `400 INVALID_ARGUMENT` with `UNREGISTERED` on some paths. Getting this wrong
        // means a dead token is never deleted from `general_settings`.
        let error = classify_failure(400, &error_body("UNREGISTERED"));

        assert!(
            error.is_stale_token(),
            "a 400 carrying UNREGISTERED must be a stale token, got {error:?}"
        );
    }

    #[test]
    fn a_quota_status_is_a_rate_limit_even_on_a_403() {
        let error = classify_failure(403, &error_body("QUOTA_EXCEEDED"));

        assert_eq!(error, PushError::RateLimited);
        assert_eq!(error.status(), Some(429));
    }

    #[test]
    fn resource_exhausted_is_also_a_rate_limit() {
        // The same condition under the google.rpc.Code name FCM sometimes uses.
        assert_eq!(
            classify_failure(429, &error_body("RESOURCE_EXHAUSTED")),
            PushError::RateLimited
        );
    }

    #[test]
    fn a_429_is_a_rate_limit_with_no_body_at_all() {
        // A proxy or a gateway in front of FCM can return 429 with an HTML body.
        assert_eq!(
            classify_failure(429, "gateway timeout"),
            PushError::RateLimited
        );
    }

    #[test]
    fn any_other_status_keeps_its_code() {
        for status in [400, 401, 403, 500, 502, 503] {
            let error = classify_failure(status, &error_body("INTERNAL"));
            assert_eq!(error, PushError::SendFailed { status });
            assert!(error.is_retryable());
        }
    }

    #[test]
    fn an_unparseable_body_falls_back_to_the_status_alone() {
        // Bodies are attacker-influenced (anything can sit behind the FCM hostname in
        // a misconfigured deployment), so classification must never require one.
        for body in ["", "<html>502</html>", "{\"error\":", "null", "[]"] {
            assert_eq!(
                classify_failure(502, body),
                PushError::SendFailed { status: 502 },
                "body {body:?} should not change the outcome"
            );
        }
    }

    #[test]
    fn an_unknown_canonical_status_does_not_override_the_status_code() {
        // Adding a new FCM status must not silently reclassify existing failures.
        let error = classify_failure(400, &error_body("SOMETHING_NEW"));

        assert_eq!(error, PushError::SendFailed { status: 400 });
    }

    #[test]
    fn classification_never_loses_the_status_for_a_hard_failure() {
        // `SendFailed` is the only variant carrying the status, and it is what an
        // on-call engineer greps for.
        let error = classify_failure(503, &error_body("UNAVAILABLE"));

        assert_eq!(error.status(), Some(503));
    }
}
