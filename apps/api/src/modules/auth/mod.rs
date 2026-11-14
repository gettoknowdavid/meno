//! Authentication: registration, sign-in, sessions and password resets (plan §4.7).
//!
//! # The shape of the module
//!
//! Five layers, each answering one question, so that a rule has exactly one home:
//!
//! | File | Question | Change it when |
//! | --- | --- | --- |
//! | [`dto`] + [`validators`] | is this shape acceptable? | a field or a rule changes |
//! | [`error`] | which taxonomy entry is this refusal? | a client needs to branch on it |
//! | [`model`] | what do the rows look like? | the schema changes |
//! | [`repository`] / [`cache`] | what exists? | storage does |
//! | [`token`] | what does a token mean? | the claim set or lifetimes change |
//! | [`services`] / [`credentials`] | in what order? | a *flow* changes |
//! | [`google`] | may this identity be linked? | the provider's contract changes |
//! | [`mailer`] | how does a code reach a person? | the transport changes |
//! | [`password`] | how is a password stored? | the KDF or its parameters change |
//! | [`state`] | which adapter is real here? | wiring changes |
//! | [`handlers`] | how does a request become a response? | the wire contract changes |
//!
//! # The invariants this module holds
//!
//! These are the properties worth stating once, because each is asserted somewhere in
//! the tests and each has a specific place it is enforced:
//!
//! - **§7.12 — no unverified-email takeover.** A Google identity can only reach the
//!   account-linking code through [`google::GoogleExchange`], which refuses an
//!   unverified address. [`google::GoogleIdentity`] has no public constructor, so a
//!   caller cannot build one that skipped the check.
//! - **§4.7 item 1 — device-bound sessions.** Every refresh token belongs to an
//!   `auth_sessions` row; [`token::TokenService::refresh`] is valid only for the session
//!   it was issued to.
//! - **§4.7 item 2 — reuse detection.** A refresh token presented after it was rotated
//!   revokes every session for that user.
//! - **§4.7 item 3 — revoke by device.** [`handlers::list_sessions`] and
//!   [`handlers::revoke_session`], plus [`services::AuthService::logout_everywhere`].
//! - **§4.7 item 4 — separate secrets.** [`token::TokenConfig::validate`] refuses a
//!   shared pair, so a refresh token cannot be presented as an access token.
//! - **§4.7 item 5 — blocklist.** Redis, with a TTL on every key (§9.4).
//! - **No enumeration.** Login, resend and forgot-password answer identically whether or
//!   not the address exists, and login still verifies a password against a dummy hash so
//!   the timing matches too.
//! - **§9.1 — no `unwrap`, `expect` or `panic` on a request path.** Construction is
//!   fallible: [`state::AuthState::new`] returns a `Result` naming which setting is
//!   wrong, rather than panicking the way `master`'s two `assert!`s and five `expect`s
//!   did.
//!
//! # What is not here
//!
//! Rate limiting is §4.7 item 8, but it is a router concern and already exists as
//! [`crate::middleware::rate_limit`]; applying it to `/auth/*` happens in `routes.rs`.
//! Email transport is [`mailer::brevo::BrevoMailer`] (plan §3.6 — Brevo's HTTPS API,
//! not raw SMTP) when `SMTP_HOST` is set, and [`mailer::NoopAuthMailer`] when it is
//! not; [`state::AuthState::default_mailer`] is where that choice is made.

pub mod cache;
pub mod credentials;
pub mod dto;
pub mod error;
pub mod google;
pub mod handlers;
pub mod mailer;
pub mod model;
pub mod password;
pub mod repository;
pub mod services;
pub mod state;
pub mod token;
pub mod validators;

// The module's contract tests live in `apps/api/tests/auth_service.rs`, outside the
// crate, where every path starts at `meno_api::` and the fixtures are shared with
// `auth_router.rs` through `tests/support/mod.rs`.
