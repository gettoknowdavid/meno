use crate::shared::constants::RATE_LIMIT_PREFIX;
use crate::state::MenoState;
use anyhow::Result;
use axum::http::StatusCode;
use axum::{
    Extension,
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use fred::prelude::*;
use std::{sync::Arc, time::SystemTime};

#[derive(Debug, Clone, Copy)]
pub struct RateLimitConfig {
    pub limit: usize,
    pub window_secs: u64,
}
impl RateLimitConfig {
    pub fn new(limit: usize, window_secs: u64) -> Self {
        Self { limit, window_secs }
    }
}

static SCRIPT: &str = r#"
    local current_key = KEYS[1]
    local previous_key = KEYS[2]
    local tokens = tonumber(ARGV[1])
    local window_secs = tonumber(ARGV[2])
    local now = tonumber(ARGV[3])
    local limit = tonumber(ARGV[4])

    local current_count = redis.call('INCRBY', current_key, tokens)
    redis.call('EXPIRE', current_key, window_secs * 2)

    local prev_count = tonumber(redis.call('GET', previous_key) or "0")

    local window_ms = window_secs * 1000
    local time_in_window = now % window_ms
    local weight = (window_ms - time_in_window) / window_ms

    local estimated = current_count + math.floor(prev_count * weight)

    if estimated > limit then
        return {0, current_count}
    else
        return {limit - estimated, current_count}
    end
"#;

async fn check_rate_limit(pool: &Pool, base_key: &str, config: RateLimitConfig) -> Result<usize> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_millis() as u64;

    let window_ms = config.window_secs * 1000;
    let current_window = now / window_ms;
    let prev_window = current_window.saturating_sub(1);

    let current_key = format!("{}:{}", base_key, current_window);
    let previous_key = format!("{}:{}", base_key, prev_window);

    let keys = vec![&current_key, &previous_key];
    let args = vec![
        1usize,
        config.window_secs as usize,
        now as usize,
        config.limit,
    ];

    let result: (usize, usize) = pool.eval(SCRIPT, keys, args).await?;

    Ok(result.0)
}

/// Extracts a unique identifier for rate limiting.
///
/// Prefers JWT bearer token subclaim; falls back to IP address.
fn extract_identifier(req: &Request<Body>) -> String {
    // Try to get user ID from Authorization header
    // Full JWT parsing happens in the auth middleware (E2); here we just use the raw token
    // as a proxy identifier — good enough for rate limiting
    if let Some(auth_header) = req.headers().get("Authorization")
        && let Ok(auth_str) = auth_header.to_str()
        && let Some(token) = auth_str.strip_prefix("Bearer ")
    {
        // Use last 16 chars of token as identifier (avoids logging full token)
        let len = token.len();
        if len >= 16 {
            return token[len - 16..].to_string();
        }
    }

    // Fall back to IP from X-Forwarded-For (set by Railway/Render/Cloudflare)
    // then to direct connection IP
    if let Some(forwarded) = req.headers().get("X-Forwarded-For")
        && let Ok(ip) = forwarded.to_str()
    {
        return ip.split(',').next().unwrap_or("unknown").trim().to_string();
    }

    "unknown".to_string()
}

pub async fn rate_limit_middleware(
    State(state): State<Arc<MenoState>>,
    Extension(maybe_custom): Extension<Option<RateLimitConfig>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let config = maybe_custom.unwrap_or(state.config.default_rate_limit);

    let identifier = extract_identifier(&req);
    let key = format!("{}:{}", RATE_LIMIT_PREFIX, identifier);

    match check_rate_limit(&state.redis, &key, config).await {
        Ok(remaining) if remaining > 0 => {
            let mut response = next.run(req).await;
            response.headers_mut().insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            response.headers_mut().insert(
                "X-RateLimit-Limit",
                config.limit.to_string().parse().unwrap(),
            );
            response
        }
        Ok(_) => (
            StatusCode::TOO_MANY_REQUESTS,
            [
                ("Retry-After", config.window_secs.to_string().as_str()),
                ("X-RateLimit-Limit", config.limit.to_string().as_str()),
                ("X-RateLimit-Remaining", "0"),
                ("Content-Type", "application/json"),
            ],
            r#"{"data":null,"meta":null,"error":{"message":"Too many requests."}}"#,
        )
            .into_response(),
        Err(_) => next.run(req).await,
    }
}
