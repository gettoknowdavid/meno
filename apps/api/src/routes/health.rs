use crate::shared::services::livekit::circuit_breaker::CircuitState;
use crate::shared::services::redis::keys::RedisKey;
use crate::state::MenoState;
use axum::extract::State;
use axum::{Json, http::StatusCode, response::IntoResponse};
use std::sync::Arc;

pub async fn health_handler(State(app): State<Arc<MenoState>>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1").fetch_one(&app.db).await.is_ok();

    let key = RedisKey::new_raw("__health__");
    let redis_ok = app.redis.exists(&key).await.is_ok();

    let livekit_ok = app.livekit.breaker.state() != CircuitState::Open;

    let code = if db_ok && redis_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let status = if db_ok && redis_ok { "ok" } else { "degraded" };

    (
        code,
        Json(serde_json::json!({
            "status": status,
            "db": db_ok,
            "redis": redis_ok,
            "livekit": livekit_ok,
        })),
    )
}
