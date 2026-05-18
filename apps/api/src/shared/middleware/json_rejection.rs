use crate::shared::errors::error_response;
use crate::state::MenoState;
use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use std::sync::Arc;

pub struct MenoJson<T>(pub T);

impl<T> axum::extract::FromRequest<Arc<MenoState>> for MenoJson<T>
where
    T: serde::de::DeserializeOwned,
{
    type Rejection = axum::response::Response;

    async fn from_request(
        req: axum::extract::Request,
        state: &Arc<MenoState>,
    ) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(val)) => Ok(MenoJson(val)),

            Err(rejection) => {
                let (status, code, message) = match &rejection {
                    JsonRejection::MissingJsonContentType(_) => (
                        StatusCode::BAD_REQUEST,
                        "INVALID_CONTENT_TYPE",
                        "Content-Type must be application/json".to_string(),
                    ),
                    JsonRejection::JsonSyntaxError(_) => (
                        StatusCode::BAD_REQUEST,
                        "JSON_SYNTAX_ERROR",
                        "Malformed JSON in request body".to_string(),
                    ),
                    JsonRejection::JsonDataError(e) => {
                        let message = make_friendly_json_error(e);
                        (
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "VALIDATION_ERROR",
                            message,
                        )
                    }
                    _ => (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "VALIDATION_ERROR",
                        "Invalid request body".to_string(),
                    ),
                };

                Err(error_response(status, code, &message))
            }
        }
    }
}

fn make_friendly_json_error(e: &axum::extract::rejection::JsonDataError) -> String {
    let error_text = e.body_text();

    if error_text.contains("missing field") {
        if let Some(field) = extract_missing_field(&error_text) {
            return format!("The '{}' field is required.", field);
        }
    }

    if error_text.contains("invalid type") || error_text.contains("unknown variant") {
        return "Invalid value provided for one or more fields.".to_string();
    }

    // Fallback to a cleaned version of the original message
    error_text
        .replace(
            "Failed to deserialize the JSON body into the target type: ",
            "",
        )
        .trim()
        .to_string()
}

fn extract_missing_field(error_text: &str) -> Option<String> {
    error_text
        .split("missing field `")
        .nth(1)
        .and_then(|s| s.split('`').next())
        .map(|s| s.to_string())
}
