use crate::state::MenoState;
use axum::extract::State;
use axum::{Json, http::StatusCode, response::IntoResponse};
use std::sync::Arc;

pub async fn health_handler(State(app): State<Arc<MenoState>>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1").fetch_one(&app.db).await.is_ok();

    let redis_ok = app
        .redis
        .exists(&crate::shared::services::redis::keys::RedisKey::new_raw(
            "__health__",
        ))
        .await
        .is_ok();

    let status = if db_ok && redis_ok { "ok" } else { "degraded" };
    let code = if db_ok && redis_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        code,
        Json(serde_json::json!({
            "status": status,
            "db":     db_ok,
            "redis":  redis_ok,
            "version": "0.1.0",
        })),
    )
}
