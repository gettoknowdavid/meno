//! The Brevo transactional-email adapter (plan §3.6, §4.6).
//!
//! # Why HTTPS and not SMTP
//!
//! §3.6 removes `lettre` and its `AsyncSmtpTransport` deliberately: speaking SMTP from
//! a request-serving process drags in connection pooling, STARTTLS negotiation and
//! deliverability failure modes that all surface as "email not sent" with no useful
//! classification. Brevo's HTTPS API answers with a status code, so the adapter's
//! failure vocabulary stays one `MenoError::Upstream` with a status in it — the same
//! shape every other outbound adapter in this tree reports.
//!
//! # How the `SMTP_*` settings map onto the API
//!
//! Email is enabled by `SMTP_HOST` being set (that is `config.rs`'s documented rule,
//! and it stays), but the transport is the API, so:
//!
//! | Setting | Used as |
//! | --- | --- |
//! | `SMTP_HOST` | the enable flag only — any value switches mail on |
//! | `SMTP_PASSWORD` | the Brevo **API key**, sent as the `api-key` header |
//! | `SMTP_FROM` | the sender address (bare, no display name) |
//! | `SMTP_PORT`, `SMTP_USER` | unused by the API transport |
//!
//! This is recorded in `.env.example` as well as here, because "configured and
//! silently ignored" is the worst failure mode for a setting like `SMTP_USER`.
//!
//! # What the message carries
//!
//! Subject plus an HTML body containing the six-digit code. The code's shape is
//! already enforced upstream ([`AuthEmailRequest`] documents it), so rendering it
//! needs no escaping and cannot inject markup. Brevo *templates* (numeric ids) are
//! the plan's eventual shape, but no configuration surface for them exists yet —
//! inventing undocumented variables to hold template ids would be a config contract
//! nobody asked for. When `EmailSettings` grows them, this is the one file that
//! changes.
//!
//! # No address in any log line
//!
//! The recipient is an email address — a record of who has an account, in a place
//! whose retention policy differs from the database's (§4.8, and the same rule
//! [`super::NoopAuthMailer`] follows). Failures log the status and the template key
//! and nothing else.

use async_trait::async_trait;
use meno_core::Error as MenoError;
use serde::Serialize;

use crate::config::{EmailSettings, Secret};
use crate::infrastructure::http;

use super::{AuthEmail, AuthEmailRequest, AuthMailer};

/// Brevo's transactional-message endpoint.
pub const DEFAULT_ENDPOINT: &str = "https://api.brevo.com/v3/smtp/email";

/// The mailer could not be constructed.
///
/// A startup failure, never a per-send one (§4.3): the two causes are a host that
/// cannot build the shared HTTP client and an endpoint that does not parse, and both
/// are permanent for the process's lifetime.
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    /// The shared outbound client could not be built.
    #[error("the mail HTTP client could not be built: {0}")]
    Http(#[source] http::BuildError),
    /// The endpoint is not a usable URL.
    ///
    /// Boxed rather than naming `url::ParseError`: `url` is a transitive dependency
    /// this crate does not declare, and a re-exported error type from a driver is the
    /// §5.5 leak the adapters exist to prevent.
    #[error("the mail endpoint is unusable: {0}")]
    Endpoint(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

/// The request body Brevo expects, minus the fields it fills in itself.
#[derive(Debug, Serialize)]
struct Payload {
    sender: Party,
    to: Vec<Party>,
    subject: &'static str,
    #[serde(rename = "htmlContent")]
    html_content: String,
}

/// An address side of the message.
#[derive(Debug, Serialize)]
struct Party {
    email: String,
}

/// HTTPS-backed [`AuthMailer`] for Brevo (plan §3.6).
pub struct BrevoMailer {
    http: reqwest::Client,
    endpoint: reqwest::Url,
    api_key: Secret,
    from: String,
}

impl std::fmt::Debug for BrevoMailer {
    /// Every field but the key is inert; the key is a credential (§9.5).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrevoMailer")
            .field("endpoint", &self.endpoint)
            .field("api_key", &"[redacted]")
            .field("from", &self.from)
            .finish_non_exhaustive()
    }
}

impl BrevoMailer {
    /// The adapter for `settings`, pointed at [`DEFAULT_ENDPOINT`].
    ///
    /// # Errors
    ///
    /// [`MailError`]; see its variants. Reported at startup by
    /// [`super::super::state::AuthState::default_mailer`].
    pub fn new(settings: &EmailSettings) -> Result<Self, MailError> {
        Self::with_endpoint(settings, DEFAULT_ENDPOINT)
    }

    /// Build against an explicit endpoint — the test seam, as in the push adapter.
    ///
    /// # Errors
    ///
    /// [`MailError`].
    pub fn with_endpoint(settings: &EmailSettings, endpoint: &str) -> Result<Self, MailError> {
        // Parsed once, at construction: a per-send parse failure would be an error
        // branch on the request path for a value that cannot change at runtime.
        let endpoint =
            reqwest::Url::parse(endpoint).map_err(|error| MailError::Endpoint(Box::new(error)))?;
        let http = http::shared().map_err(MailError::Http)?.clone();

        Ok(Self {
            http,
            endpoint,
            api_key: settings.password.clone(),
            from: settings.from.clone(),
        })
    }

    /// The rendered message for one [`AuthEmailRequest`].
    ///
    /// A function rather than an inline literal so the body can be asserted on
    /// without a server, and so subject and body can never disagree about which
    /// template is being sent.
    fn render(message: &AuthEmailRequest) -> (&'static str, String) {
        let subject = match message.kind {
            AuthEmail::VerifyEmail => "Verify your email address",
            AuthEmail::ResetPassword => "Reset your password",
        };

        // `code` is six ASCII digits by construction, so there is nothing here that
        // needs escaping — which is exactly why the interpolation is safe rather than
        // merely believed to be.
        let html = format!(
            "<html><body>\
               <p>{}</p>\
               <p style=\"font-size:28px;font-family:monospace;letter-spacing:0.3em\">{}</p>\
               <p>If you did not ask for this, you can ignore this email.</p>\
             </body></html>",
            subject, message.code,
        );

        (subject, html)
    }
}

#[async_trait]
impl AuthMailer for BrevoMailer {
    /// Send one message through Brevo's HTTPS API.
    ///
    /// # Errors
    ///
    /// [`MenoError::Upstream`] with `service: "mail"` — for a transport failure, a
    /// non-2xx answer, or a header value Brevo would not accept. The service logs it
    /// and reports success to the client either way; see [`super`] for why.
    async fn send(&self, message: &AuthEmailRequest) -> Result<(), MenoError> {
        let (subject, html_content) = Self::render(message);
        let payload = Payload {
            sender: Party {
                email: self.from.clone(),
            },
            to: vec![Party {
                email: message.to.clone(),
            }],
            subject,
            html_content,
        };

        let api_key = self.api_key.expose();

        let response = self
            .http
            .post(self.endpoint.clone())
            .header("api-key", api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(
                    template = message.kind.template_key(),
                    detail = %error,
                    "the mail transport failed"
                );
                upstream("the mail transport failed")
            })?;

        let status = response.status();
        if !status.is_success() {
            // The response body is deliberately *not* logged or returned: Brevo echoes
            // the recipient address in its error payloads, and this is the path that
            // reaches the log.
            tracing::warn!(
                template = message.kind.template_key(),
                status = %status.as_u16(),
                "brevo rejected a message"
            );
            return Err(upstream(&format!("brevo answered {status}")));
        }

        tracing::debug!(template = message.kind.template_key(), "mail sent");
        Ok(())
    }
}

/// One `Upstream { service: "mail" }`, so every refusal has one spelling.
fn upstream(detail: &str) -> MenoError {
    MenoError::Upstream {
        service: "mail",
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    //! The adapter's wire contract, against a loopback server: the header Brevo
    //! authenticates with, the address fields, and the classification of a rejection.
    //! None of these need a Brevo account, which is the point of `wiremock`.

    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn settings() -> EmailSettings {
        EmailSettings {
            host: "smtp.example.com".to_owned(),
            port: 587,
            user: "brevo".to_owned(),
            password: Secret::new("api-key-123"),
            from: "no-reply@example.com".to_owned(),
        }
    }

    fn message() -> AuthEmailRequest {
        AuthEmailRequest {
            to: "ada@example.com".to_owned(),
            kind: AuthEmail::VerifyEmail,
            code: "123456".to_owned(),
        }
    }

    async fn mailer_for(server: &MockServer) -> BrevoMailer {
        // The full path, mirroring production: a fixture that posted to the server
        // root would let a wrong-path regression pass unnoticed.
        let endpoint = format!("{}/v3/smtp/email", server.uri());
        BrevoMailer::with_endpoint(&settings(), &endpoint).expect("the fixture endpoint parses")
    }

    #[tokio::test]
    async fn a_message_carries_the_api_key_the_addresses_and_the_code() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v3/smtp/email"))
            .and(header("api-key", "api-key-123"))
            .respond_with(ResponseTemplate::new(201))
            .expect(1)
            .mount(&server)
            .await;

        let mailer = mailer_for(&server).await;
        mailer.send(&message()).await.expect("sent");

        // The body is asserted through the recorded request rather than a second
        // matcher, because what matters is the *pairing*: the code went to the
        // address the service asked for.
        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("the body is JSON");
        assert_eq!(body["sender"]["email"], "no-reply@example.com");
        assert_eq!(body["to"][0]["email"], "ada@example.com");
        assert_eq!(body["subject"], "Verify your email address");
        assert!(
            body["htmlContent"]
                .as_str()
                .expect("html")
                .contains("123456"),
            "the code must reach the template: {body}"
        );
    }

    #[tokio::test]
    async fn a_rejection_is_upstream_and_names_the_status_without_the_address() {
        // The classification the service relies on: a 4xx from Brevo is a provider
        // fault (retryable by them, not by the user), and neither the detail nor the
        // log may carry the recipient.
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "brevo rejected ada@example.com"
            })))
            .mount(&server)
            .await;

        let mailer = mailer_for(&server).await;
        let error = mailer.send(&message()).await.expect_err("rejected");

        // Asserted on the variant, not `Display`: the taxonomy's `Display` is a fixed
        // "mail unavailable" precisely so a log line cannot leak — the detail an
        // operator needs lives in the field, and reaches the log through the service.
        let MenoError::Upstream { service, detail } = &error else {
            panic!("a Brevo rejection must classify as Upstream, got {error:?}");
        };
        assert_eq!(*service, "mail");
        assert!(
            detail.contains("401"),
            "the status is what an operator needs: {detail}"
        );
        assert!(
            !detail.contains("ada@example.com"),
            "the recipient must not leak through the detail: {detail}"
        );
    }

    #[tokio::test]
    async fn a_reset_message_uses_the_reset_subject() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let mailer = mailer_for(&server).await;
        mailer
            .send(&AuthEmailRequest {
                to: "ada@example.com".to_owned(),
                kind: AuthEmail::ResetPassword,
                code: "654321".to_owned(),
            })
            .await
            .expect("sent");

        let requests = server.received_requests().await.expect("recorded");
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("the body is JSON");
        assert_eq!(body["subject"], "Reset your password");
    }

    #[test]
    fn an_unparsable_endpoint_is_a_construction_failure_not_a_send_failure() {
        // §4.3: a permanent problem stops the boot, rather than failing one send at a
        // time forever.
        let error =
            BrevoMailer::with_endpoint(&settings(), "not a url").expect_err("must be refused");

        assert!(matches!(error, MailError::Endpoint(_)));
    }

    #[test]
    fn debug_never_prints_the_api_key() {
        // §9.5. The key would otherwise be one `{:?}` of the mailer away from the log.
        let mailer = BrevoMailer::new(&settings()).expect("builds");
        let rendered = format!("{mailer:?}");

        assert!(!rendered.contains("api-key-123"), "{rendered}");
        assert!(rendered.contains("[redacted]"), "{rendered}");
    }
}
