use crate::modules::auth::handlers as h;
use crate::shared::middleware::idempotency::idempotency_middleware;
use crate::state::MenoState;
use axum::middleware::from_fn;
use axum::{Router, routing::get, routing::post};

pub fn router() -> Router<std::sync::Arc<MenoState>> {
    let normal = Router::<std::sync::Arc<MenoState>>::new()
        .route("/refresh", post(h::refresh))
        .route("/google/url", get(h::google_auth_url))
        .route("/google/web", post(h::google_web_callback))
        .route("/google/mobile", post(h::google_mobile_auth));

    let idempotent = Router::<std::sync::Arc<MenoState>>::new()
        .route("/register", post(h::register))
        .route("/login", post(h::login))
        .route("/logout", post(h::logout))
        .route("/forgot-password", post(h::forgot_password))
        .route("/reset-password", post(h::reset_password))
        .route("/verify-email", post(h::verify_email))
        .route("/verify-email/resend", post(h::resend_otp))
        .route("/resend-otp", post(h::resend_otp))
        .layer(from_fn(idempotency_middleware));

    normal.merge(idempotent)
}
