use crate::shared::errors::error_response;
use crate::state::MenoState;
use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::Response;
use std::sync::Arc;

pub struct MenoJson<T>(pub T);
impl<T> axum::extract::FromRequest<Arc<MenoState>> for MenoJson<T>
where
    T: serde::de::DeserializeOwned,
{
    type Rejection = Response;

    async fn from_request(
        req: axum::extract::Request,
        state: &Arc<MenoState>,
    ) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(val)) => Ok(MenoJson(val)),
            Err(rejection) => {
                let message = match &rejection {
                    JsonRejection::MissingJsonContentType(_) => {
                        "Content-Type must be application/json".to_string()
                    }
                    JsonRejection::JsonDataError(e) => {
                        format!("Invalid request body: {}", e.body_text())
                    }
                    JsonRejection::JsonSyntaxError(e) => {
                        format!("Malformed JSON: {}", e.body_text())
                    }
                    _ => "Invalid request body".to_string(),
                };
                Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    &message,
                ))
            }
        }
    }
}
