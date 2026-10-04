//! The auth endpoints (plan §9.3).
//!
//! # Thin on purpose
//!
//! Each function is parse → validate → delegate → respond, and nothing else. §9.3 says
//! "no business logic in `handlers.rs`", and that is not a style preference: the
//! decisions these endpoints make — whether an address is taken, whether a code is
//! still live, whether a session belongs to the caller — are the ones that have to be
//! testable without HTTP. Putting one here means it can only be tested by driving a
//! router, and the interesting failures are the ones a router test cannot set up.
//!
//! So the rule this file keeps is narrow and checkable: a handler may read a body, call
//! `validate`, call exactly one service method, and build a [`MenoResponse`]. If a
//! handler ever needs an `if` that is not "which extractor did I get", it belongs in
//! [`super::services`] or [`super::credentials`].
//!
//! # Errors
//!
//! Every function returns [`Outcome`], whose error is [`Failure`] — a one-field
//! newtype whose only job is to render a [`MenoError`] through
//! [`crate::middleware::from_error`], the crate's single §4.2 renderer. The newtype
//! exists because `impl From<MenoError> for Response` is the orphan rule: neither type
//! is local. Handlers do not build error bodies themselves, which is what stops the nine
//! parallel shapes `master` had from reappearing.
//!
//! # Authentication
//!
//! The three session endpoints read [`AuthUser`] from request extensions, inserted by
//! [`crate::middleware::auth::auth_middleware`]. That extractor is what enforces the
//! layer order documented in [`crate::middleware`]; a missing `AuthUser` there is a
//! wiring bug reported as a 500, not a user who forgot to log in.

use axum::extract::{Extension, Path, State};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use super::dto::{
    AuthResponse, ForgotPasswordRequest, GoogleMobileAuthRequest, GoogleWebAuthRequest,
    LoginRequest, LogoutRequest, RefreshTokenRequest, RegisterRequest, ResendOtpRequest,
    ResetPasswordRequest, SessionResponse, VerifyEmailRequest,
};
use super::google::GoogleAuthorize;
use super::state::AuthState;
use crate::middleware::auth::AuthUser;
use crate::middleware::extractors::MenoBody;
use crate::middleware::from_error;
use crate::types::meno_response::MenoResponse;

/// A handler result: the envelope on success, the shared renderer on failure.
pub type Outcome<T> = Result<MenoResponse<T>, Failure>;

/// A domain failure on its way to the wire.
///
/// Deliberately not a `Response`. Returning the rendered `Response` as the error type
/// would type `axum::response::Response` into every handler signature, and a handler
/// that wanted to *inspect* a failure — a test asserting the code, or a future handler
/// that adds a header — would find it had already been thrown away.
#[derive(Debug)]
pub struct Failure(pub meno_core::Error);

impl From<meno_core::Error> for Failure {
    fn from(error: meno_core::Error) -> Self {
        Self(error)
    }
}

impl IntoResponse for Failure {
    /// The §4.2 boundary. `middleware::from_error` is the only place in the crate that
    /// builds an error body, so this is the only place that reaches for it.
    fn into_response(self) -> Response {
        from_error(&self.0)
    }
}

/// `POST /auth/register`.
pub async fn register(
    State(state): State<AuthState>,
    MenoBody(body): MenoBody<RegisterRequest>,
) -> Outcome<AuthResponse> {
    let response = state.service.register(&body).await?;

    Ok(MenoResponse::created(
        "Account created successfully",
        response,
    ))
}

/// `POST /auth/login`.
pub async fn login(
    State(state): State<AuthState>,
    MenoBody(body): MenoBody<LoginRequest>,
) -> Outcome<AuthResponse> {
    let response = state.service.login(&body).await?;

    Ok(MenoResponse::ok("Login successful", response))
}

/// `POST /auth/refresh`.
pub async fn refresh(
    State(state): State<AuthState>,
    MenoBody(body): MenoBody<RefreshTokenRequest>,
) -> Outcome<AuthResponse> {
    let response = state.service.refresh(&body).await?;

    Ok(MenoResponse::ok("Token refreshed successfully", response))
}

/// `POST /auth/logout`.
///
/// 200 with no payload rather than 204: the §4.2 envelope has to carry the `code` a
/// client matches on, and a 204 has no body to put it in.
pub async fn logout(
    State(state): State<AuthState>,
    MenoBody(body): MenoBody<LogoutRequest>,
) -> Outcome<()> {
    state.service.logout(&body).await?;

    Ok(MenoResponse::no_content("Logout successful"))
}

/// `POST /auth/verify-email`.
pub async fn verify_email(
    State(state): State<AuthState>,
    MenoBody(body): MenoBody<VerifyEmailRequest>,
) -> Outcome<AuthResponse> {
    let response = state.credentials.verify_email(&body).await?;

    Ok(MenoResponse::ok("Account verified successfully", response))
}

/// `POST /auth/resend-otp`.
pub async fn resend_otp(
    State(state): State<AuthState>,
    MenoBody(body): MenoBody<ResendOtpRequest>,
) -> Outcome<()> {
    state.credentials.resend_otp(&body).await?;

    Ok(MenoResponse::no_content("Verification email resent"))
}

/// `POST /auth/forgot-password`.
pub async fn forgot_password(
    State(state): State<AuthState>,
    MenoBody(body): MenoBody<ForgotPasswordRequest>,
) -> Outcome<()> {
    state.credentials.forgot_password(&body).await?;

    Ok(MenoResponse::no_content("Password reset email sent"))
}

/// `POST /auth/reset-password`.
pub async fn reset_password(
    State(state): State<AuthState>,
    MenoBody(body): MenoBody<ResetPasswordRequest>,
) -> Outcome<()> {
    state.credentials.reset_password(&body).await?;

    Ok(MenoResponse::no_content("Password reset successful"))
}

/// `GET /auth/sessions` — the device list (§4.7 item 3).
pub async fn list_sessions(
    State(state): State<AuthState>,
    Extension(user): Extension<AuthUser>,
) -> Outcome<Vec<SessionResponse>> {
    let sessions = state.service.list_sessions(user.id).await?;

    Ok(MenoResponse::ok(
        "Sessions retrieved successfully",
        sessions,
    ))
}

/// `POST /auth/sessions/{id}/revoke` (§4.7 item 3).
pub async fn revoke_session(
    State(state): State<AuthState>,
    Extension(user): Extension<AuthUser>,
    Path(session_id): Path<Uuid>,
) -> Outcome<()> {
    state.service.revoke_session(user.id, session_id).await?;

    Ok(MenoResponse::no_content("Session revoked successfully"))
}

/// `POST /auth/logout-all` — "log out everywhere" (§4.7 item 3).
pub async fn logout_everywhere(
    State(state): State<AuthState>,
    Extension(user): Extension<AuthUser>,
) -> Outcome<()> {
    state.service.logout_everywhere(user.id).await?;

    Ok(MenoResponse::no_content("Logged out on all devices"))
}

/// `GET /auth/google/url` — where to send the browser.
pub async fn google_authorize_url(State(state): State<AuthState>) -> Outcome<GoogleAuthorize> {
    let authorize = state
        .service
        .google_authorize(state.google.as_ref())
        .await?;

    Ok(MenoResponse::ok("Google auth URL generated", authorize))
}

/// `POST /auth/google/callback` — the web flow.
pub async fn google_web_callback(
    State(state): State<AuthState>,
    MenoBody(body): MenoBody<GoogleWebAuthRequest>,
) -> Outcome<AuthResponse> {
    let response = state
        .service
        .google_web_auth(&body, state.google.as_ref())
        .await?;

    Ok(MenoResponse::ok(
        "Google authentication successful",
        response,
    ))
}

/// `POST /auth/google` — the mobile flow.
pub async fn google_mobile_auth(
    State(state): State<AuthState>,
    MenoBody(body): MenoBody<GoogleMobileAuthRequest>,
) -> Outcome<AuthResponse> {
    let response = state
        .service
        .google_mobile_auth(&body, state.google.as_ref())
        .await?;

    Ok(MenoResponse::ok(
        "Google authentication successful",
        response,
    ))
}
