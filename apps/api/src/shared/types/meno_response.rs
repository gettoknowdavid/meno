use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// The single response shape for every endpoint in the app.
///
/// Matches the `NestJS` contract your clients (Flutter, Next.js) are built against:
/// { code, message, status, data? }
///
/// `data` is Option<T>, so endpoints that return nothing (logout, delete, mark-read)
/// can use `MenoResponse::<()>::ok(...)` and `data` will serialize as `null`.
#[derive(Serialize)]
pub struct MenoResponse<T: Serialize> {
    pub status_code: u16,

    pub code: String,

    pub message: String,

    pub status: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: Serialize> MenoResponse<T> {
    pub fn ok(message: impl Into<String>, data: T) -> Self {
        Self {
            status_code: StatusCode::OK.as_u16(),
            code: StatusCode::OK.to_string(),
            message: message.into(),
            data: Some(data),
            status: true,
        }
    }

    pub fn created(message: impl Into<String>, data: T) -> Self {
        Self {
            status_code: StatusCode::OK.as_u16(),
            code: StatusCode::OK.to_string(),
            message: message.into(),
            data: Some(data),
            status: true,
        }
    }
}

impl MenoResponse<()> {
    /// Success with no payload — logout, delete, mark-read, leave broadcast, etc.
    /// `data` is omitted entirely from the JSON (`skip_serializing_if` = None).
    pub fn no_content(message: impl Into<String>) -> Self {
        Self {
            status_code: StatusCode::OK.as_u16(),
            code: StatusCode::OK.to_string(),
            message: message.into(),
            data: None,
            status: true,
        }
    }
}

impl<T: Serialize> IntoResponse for MenoResponse<T> {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(self),
        )
            .into_response()
    }
}
