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
}
