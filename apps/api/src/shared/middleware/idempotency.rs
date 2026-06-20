use crate::shared::services::redis::keys::RedisKey;
use crate::shared::services::redis::Redis;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Cached response stored in Redis for a given idempotency key.
#[derive(Serialize, Deserialize)]
pub struct CachedResponse {
    status: u16,
    body: String,
}

/// Extracts `Idempotency-Key` from the request headers.
/// Returns None if missing (meaning the endpoint does not require it).
pub fn extract_idempotency_key(headers: &HeaderMap) -> Option<Uuid> {
    headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// Tower middleware layer — wrap around any router that needs idempotency.
///
/// Flow:
///   1. Parse `Idempotency-Key` header. If absent → pass through normally.
///   2. Check Redis for a stored response under this key.
///   3. If found → return the cached response immediately (no DB hit).
///   4. If not found → run the handler, buffer the response body,
///      store it in Redis with a 24-hour TTL, return.
///
/// The TTL of 24 hours is intentional: it covers any realistic retry window
/// but does not bloat Redis storage long-term.
pub async fn idempotency_middleware(req: Request<Body>, next: Next) -> Response {
    // Get Redis from the extensions
    let redis = match req.extensions().get::<Arc<Redis>>() {
        Some(r) => Arc::clone(r),
        None => return next.run(req).await,
    };

    // Extract the idempotency key from the header
    let idem_key = match extract_idempotency_key(req.headers()) {
        Some(k) => k,
        None => return next.run(req).await,
    };

    let redis_key = RedisKey::idempotency(idem_key);

    // Check if request is cached and return if it is
    if let Ok(Some(cached)) = redis.get::<CachedResponse>(&redis_key).await {
        let status = StatusCode::from_u16(cached.status).unwrap_or(StatusCode::OK);
        return (status, cached.body).into_response();
    }

    // No cache, run the handler
    let response = next.run(req).await;

    // Cache only 2xx responses. Not caching errors, those should be retried.
    if response.status().is_success() {
        let status = response.status().as_u16();
        let (parts, body) = response.into_parts();

        let bytes: bytes::Bytes = match body.collect().await {
            Ok(b) => b.to_bytes(),
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

        let body_str = String::from_utf8_lossy(&bytes).to_string();

        let cached = CachedResponse {
            body: body_str.clone(),
            status,
        };

        let redis_clone = Arc::clone(&redis);
        let key_clone = redis_key;
        tokio::spawn(async move {
            // Store in Redis for 24 hours
            let _ = redis_clone.set(&key_clone, &cached, Some(86400)).await;
        });

        Response::from_parts(parts, Body::from(bytes))
    } else {
        response
    }
}
