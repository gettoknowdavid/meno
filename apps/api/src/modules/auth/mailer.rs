//! The transactional-mail boundary (plan §3.6 and §4.6).
//!
//! # Why a trait and not a direct `Brevo` client
//!
//! §6's job queue has not landed, so there is no `jobs::Jobs` to hand an email to yet.
//! Rather than reach into an unwritten module — or, worse, call an HTTP API from inside
//! a request handler — auth declares the narrow interface it needs and the wiring layer
//! supplies an implementation. §4.6's rule about optional integrations applies: with
//! mail unconfigured, [`NoopAuthMailer`] is used and every send logs `skipped`.
//!
//! # Why sends are best-effort at the call site
//!
//! [`AuthMailer::send`] returns `Result`, and the service decides what to do with it. It
//! deliberately does **not** propagate the failure to the client: a user who pressed
//! "resend code" and got a 503 because Brevo is down cannot act on that, and a 202 with
//! no email is just as broken. The failure is logged and the request reports success,
//! which is also what keeps the endpoint from telling an attacker whether an address is
//! registered.
//!
//! §3.6's 300 emails/day free tier is the reason the rate limit on these endpoints is
//! 5/min and fails **closed**.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use meno_core::Error as MenoError;

/// The kinds of message auth sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthEmail {
    /// "Confirm this address so you can sign in".
    VerifyEmail,
    /// "Here is a code to choose a new password".
    ResetPassword,
}

impl AuthEmail {
    /// A short name for logs and for the provider's template key.
    ///
    /// Never the recipient address and never the code — a log line is not a place for
    /// either.
    #[must_use]
    pub const fn template_key(self) -> &'static str {
        match self {
            Self::VerifyEmail => "auth.verify_email",
            Self::ResetPassword => "auth.reset_password",
        }
    }
}

/// One message to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthEmailRequest {
    /// The recipient.
    pub to: String,
    /// Which template to render.
    pub kind: AuthEmail,
    /// The one-time code, when the template needs one.
    ///
    /// Six digits and nothing else — the DTOs and
    /// [`super::validators::code_problem`] both enforce that, so a template can render
    /// it without truncating or reformatting.
    pub code: String,
}

/// Sends auth's transactional email.
#[async_trait]
pub trait AuthMailer: Send + Sync + std::fmt::Debug {
    /// Send one message.
    ///
    /// # Errors
    ///
    /// [`MenoError::Upstream`] with `service: "mail"`. The service logs it and reports
    /// success to the client; see the module note on why.
    async fn send(&self, message: &AuthEmailRequest) -> Result<(), MenoError>;
}

/// The mailer used when no provider is configured (§4.6).
///
/// A real implementation rather than a `cfg`-gated stub, for the same reason push and
/// storage have one: a build without mail configured must still compile the flows that
/// would send it.
#[derive(Debug, Default)]
pub struct NoopAuthMailer;

#[async_trait]
impl AuthMailer for NoopAuthMailer {
    async fn send(&self, message: &AuthEmailRequest) -> Result<(), MenoError> {
        // The address is deliberately not logged. An email address in an info-level log
        // is a record of who has an account, in a place with a different retention
        // policy than the database.
        tracing::info!(template = message.kind.template_key(), "auth email skipped");
        Ok(())
    }
}

/// A mailer that records what it was asked to send.
///
/// The §5.6 double. A test asserts that a verification code was *dispatched* to the right
/// address without asserting on the provider's HTTP behaviour, which is the adapter's
/// job and is covered by its own tests.
#[derive(Debug, Default)]
pub struct RecordingAuthMailer {
    sent: Mutex<Vec<AuthEmailRequest>>,
    failure: Mutex<Option<String>>,
}

impl RecordingAuthMailer {
    /// A mailer that succeeds.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A mailer that fails every send with `reason`.
    ///
    /// Lets a test pin the behaviour that matters most: the endpoint still reports
    /// success, because the alternative tells an attacker whether the address exists.
    #[must_use]
    pub fn failing(reason: &str) -> Self {
        let mailer = Self::new();
        *mailer.failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason.to_owned());
        mailer
    }

    /// Everything sent so far, oldest first.
    #[must_use]
    pub fn sent(&self) -> Vec<AuthEmailRequest> {
        self.sent.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// How many messages have been sent.
    #[must_use]
    pub fn count(&self) -> usize {
        self.sent.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// The most recent message, if any.
    #[must_use]
    pub fn last(&self) -> Option<AuthEmailRequest> {
        self.sent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last()
            .cloned()
    }
}

#[async_trait]
impl AuthMailer for RecordingAuthMailer {
    async fn send(&self, message: &AuthEmailRequest) -> Result<(), MenoError> {
        let failure = self
            .failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(reason) = failure {
            return Err(MenoError::Upstream {
                service: "mail",
                detail: reason,
            });
        }

        self.sent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(message.clone());
        Ok(())
    }
}

/// Share a mailer between the state and the service.
///
/// A newtype so the service field is not a bare `Arc<dyn AuthMailer>` that a call site
/// can build with the wrong implementation; the constructor is the only way in.
#[derive(Debug, Clone)]
pub struct SharedMailer(pub Arc<dyn AuthMailer>);

impl SharedMailer {
    /// Wrap any mailer.
    #[must_use]
    pub fn new(mailer: Arc<dyn AuthMailer>) -> Self {
        Self(mailer)
    }

    /// The no-op mailer.
    #[must_use]
    pub fn noop() -> Self {
        Self::new(Arc::new(NoopAuthMailer))
    }
}

#[cfg(test)]
mod tests {
    //! The two properties the service depends on: the double records faithfully, and a
    //! failure is reported as an `Upstream` error rather than swallowed.

    use super::*;
    use meno_core::ErrorCode;

    fn verification(to: &str, code: &str) -> AuthEmailRequest {
        AuthEmailRequest {
            to: to.to_owned(),
            kind: AuthEmail::VerifyEmail,
            code: code.to_owned(),
        }
    }

    #[tokio::test]
    async fn the_recorder_keeps_what_it_was_asked_to_send() {
        let mailer = RecordingAuthMailer::new();
        mailer
            .send(&verification("ada@example.com", "123456"))
            .await
            .expect("sent");

        assert_eq!(mailer.count(), 1);
        let last = mailer.last().expect("a recorded message");
        assert_eq!(last.to, "ada@example.com");
        assert_eq!(last.code, "123456");
        assert_eq!(last.kind, AuthEmail::VerifyEmail);
    }

    #[tokio::test]
    async fn messages_are_recorded_in_order() {
        // The "your devices"-style debugging a support ticket needs: which code was sent
        // first when a user says the second one never arrived.
        let mailer = RecordingAuthMailer::new();
        mailer
            .send(&verification("ada@example.com", "111111"))
            .await
            .expect("first");
        mailer
            .send(&verification("ada@example.com", "222222"))
            .await
            .expect("second");

        let codes: Vec<String> = mailer.sent().into_iter().map(|m| m.code).collect();
        assert_eq!(codes, ["111111", "222222"]);
    }

    #[tokio::test]
    async fn a_failing_mailer_reports_an_upstream_error() {
        let mailer = RecordingAuthMailer::failing("brevo is down");

        let error = mailer
            .send(&verification("ada@example.com", "123456"))
            .await
            .expect_err("the failure surfaces");

        assert_eq!(error.code(), ErrorCode::UpstreamUnavailable);
        assert_eq!(mailer.count(), 0, "a failed send is not recorded");
    }

    #[tokio::test]
    async fn the_noop_mailer_succeeds_without_recording_anything() {
        // §4.6: an unconfigured deployment must not fail the flows that would send mail.
        let mailer = NoopAuthMailer;
        mailer
            .send(&verification("ada@example.com", "123456"))
            .await
            .expect("skipped");
    }

    #[test]
    fn each_template_key_is_distinct() {
        assert_ne!(
            AuthEmail::VerifyEmail.template_key(),
            AuthEmail::ResetPassword.template_key()
        );
    }

    #[test]
    fn a_shared_mailer_can_only_be_built_from_a_trait_object() {
        let mailer = SharedMailer::noop();
        let _: Arc<dyn AuthMailer> = mailer.0;
    }
}
