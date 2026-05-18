use crate::modules::auth::handlers;
use crate::state::MenoState;
use axum::{Router, routing::get, routing::post};

pub fn router() -> Router<std::sync::Arc<MenoState>> {
    Router::new()
        .route("/register", post(handlers::register))
        .route("/verify-email", post(handlers::verify_email))
        .route("/verify-email/resend", post(handlers::resend_otp))
        .route("/resend-otp", post(handlers::resend_otp))
        .route("/login", post(handlers::login))
        .route("/logout", post(handlers::logout))
        .route("/forgot-password", post(handlers::forgot_password))
        .route("/reset-password", post(handlers::reset_password))
        .route("/refresh", post(handlers::refresh))
        .route("/google/url", get(handlers::google_auth_url))
        .route("/google/web", post(handlers::google_web_callback))
        .route("/google/mobile", post(handlers::google_mobile_auth))
}
