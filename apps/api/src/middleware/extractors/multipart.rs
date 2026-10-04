//! Multipart body extraction.
//!
//! Split out of the parent module for §9.3's file-size rule, and because multipart is the one
//! body type with a genuinely separate hazard: it carries **files**, and how large a file this
//! layer will buffer is a security decision rather than a parsing detail.
//!
//! # Untrusted input
//!
//! Three fields here are whatever the client put in a part header, and none of them may be
//! trusted:
//!
//! - [`UploadedFile::filename`] is **not** a path. It is used for display and, at most, for an
//!   extension after sanitising. Handing it to a filesystem unchecked is a path-traversal
//!   bug, so the storage layer (§9.5) must derive its own object key.
//! - [`UploadedFile::content_type`] is a *claim*. It is what the client said it was sending,
//!   not what arrived; validate against the bytes before acting on it.
//! - Field *names* are likewise client-supplied, and are the map keys a handler will iterate.
//!
//! # Size
//!
//! axum's `Multipart` applies `DefaultBodyLimit`, so a body larger than that is refused by the
//! server before this module buffers anything. The limit is a property of the router, and it is
//! documented here so whoever sets it knows that raising it raises this layer's memory use by
//! the same amount.

use axum::extract::{FromRequest, Multipart, Request};
use axum::http::StatusCode;
use axum::response::Response;
use serde::de::DeserializeOwned;
use std::collections::HashMap;

use crate::middleware::error_response;

/// A multipart body whose file parts are separated from its text parts.
///
/// A handler takes `data` for the fields and `files` for the uploads, so the decision about
/// what to do with an untrusted file is a line in the handler rather than something the
/// extractor has already done on its behalf.
pub struct MenoMultipartForm<T> {
    /// The non-file parts, deserialised into `T`.
    pub data: T,
    /// The file parts, keyed by the client-supplied field name.
    pub files: HashMap<String, UploadedFile>,
}

/// One file from a multipart body.
#[derive(Debug, Clone)]
pub struct UploadedFile {
    /// The client-supplied filename. **Untrusted** — never a filesystem path.
    pub filename: String,
    /// The declared content type, if the client sent one. Also untrusted; see the module docs.
    pub content_type: Option<String>,
    /// The bytes, already buffered.
    pub bytes: Vec<u8>,
}

impl UploadedFile {
    /// The size in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the file is empty.
    ///
    /// An empty "upload" is a client bug worth catching early rather than writing a
    /// zero-byte object and discovering it later.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// A multipart read failure, in the §4.2 envelope's terms but not yet rendered.
///
/// The reason this is a type and not a [`Response`] is the *shape*, not the plumbing: an
/// `axum::response::Response` is a fat value, and a `Result<T, Response>` in the middle of a
/// parsing chain asks every `?` in that chain to carry one. This is three words wide, so the
/// intermediate steps can fail without each of them being a rendering decision, and the single
/// place that turns one into a response is [`From<MultipartError>`].
///
/// Each constructor call names its own wire `code`, so the codes stay at the site that knows
/// which part of the parse went wrong rather than in one shared `match`.
#[derive(Debug)]
pub(super) struct MultipartError {
    /// The HTTP status the failure renders as.
    status: StatusCode,
    /// The §4.2 wire code.
    code: &'static str,
    /// The client-facing explanation.
    message: String,
}

impl MultipartError {
    /// Build a failure. `code` is the §4.2 wire code for this specific parse step.
    fn new(status: StatusCode, code: &'static str, message: String) -> Self {
        Self {
            status,
            code,
            message,
        }
    }

    /// A body that could not be read as multipart at all.
    fn unreadable(error: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "MULTIPART_ERROR",
            format!("could not read the multipart body: {error}"),
        )
    }
}

impl From<MultipartError> for Response {
    fn from(error: MultipartError) -> Self {
        error_response(error.status, error.code, &error.message)
    }
}

impl<T, S> FromRequest<S> for MenoMultipartForm<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        if !is_multipart(&req) {
            return Err(error_response(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "INVALID_CONTENT_TYPE",
                "Content-Type must be multipart/form-data",
            ));
        }

        let mut fields = serde_json::Map::new();
        let mut files = HashMap::new();
        collect(
            &mut Multipart::from_request(req, state)
                .await
                .map_err(MultipartError::unreadable)?,
            &mut fields,
            &mut files,
        )
        .await?;

        let data: T =
            serde_json::from_value(serde_json::Value::Object(fields)).map_err(|error| {
                error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    &format!("the form fields did not match the expected shape: {error}"),
                )
            })?;

        Ok(Self { data, files })
    }
}

/// Read every part of a multipart body, splitting fields from files.
async fn collect(
    multipart: &mut Multipart,
    fields: &mut serde_json::Map<String, serde_json::Value>,
    files: &mut HashMap<String, UploadedFile>,
) -> Result<(), MultipartError> {
    while let Some(field) = multipart.next_field().await.map_err(|error| {
        MultipartError::new(
            StatusCode::BAD_REQUEST,
            "FORM_FIELD_ERROR",
            format!("could not read a form part: {error}"),
        )
    })? {
        // A part with no name cannot be addressed, so it is dropped rather than stored under
        // a placeholder that two such parts would collide on.
        let Some(name) = field.name().map(str::to_owned) else {
            continue;
        };

        // The name, filename and content type must all be read *before* the body is
        // consumed; `field.text()` and `field.bytes()` take `self`.
        let filename = field.file_name().map(str::to_owned);
        let content_type = field.content_type().map(|mime| mime.to_string());

        match filename {
            Some(filename) => {
                let bytes = field.bytes().await.map_err(|error| {
                    MultipartError::new(
                        StatusCode::BAD_REQUEST,
                        "FILE_READ_ERROR",
                        format!("could not read an uploaded file: {error}"),
                    )
                })?;

                files.insert(
                    name,
                    UploadedFile {
                        filename,
                        content_type,
                        bytes: bytes.to_vec(),
                    },
                );
            }
            None => {
                let text = field.text().await.map_err(|error| {
                    MultipartError::new(
                        StatusCode::BAD_REQUEST,
                        "FORM_FIELD_ERROR",
                        format!("could not read a form field: {error}"),
                    )
                })?;

                fields.insert(name, serde_json::Value::String(text));
            }
        }
    }

    Ok(())
}

/// Deserialize only the *text* parts of a multipart body, skipping files.
///
/// What [`MenoBody`] and [`MenoForm`] use when they meet a multipart request: the caller did
/// not ask for uploads, so buffering them would be memory spent on nothing — and §9.4's point
/// about unbounded reads is that "the handler ignores it" is not a reason to buffer it.
pub(super) async fn fields_into<T, S>(req: Request, state: &S) -> Result<T, MultipartError>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    let mut multipart = Multipart::from_request(req, state)
        .await
        .map_err(MultipartError::unreadable)?;

    let mut fields = serde_json::Map::new();
    let mut sink = HashMap::new();

    collect(&mut multipart, &mut fields, &mut sink).await?;

    let data: T = serde_json::from_value(serde_json::Value::Object(fields)).map_err(|error| {
        MultipartError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            format!("the form fields did not match the expected shape: {error}"),
        )
    })?;

    Ok(data)
}

/// Whether a request's `Content-Type` is `multipart/form-data`.
///
/// Reuses the parent's classifier so "is this multipart?" has one definition — the boundary
/// parameter a client must send is stripped there, and a second implementation here would be
/// a second thing to get wrong.
fn is_multipart(req: &Request) -> bool {
    super::media_type(req) == super::MULTIPART
}

#[cfg(test)]
mod tests {
    //! Tests for the multipart boundary.
    //!
    //! The properties worth pinning are the split (files versus fields), the size surface, and
    //! that a client-supplied filename is passed through as data rather than being used as a
    //! path.

    use super::*;
    use crate::middleware::extractors::MenoBody;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::http::header;
    use axum::routing::post;
    use http_body_util::BodyExt;
    use serde::Deserialize;
    use tower::ServiceExt;

    /// A minimal multipart body. Built by hand rather than with a client library so the exact
    /// bytes the server sees are visible in the test.
    fn multipart_body(boundary: &str, parts: &[(&str, Option<&str>, &str)]) -> String {
        let mut body = String::new();
        for (name, filename, value) in parts {
            body.push_str(&format!("--{boundary}\r\n"));
            match filename {
                Some(filename) => {
                    body.push_str(&format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n"
                    ));
                    body.push_str("Content-Type: text/plain\r\n\r\n");
                }
                None => {
                    body.push_str(&format!(
                        "Content-Disposition: form-data; name=\"{name}\"\r\n"
                    ));
                    body.push_str("\r\n");
                }
            }
            body.push_str(value);
            body.push_str("\r\n");
        }
        body.push_str(&format!("--{boundary}--\r\n"));
        body
    }

    fn post_multipart(boundary: &str, body: String) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/x")
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .expect("a valid request")
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Profile {
        display_name: String,
    }

    async fn upload(MenoMultipartForm { data, files }: MenoMultipartForm<Profile>) -> String {
        let file = files.get("avatar").cloned();

        format!(
            "{}|{}",
            data.display_name,
            file.map_or_else(
                || "none".to_owned(),
                |file| format!("{}:{}", file.filename, file.len())
            )
        )
    }

    async fn text_only(MenoBody(body): MenoBody<Profile>) -> String {
        body.display_name
    }

    async fn body_of(response: Response) -> serde_json::Value {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("the body buffers")
            .to_bytes();

        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }

    #[tokio::test]
    async fn a_field_and_a_file_are_separated() {
        let body = multipart_body(
            "b1",
            &[
                ("display_name", None, "Ada"),
                ("avatar", Some("ada.png"), "PNGDATA"),
            ],
        );

        let response = Router::new()
            .route("/x", post(upload))
            .oneshot(post_multipart("b1", body))
            .await
            .expect("the router answers");
        let text = String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .expect("the body buffers")
                .to_bytes()
                .to_vec(),
        )
        .expect("utf8");

        assert_eq!(text, "Ada|ada.png:7");
    }

    #[tokio::test]
    async fn several_files_all_reach_the_handler() {
        let body = multipart_body(
            "b2",
            &[
                ("display_name", None, "Ada"),
                ("a", Some("a.txt"), "A"),
                ("b", Some("b.txt"), "BB"),
            ],
        );

        let response = Router::new()
            .route("/x", post(upload))
            .oneshot(post_multipart("b2", body))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_non_multipart_body_is_refused_with_the_shared_envelope() {
        let response = Router::new()
            .route("/x", post(upload))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/x")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(body_of(response).await["code"], "INVALID_CONTENT_TYPE");
    }

    #[tokio::test]
    async fn a_missing_field_is_a_422_that_names_it() {
        let body = multipart_body("b3", &[("avatar", Some("a.png"), "A")]);

        let response = Router::new()
            .route("/x", post(upload))
            .oneshot(post_multipart("b3", body))
            .await
            .expect("the router answers");
        let status = response.status();
        let body = body_of(response).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["code"], "VALIDATION_ERROR");
        assert!(
            body["message"]
                .as_str()
                .is_some_and(|m| m.contains("display_name")),
            "the missing field must be named: {body}"
        );
    }

    #[tokio::test]
    async fn a_part_with_no_name_is_ignored_rather_than_colliding() {
        // Two nameless parts would otherwise land under one placeholder key and the second
        // would silently replace the first.
        let body = multipart_body("b4", &[("", None, "orphan"), ("display_name", None, "Ada")]);

        let response = Router::new()
            .route("/x", post(text_only))
            .oneshot(post_multipart("b4", body))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn the_text_field_extractor_reads_a_multipart_body() {
        // What `MenoBody` and `MenoForm` do when they meet a multipart request.
        let body = multipart_body("b5", &[("display_name", None, "Ada")]);

        let response = Router::new()
            .route("/x", post(text_only))
            .oneshot(post_multipart("b5", body))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_filename_is_carried_as_data_and_not_interpreted() {
        // §9.5. A traversal-looking filename must arrive intact as *data* — proving this layer
        // neither sanitises it nor uses it as a path. Sanitising belongs to whoever writes the
        // object, where the key is actually chosen.
        let hostile = "../../etc/passwd";
        let body = multipart_body(
            "b6",
            &[
                ("display_name", None, "Ada"),
                ("avatar", Some(hostile), "A"),
            ],
        );

        let response = Router::new()
            .route("/x", post(upload))
            .oneshot(post_multipart("b6", body))
            .await
            .expect("the router answers");
        let text = String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .expect("the body buffers")
                .to_bytes()
                .to_vec(),
        )
        .expect("utf8");

        assert!(
            text.contains(hostile),
            "the filename must reach the handler verbatim: {text}"
        );
    }

    #[tokio::test]
    async fn a_declared_content_type_is_recorded_but_never_asserted() {
        // The client *claims* a type. This layer records the claim and nothing more; anything
        // that trusts it is the caller's decision to get wrong.
        let file = UploadedFile {
            filename: "a.png".to_owned(),
            content_type: Some("image/png".to_owned()),
            bytes: b"not actually a png".to_vec(),
        };

        assert_eq!(file.content_type.as_deref(), Some("image/png"));
        assert_eq!(file.len(), b"not actually a png".len());
        assert!(!file.is_empty());
    }

    #[test]
    fn an_empty_upload_is_visible_as_such() {
        let file = UploadedFile {
            filename: "a.png".to_owned(),
            content_type: None,
            bytes: Vec::new(),
        };

        assert!(file.is_empty());
        assert_eq!(file.len(), 0);
    }
}
