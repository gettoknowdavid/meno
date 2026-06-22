#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct SendEmailJob {
    pub to: String,
    pub subject: String,
    pub html: String,
}

///
/// Sends an email asynchronously using the provided `SendEmailJob` and application state.
///
/// # Arguments
///
/// * `job` - A `SendEmailJob` struct containing the recipient's email address, subject, and HTML content of the email.
/// * `state` - An `Arc` wrapped `MenoState` structure containing the application's shared state, including SMTP transport and configuration.
///
/// # Returns
///
/// A `Result` indicating success (`Ok(())`) or error (`Err(BoxDynError)`) if the email could not be sent.
///
/// # Behavior
///
/// * Clones the SMTP transport and sender email address from the shared application state.
/// * Initializes an `EmailService` with the transport and sender email address.
/// * Sends an email to the recipient address specified in the job with the provided subject and HTML content.
/// * Logs an info message using `tracing` once the email is successfully sent, with the `to` field indicating the recipient.
///
/// # Errors
///
/// This function will return an error if:
/// * There is an issue initializing or cloning the SMTP transport.
/// * The email fails to be sent by the `EmailService`.
///
/// # Example
///
/// ```rust
/// use std::sync::Arc;
/// use actix_web::web::Data;
///
/// let state = Data::new(Arc::new(MenoState::new()));
/// let job = SendEmailJob {
///     to: "user@example.com".to_string(),
///     subject: "Welcome".to_string(),
///     html: "<h1>Welcome to our service</h1>".to_string(),
/// };
///
/// if let Err(e) = send_email(job, state).await {
///     eprintln!("Failed to send email: {:?}", e);
/// }
/// ```
pub async fn send_email(
    job: SendEmailJob,
    state: apalis::prelude::Data<std::sync::Arc<crate::state::MenoState>>,
) -> Result<(), apalis::prelude::BoxDynError> {
    let transport = state.smtp.clone();
    let from = state.config.smtp_from.clone();
    let email = crate::shared::email::EmailService::new(transport, from);
    email.send(&job.to, &job.subject, &job.html).await?;
    tracing::info!(to = %job.to, "Verification email sent");
    Ok(())
}

// HTML
/// Returns a Tuple (subject, html)
#[must_use]
pub fn verify_email_html(full_name: &str, otp: &str) -> (String, String) {
    let full_name = crate::shared::utils::escape_html(full_name);
    let html = format!(
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
            "#
    );
    ("Verify your Meno account".to_string(), html)
}

/// Returns a Tuple (subject, html)
#[must_use]
pub fn reset_pwd_email_html(full_name: &str, otp: &str) -> (String, String) {
    let full_name = crate::shared::utils::escape_html(full_name);
    let html = format!(
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
    );
    ("Reset your password".to_string(), html)
}
