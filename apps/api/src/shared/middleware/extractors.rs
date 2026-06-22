use crate::shared::errors::error_response;
use axum::{
    Json,
    extract::Query as AxumQuery,
    extract::rejection::JsonRejection,
    extract::{FromRequest, Multipart, Request},
    http::StatusCode,
    http::request::Parts,
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;
use std::collections::HashMap;

// ========== EXTRACTORS ==========

/// Universal body extractor - auto-detects content type (JSON, form, multipart)
/// Use when you want to accept any content type
pub struct MenoBody<T>(pub T);

/// Strict JSON-only extractor - rejects non-JSON content types
pub struct MenoJson<T>(pub T);

/// Strict URL-encoded form extractor - rejects non-form content types
pub struct MenoForm<T>(pub T);

/// Multipart form extractor with file support - separates fields from files
pub struct MenoMultipartForm<T> {
    pub data: T,
    pub files: HashMap<String, UploadedFile>,
}

/// Represents an uploaded file from multipart form data
#[derive(Debug, Clone)]
pub struct UploadedFile {
    pub filename: String,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

impl<T, S> FromRequest<S> for MenoBody<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // JSON
        if content_type.starts_with("application/json") {
            return match Json::<T>::from_request(req, state).await {
                Ok(Json(data)) => Ok(MenoBody(data)),
                Err(rejection) => Err(json_rejection_to_response(rejection)),
            };
        }

        // URL-encoded form
        if content_type.starts_with("application/x-www-form-urlencoded") {
            return match axum::Form::<T>::from_request(req, state).await {
                Ok(axum::Form(data)) => Ok(MenoBody(data)),
                Err(e) => Err(error_response(
                    StatusCode::BAD_REQUEST,
                    "FORM_PARSE_ERROR",
                    &format!("Failed to parse form data: {}", e),
                )),
            };
        }

        // Multipart form (with potential files)
        if content_type.starts_with("multipart/form-data") {
            return extract_multipart_to_body::<T, S>(req, state).await;
        }

        Err(error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "UNSUPPORTED_CONTENT_TYPE",
            "Content-Type must be application/json, application/x-www-form-urlencoded, or multipart/form-data",
        ))
    }
}

impl<T, S> FromRequest<S> for MenoJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !content_type.starts_with("application/json") {
            return Err(error_response(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "INVALID_CONTENT_TYPE",
                "Content-Type must be application/json",
            ));
        }

        match Json::<T>::from_request(req, state).await {
            Ok(Json(data)) => Ok(MenoJson(data)),
            Err(rejection) => Err(json_rejection_to_response(rejection)),
        }
    }
}

impl<T, S> FromRequest<S> for MenoForm<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // URL-encoded form
        if content_type.starts_with("application/x-www-form-urlencoded") {
            match axum::Form::<T>::from_request(req, state).await {
                Ok(axum::Form(data)) => Ok(MenoForm(data)),
                Err(e) => Err(error_response(
                    StatusCode::BAD_REQUEST,
                    "FORM_PARSE_ERROR",
                    &format!("Failed to parse form data: {}", e),
                )),
            }
        }
        // Also support multipart/form-data for form fields (without files)
        else if content_type.starts_with("multipart/form-data") {
            extract_multipart_to_form::<T, S>(req, state).await
        } else {
            Err(error_response(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "INVALID_CONTENT_TYPE",
                "Content-Type must be application/x-www-form-urlencoded or multipart/form-data",
            ))
        }
    }
}

impl<T, S> FromRequest<S> for MenoMultipartForm<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        if !content_type.starts_with("multipart/form-data") {
            return Err(error_response(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "INVALID_CONTENT_TYPE",
                "Content-Type must be multipart/form-data",
            ));
        }

        let mut multipart = Multipart::from_request(req, state).await.map_err(|e| {
            error_response(StatusCode::BAD_REQUEST, "MULTIPART_ERROR", &e.to_string())
        })?;

        let mut form_data = serde_json::Map::new();
        let mut files = HashMap::new();

        while let Some(field) = multipart.next_field().await.map_err(|e| {
            error_response(StatusCode::BAD_REQUEST, "FORM_FIELD_ERROR", &e.to_string())
        })? {
            let name = field.name().unwrap_or_default().to_string();

            // Check if this is a file field
            if field.file_name().is_some() {
                // Extract ALL data BEFORE consuming the field
                let filename = field.file_name().unwrap_or_default().to_string();
                let content_type = field.content_type().map(|ct| ct.to_string());

                // Now consume the field to get bytes
                let bytes = field.bytes().await.map_err(|e| {
                    error_response(StatusCode::BAD_REQUEST, "FILE_READ_ERROR", &e.to_string())
                })?;

                files.insert(
                    name,
                    UploadedFile {
                        filename,
                        content_type,
                        bytes: bytes.to_vec(),
                    },
                );
            } else {
                // Regular text field - doesn't consume the field
                let text = field.text().await.map_err(|e| {
                    error_response(StatusCode::BAD_REQUEST, "FORM_FIELD_ERROR", &e.to_string())
                })?;
                form_data.insert(name, serde_json::Value::String(text));
            }
        }

        let data: T =
            serde_json::from_value(serde_json::Value::Object(form_data)).map_err(|e| {
                error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    &format!("Failed to parse form data: {}", e),
                )
            })?;

        Ok(MenoMultipartForm { data, files })
    }
}

// ========== HELPER FUNCTIONS ==========

/// Extract multipart data into MenoBody (skip files)
async fn extract_multipart_to_body<T: DeserializeOwned, S: Send + Sync>(
    req: Request,
    state: &S,
) -> Result<MenoBody<T>, Response> {
    let mut multipart = Multipart::from_request(req, state)
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, "MULTIPART_ERROR", &e.to_string()))?;

    let mut form_data = serde_json::Map::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, "FORM_FIELD_ERROR", &e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();

        // Skip file fields for MenoBody
        if field.file_name().is_some() {
            continue;
        }

        let text = field.text().await.map_err(|e| {
            error_response(StatusCode::BAD_REQUEST, "FORM_FIELD_ERROR", &e.to_string())
        })?;
        form_data.insert(name, serde_json::Value::String(text));
    }

    let data: T = serde_json::from_value(serde_json::Value::Object(form_data)).map_err(|e| {
        error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            &format!("Failed to parse form data: {}", e),
        )
    })?;

    Ok(MenoBody(data))
}

/// Extract multipart data into MenoForm (convert files to text representation)
async fn extract_multipart_to_form<T: DeserializeOwned, S: Send + Sync>(
    req: Request,
    state: &S,
) -> Result<MenoForm<T>, Response> {
    let mut multipart = Multipart::from_request(req, state)
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, "MULTIPART_ERROR", &e.to_string()))?;

    let mut form_data = serde_json::Map::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, "FORM_FIELD_ERROR", &e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();

        // For MenoForm, convert files to JSON object with metadata
        if let Some(filename) = field.file_name() {
            let file_obj = serde_json::json!({
                "filename": filename,
                "content_type": field.content_type(),
                "size": field.bytes().await.unwrap_or_default().len(),
            });
            form_data.insert(name, file_obj);
        } else {
            let text = field.text().await.map_err(|e| {
                error_response(StatusCode::BAD_REQUEST, "FORM_FIELD_ERROR", &e.to_string())
            })?;
            form_data.insert(name, serde_json::Value::String(text));
        }
    }

    let data: T = serde_json::from_value(serde_json::Value::Object(form_data)).map_err(|e| {
        error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            &format!("Failed to parse form data: {}", e),
        )
    })?;

    Ok(MenoForm(data))
}

// ========== ERROR HANDLING ==========

fn make_friendly_json_error(e: &axum::extract::rejection::JsonDataError) -> String {
    let error_text = e.body_text();

    if let Some(field) = extract_missing_field(&error_text) {
        return format!("The '{}' field is required", field);
    }

    if error_text.contains("invalid type") || error_text.contains("unknown variant") {
        return "Invalid value provided for one or more fields".to_string();
    }

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

fn json_rejection_to_response(rejection: JsonRejection) -> Response {
    let (status, code, message) = match rejection {
        JsonRejection::MissingJsonContentType(_) => (
            StatusCode::BAD_REQUEST,
            "INVALID_CONTENT_TYPE".to_string(),
            "Content-Type must be application/json".to_string(),
        ),
        JsonRejection::JsonSyntaxError(_) => (
            StatusCode::BAD_REQUEST,
            "JSON_SYNTAX_ERROR".to_string(),
            "Malformed JSON in request body".to_string(),
        ),
        JsonRejection::JsonDataError(e) => {
            let msg = make_friendly_json_error(&e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_ERROR".to_string(),
                msg,
            )
        }
        _ => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR".to_string(),
            "Invalid request body".to_string(),
        ),
    };

    error_response(status, code.as_str(), message.as_str())
}

// ========== CUSTOM MEN QUERY REJECTION ==========
/// Drop-in replacement for axum's `Query<T>` that converts deserialisation
/// errors into the standard `MenoResponse` error JSON instead of a raw 422.
pub struct MenoQuery<T>(pub T);

impl<T, S> axum::extract::FromRequestParts<S> for MenoQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = MenoQueryRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        AxumQuery::<T>::from_request_parts(parts, state)
            .await
            .map(|q| MenoQuery(q.0))
            .map_err(|e| MenoQueryRejection(e.to_string()))
    }
}

pub struct MenoQueryRejection(String);

impl IntoResponse for MenoQueryRejection {
    fn into_response(self) -> Response {
        // Extract the useful part of the serde message
        let msg = self
            .0
            .trim_start_matches("Failed to deserialize query string: ")
            .to_string();

        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status_code": 400,
                "code":    "INVALID_QUERY_PARAMS",
                "message": msg,
                "error":   "Bad Request",
            })),
        )
            .into_response()
    }
}
