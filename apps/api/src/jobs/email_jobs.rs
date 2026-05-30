use crate::shared::services::email::EmailService;
use crate::state::MenoState;
use apalis::prelude::{BoxDynError, Data};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SendVerificationEmailJob {
    pub to: String,
    pub full_name: String,
    pub otp: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SendResetPasswordEmailJob {
    pub to: String,
    pub full_name: String,
    pub otp: String,
}

pub async fn handle_send_verification_email(
    job: SendVerificationEmailJob,
    state: Data<Arc<MenoState>>,
) -> Result<(), BoxDynError> {
    let email_service = EmailService::new(&state.config);
    let subject = "Verify your Meno account";
    let html = verification_email_html(&job.full_name, &job.otp);
    email_service.send(&job.to, subject, html).await?;
    tracing::info!(to = %job.to, "Verification email sent");
    Ok(())
}

pub async fn handle_send_reset_password_email(
    job: SendResetPasswordEmailJob,
    state: Data<Arc<MenoState>>,
) -> Result<(), BoxDynError> {
    let email_service = EmailService::new(&state.config);
    let subject = "Reset your Meno password";
    let html = reset_password_email_html(&job.full_name, &job.otp);
    email_service.send(&job.to, subject, html).await?;
    tracing::info!(to = %job.to, "Password reset email sent");
    Ok(())
}

// HTML
fn verification_email_html(full_name: &str, otp: &str) -> String {
    format!(
        r#"
            <!DOCTYPE html>
            <html>
            <body style="margin:0;padding:0;background:#0f0f1a;font-family:sans-serif;">
              <div style="max-width:480px;margin:40px auto;background:#1a1a2e;border-radius:16px;padding:40px;text-align:center;">
                <h1 style="color:#ffffff;font-size:24px;margin-bottom:8px;">Verify your email</h1>
                <p style="color:#a0a0b8;margin-bottom:32px;">Hi {full_name}, enter this code in the app to verify your account.</p>
                <div style="background:#2a2a3e;border-radius:12px;padding:24px;margin-bottom:32px;">
                  <span style="color:#7c3aed;font-size:48px;font-weight:700;letter-spacing:12px;">{otp}</span>
                </div>
                <p style="color:#606080;font-size:14px;">This code expires in 15 minutes.<br/>If you didn't create a Meno account, ignore this email.</p>
              </div>
            </body>
            </html>
            "#,
        full_name = full_name,
        otp = otp
    )
}
fn reset_password_email_html(full_name: &str, otp: &str) -> String {
    format!(
        r#"
        <!DOCTYPE html>
            <html>
            <body style="margin:0;padding:0;background:#0f0f1a;font-family:sans-serif;">
              <div style="max-width:480px;margin:40px auto;background:#1a1a2e;border-radius:16px;padding:40px;text-align:center;">
                <h1 style="color:#ffffff;font-size:24px;margin-bottom:8px;">Reset your password</h1>

                <p style="color:#a0a0b8;margin-bottom:32px;">
                    Hi {full_name},<br>
                    You requested to reset your Meno account password.
                </p>

                <div style="background:#2a2a3e;border-radius:12px;padding:24px;margin-bottom:32px;">
                  <span style="color:#7c3aed;font-size:48px;font-weight:700;letter-spacing:12px;">{otp}</span>
                </div>

                <p style="color:#606080;font-size:14px;">
                    This code expires in 15 minutes.<br>
                    If you didn't request a password reset, please ignore this email.
                </p>

                <p style="color:#606080;font-size:13px;margin-top:32px;">
                    For security reasons, never share this code with anyone.
                </p>
              </div>
            </body>
            </html>
            "#,
        full_name = full_name,
        otp = otp
    )
}
