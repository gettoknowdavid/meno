use crate::modules::auth::handlers;
use crate::state::MenoState;
use axum::{Router, routing::post};

pub fn router() -> Router<std::sync::Arc<MenoState>> {
    Router::new()
        .route("/register", post(handlers::register))
        .route("/verify-email", post(handlers::verify_email))
        .route("/verify-email/resend", post(handlers::resend_otp))
        .route("/resend-otp", post(handlers::resend_otp))
        .route("/login", post(handlers::login))
        .route("/logout", post(handlers::logout))
}
