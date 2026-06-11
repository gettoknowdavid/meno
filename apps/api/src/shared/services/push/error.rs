#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("FCM token is no longer registered (stale token)")]
    TokenInvalid,

    #[error("FCM rate limit exceeded")]
    RateLimited,

    #[error("FCM send failed with status {0}")]
    SendFailed(u16),

    #[error("Circuit breaker is open — FCM unavailable")]
    CircuitOpen,

    #[error("OAuth2 token fetch failed: {0}")]
    TokenFetch(String),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
