use crate::shared::services::livekit::circuit_breaker::CircuitBreaker;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use std::sync::Arc;

#[derive(Clone)]
pub struct EmailService {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
    breaker: Arc<CircuitBreaker>,
}

impl EmailService {
    pub fn new(transport: AsyncSmtpTransport<Tokio1Executor>, from: String) -> Self {
        let breaker = CircuitBreaker::new(2, 60);
        Self {
            transport,
            from,
            breaker,
        }
    }

    pub async fn send(&self, to: &str, subject: &str, html: &str) -> anyhow::Result<()> {
        // If SMTP is down, we push to a job queue (apalis) instead of failing the request.
        // The circuit breaker here is for the *immediate* send path used by apalis workers.
        self.breaker.check().await.map_err(|e| anyhow::anyhow!(e))?;

        let message = lettre::Message::builder()
            .from(self.from.parse()?)
            .to(to.parse()?)
            .subject(subject)
            .header(lettre::message::header::ContentType::TEXT_HTML)
            .body(html.to_string())?;

        match self.transport.send(message).await {
            Ok(_) => {
                self.breaker.on_success().await;
                Ok(())
            }
            Err(e) => {
                self.breaker.on_failure().await;
                Err(anyhow::anyhow!("SMTP error: {}", e))
            }
        }
    }
}
