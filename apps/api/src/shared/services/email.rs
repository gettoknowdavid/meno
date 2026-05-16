use crate::config::MenoConfig;
use lettre::message::header::ContentType;
use lettre::transport::smtp::{authentication::Credentials, client::Tls};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

#[derive(Clone)]
pub struct EmailService {
    pub transport: AsyncSmtpTransport<Tokio1Executor>,
    pub from: String,
}
impl EmailService {
    pub fn new(config: &MenoConfig) -> Self {
        let creds = Credentials::new(config.smtp_user.clone(), config.smtp_password.clone());
        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)
            .unwrap()
            .port(config.smtp_port)
            .credentials(creds)
            .tls(Tls::None)
            .build();
        Self {
            from: config.smtp_from.clone(),
            transport,
        }
    }

    pub async fn send(&self, to: &str, subject: &str, html: String) -> anyhow::Result<()> {
        let email = Message::builder()
            .from(self.from.parse()?)
            .to(to.parse()?)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(html)?;
        self.transport.send(email).await?;
        Ok(())
    }

    pub fn verification_email_html(&self, full_name: &str, otp: &str) -> String {
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
}
