use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = axum::Json(serde_json::json!({
        "data": null,
        "meta": null,
        "error": {
            "code": code,
            "message": message,
        }
    }));
    (status, body).into_response()
}
