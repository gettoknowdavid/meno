pub async fn health_handler() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::OK,
        axum::Json(serde_json::json!({
            "data": {
                "status": "ok",
                "version": "0.1.0"
            },
            "meta": null,
            "error": null
        })),
    )
}
