use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::time::Instant;
use tracing::info;

pub async fn timing_middleware(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let path = req.uri().path().to_string();

    let response = next.run(req).await;

    let duration = start.elapsed();
    let status = response.status();

    info!(
        duration_ms = %duration.as_millis(),
        status = %status.as_u16(),
        path = %path,
        "Request completed"
    );

    response
}
