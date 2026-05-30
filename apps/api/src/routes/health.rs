use crate::state::MenoState;
use axum::extract::State;
use axum::{Json, http::StatusCode, response::IntoResponse};
use fred::prelude::ClientLike;
use serde_json::json;
use std::sync::Arc;

pub async fn health_handler(State(state): State<Arc<MenoState>>) -> impl IntoResponse {
    let mut status = "healthy";
    let mut checks: Vec<serde_json::Value> = Vec::new();

    // Check Database
    match state.db.acquire().await {
        Ok(_) => {
            checks.push(json!({
                "name": "database",
                "status": "ok"
            }));
        }
        Err(e) => {
            status = "degraded";
            checks.push(json!({
                "name": "database",
                "status": "error",
                "message": e.to_string()
            }));
        }
    }

    // Check Redis
    match state.redis.client().ping::<()>(None).await {
        Ok(_) => {
            checks.push(json!({
                "name": "redis",
                "status": "ok"
            }));
        }
        Err(e) => {
            status = "degraded";
            checks.push(json!({
                "name": "redis",
                "status": "error",
                "message": e.to_string()
            }));
        }
    }

    let status_code = if status == "healthy" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(json!({
            "data": {
                "status": status,
                "checks": checks,
                "version": "0.1.0"
            },
            "meta": null,
        })),
    )
}
