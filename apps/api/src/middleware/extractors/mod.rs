//! Request extractors with one rejection vocabulary.
//!
//! Ported from `apps/api/src/shared/middleware/extractors.rs` on `master` (`903c3ba`).
//!
//! # Why this module exists at all
//!
//! axum's own extractors reject in axum's shapes: `Json<T>` answers a bare `text/plain`
//! for a wrong `Content-Type`, and `Query<T>` answers a `serde_urlencoded` message with no
//! `code`. §4.2 requires **one** error envelope on the whole API so Flutter and Next.js
//! branch on a stable `code` instead of parsing prose. These extractors keep axum's ergonomics
//! — same `FromRequest` bound, same handler signatures — and replace the response.
//!
//! # The split between the three
//!
//! | Extractor | Accepts | Use when |
//! | --- | --- | --- |
//! | [`MenoJson`] | `application/json` only | the endpoint takes JSON |
//! | [`MenoForm`] | url-encoded, or multipart text fields | the endpoint takes a form |
//! | [`MenoBody`] | all three, dispatched on `Content-Type` | the endpoint genuinely takes any |
//!
//! [`MenoBody`] exists because "accept whatever the client sent" is occasionally the right
//! thing, but it should be a decision. Where the content type is known, the strict extractor
//! refuses a mismatch instead of guessing: "the client sent form data to a JSON endpoint" is
//! worth surfacing, not silently accepting.
//!
//! # Validation boundary (§9.5)
//!
//! Deserialisation into `T` is the boundary. `T` should carry `validator` derives so a
//! handler receives an already-valid value — this layer does shape checking, not rules.

pub mod multipart;

use axum::extract::Query as AxumQuery;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;

use crate::middleware::error_response;

pub use multipart::{MenoMultipartForm, UploadedFile};

/// `Content-Type` of a JSON request, with any parameters (`; charset=utf-8`) stripped.
const JSON: &str = "application/json";
/// `Content-Type` of a URL-encoded form.
const URL_ENCODED: &str = "application/x-www-form-urlencoded";
/// `Content-Type` of a multipart body, which carries a boundary parameter.
const MULTIPART: &str = "multipart/form-data";

/// The lowercased media type of a request, with parameters removed.
///
/// Parameters are stripped because `multipart/form-data; boundary=…` and
/// `application/json; charset=utf-8` are the same media type as their bare forms, and a
/// client that omits `charset` is not sending a different thing. Comparison is on a
/// lowercased copy because media types are case-insensitive per RFC 2045 and some clients
/// send `Application/JSON`.
fn media_type(req: &Request) -> String {
    let raw = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    // `;` is the parameter separator. A media type cannot contain one, so the first
    // occurrence ends the type — no full RFC 2045 parser needed for a prefix comparison.
    raw.split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

/// Body extractor that dispatches on `Content-Type`.
///
/// Accepts JSON, a URL-encoded form, or multipart, and deserialises whichever arrived into
/// `T`. Prefer [`MenoJson`] or [`MenoForm`] where the content type is known — see the module
/// docs.
pub struct MenoBody<T>(pub T);

/// JSON-only extractor. Rejects any other `Content-Type`.
///
/// Rejection renders as the §4.2 envelope with `INVALID_CONTENT_TYPE`, not axum's raw text,
/// so a client sees one error shape everywhere.
pub struct MenoJson<T>(pub T);

/// Form extractor for url-encoded bodies, and for the text fields of a multipart body.
///
/// A multipart body is accepted because HTML forms cannot send `application/json` without
/// JavaScript, and a mobile client uploading a form alongside a file should not have to
/// choose between sending the file and sending JSON.
pub struct MenoForm<T>(pub T);

impl<T, S> FromRequest<S> for MenoBody<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match media_type(&req).as_str() {
            JSON => {
                return MenoJson::<T>::from_request(req, state)
                    .await
                    .map(|MenoJson(data)| MenoBody(data));
            }
            URL_ENCODED => {
                return axum::Form::<T>::from_request(req, state)
                    .await
                    .map(|axum::Form(data)| MenoBody(data))
                    .map_err(|error| {
                        error_response(
                            StatusCode::BAD_REQUEST,
                            "FORM_PARSE_ERROR",
                            &format!("could not parse the form body: {error}"),
                        )
                    });
            }
            MULTIPART => {
                // Files are skipped rather than buffered: a handler that wanted them uses
                // [`MenoMultipartForm`], and an unbounded upload is a memory-exhaustion
                // vector (§9.4).
                return Ok(MenoBody(multipart::fields_into(req, state).await?));
            }
            _ => {}
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
        if media_type(&req) != JSON {
            return Err(error_response(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "INVALID_CONTENT_TYPE",
                "Content-Type must be application/json",
            ));
        }

        axum::Json::<T>::from_request(req, state)
            .await
            .map(|axum::Json(data)| MenoJson(data))
            .map_err(json_rejection_to_response)
    }
}

impl<T, S> FromRequest<S> for MenoForm<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match media_type(&req).as_str() {
            URL_ENCODED => axum::Form::<T>::from_request(req, state)
                .await
                .map(|axum::Form(data)| MenoForm(data))
                .map_err(|error| {
                    error_response(
                        StatusCode::BAD_REQUEST,
                        "FORM_PARSE_ERROR",
                        &format!("could not parse the form body: {error}"),
                    )
                }),
            MULTIPART => Ok(MenoForm(multipart::fields_into(req, state).await?)),
            _ => Err(error_response(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "INVALID_CONTENT_TYPE",
                "Content-Type must be application/x-www-form-urlencoded or multipart/form-data",
            )),
        }
    }
}

/// Drop-in replacement for axum's `Query<T>` whose rejection is the §4.2 envelope.
///
/// `master` had a second, incompatible error shape here — `{statusCode, code, message,
/// error}` — so a client that handled a body rejection could not handle a query rejection
/// without a special case. That is precisely what §4.2 exists to prevent, and this type is
/// the only place it happened.
pub struct MenoQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for MenoQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = MenoQueryRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        AxumQuery::<T>::from_request_parts(parts, state)
            .await
            .map(|query| MenoQuery(query.0))
            .map_err(|error| MenoQueryRejection::new(error.to_string()))
    }
}

/// A query string that could not be deserialised into the handler's type.
///
/// Its own type rather than a bare `Response` so `?` works in a handler signature, and so the
/// failure is a named thing rather than an anonymous `Response` a caller cannot construct.
pub struct MenoQueryRejection(String);

impl MenoQueryRejection {
    /// Build a rejection from the extractor's own message.
    #[must_use]
    pub fn new(message: String) -> Self {
        // `serde_urlencoded` prefixes its message with a phrase that adds nothing over the
        // envelope's own shape, and stripping it keeps the body short and stable.
        Self(
            message
                .trim_start_matches("Failed to deserialize query string: ")
                .to_owned(),
        )
    }

    /// The message the client will see.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl IntoResponse for MenoQueryRejection {
    fn into_response(self) -> Response {
        error_response(StatusCode::BAD_REQUEST, "INVALID_QUERY_PARAMS", &self.0)
    }
}

/// Render an axum `JsonRejection` in the §4.2 envelope.
///
/// Every branch is `client-safe`: a body that failed to deserialise is the caller's to fix,
/// and the specifics help them fix it. The one thing never forwarded is the raw body echo —
/// `serde`'s message quotes the offending value, which for a password field would be the
/// password.
fn json_rejection_to_response(rejection: JsonRejection) -> Response {
    match rejection {
        JsonRejection::MissingJsonContentType(_) => error_response(
            StatusCode::BAD_REQUEST,
            "INVALID_CONTENT_TYPE",
            "Content-Type must be application/json",
        ),
        JsonRejection::JsonSyntaxError(_) => error_response(
            StatusCode::BAD_REQUEST,
            "JSON_SYNTAX_ERROR",
            "Malformed JSON in request body",
        ),
        JsonRejection::JsonDataError(error) => error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            &friendly_data_error(&error),
        ),
        // Length and encoding failures. 413 and 422 respectively, because they are the
        // statuses a client can act on, and both carry a generic message: the underlying
        // detail describes our limits, not their mistake.
        JsonRejection::BytesRejection(_) => error_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "PAYLOAD_TOO_LARGE",
            "The request body is too large",
        ),
        _ => error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "The request body could not be read",
        ),
    }
}

/// Turn a `serde` data error into something worth showing a person.
///
/// `master`'s version returned `body_text()` unchanged on the fallback path, which means a
/// `serde` message that quotes the value it choked on — and for a login body that value is
/// the password. So the messages are reconstructed from known patterns and the raw text is
/// only ever used after it has been checked for those patterns.
///
/// A missing field and a wrong type get different messages because they are different
/// mistakes, and a client can fix both without reading the `serde` output.
fn friendly_data_error(error: &axum::extract::rejection::JsonDataError) -> String {
    let text = error.body_text();

    if let Some(field) = missing_field(&text) {
        return format!("The field `{field}` is required");
    }

    if text.contains("invalid type") || text.contains("unknown variant") {
        return "One or more fields have the wrong type or an unrecognised value".to_owned();
    }

    if text.contains("invalid length") {
        return "One or more fields have the wrong number of values".to_owned();
    }

    if text.contains("unknown field") {
        return "The request body contains a field this endpoint does not accept".to_owned();
    }

    // Nothing recognised. The raw text is *not* forwarded here: an unrecognised `serde`
    // message may quote the offending value, and for a login body that value is the password.
    // The endpoint's own field names are the actionable part, and the client has them.
    "The request body does not match the shape this endpoint expects".to_owned()
}

/// Extract the field name from serde's `missing field \`x\`` message.
///
/// # Errors
///
/// Returns `None` when the message is not a missing-field error, which the caller treats as
/// "try the next pattern" rather than as a failure.
fn missing_field(text: &str) -> Option<&str> {
    text.split("missing field `")
        .nth(1)?
        .split('`')
        .next()
        .filter(|field| !field.is_empty())
}

#[cfg(test)]
mod tests {
    //! Tests for the rejection vocabulary.
    //!
    //! The properties worth pinning are the ones a client depends on: the envelope shape, the
    //! status for each kind of fault, and — the security property — that no rejected body is
    //! echoed back, because a login body contains a password.

    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::post;
    use http_body_util::BodyExt;
    use serde::Deserialize;
    use tower::ServiceExt;

    /// `deny_unknown_fields` is the mass-assignment defence (§9.5). Without it serde silently
    /// drops `{"is_admin": true}`, and a DTO that forgets to validate never sees the field at
    /// all — so a rejection test on this type proves the whole path, not just serde.
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Login {
        email: String,
        password: String,
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

    async fn json_login(MenoJson(body): MenoJson<Login>) -> String {
        body.email
    }

    async fn universal(MenoBody(body): MenoBody<Login>) -> String {
        body.email
    }

    async fn form_login(MenoForm(body): MenoForm<Login>) -> String {
        body.email
    }

    async fn query_login(MenoQuery(body): MenoQuery<Login>) -> String {
        body.email
    }

    fn post_json(body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/x")
            .header(axum::http::header::CONTENT_TYPE, JSON)
            .body(Body::from(body.to_owned()))
            .expect("a valid request")
    }

    // ── the media-type classifier ──────────────────────────────────────────

    #[test]
    fn a_content_type_with_parameters_is_still_the_same_media_type() {
        // `charset` and `boundary` are parameters, not different media types, and clients
        // omit them freely.
        for raw in [
            "application/json",
            "application/json; charset=utf-8",
            "Application/JSON",
            "  application/json  ",
        ] {
            let req = Request::builder()
                .header(axum::http::header::CONTENT_TYPE, raw)
                .body(Body::empty())
                .expect("a valid request");

            assert_eq!(media_type(&req), JSON, "`{raw}` must read as JSON");
        }
    }

    #[test]
    fn a_multipart_boundary_does_not_change_the_media_type() {
        let req = Request::builder()
            .header(
                axum::http::header::CONTENT_TYPE,
                "multipart/form-data; boundary=----abc123",
            )
            .body(Body::empty())
            .expect("a valid request");

        assert_eq!(media_type(&req), MULTIPART);
    }

    #[test]
    fn a_request_with_no_content_type_is_not_json() {
        let req = Request::builder()
            .body(Body::empty())
            .expect("a valid request");

        assert_ne!(media_type(&req), JSON);
    }

    // ── MenoJson: strictness ───────────────────────────────────────────────

    #[tokio::test]
    async fn a_well_formed_json_body_is_accepted() {
        let response = Router::new()
            .route("/x", post(json_login))
            .oneshot(post_json(
                r#"{"email":"ada@example.com","password":"hunter2"}"#,
            ))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_json_extractor_refuses_a_form_body() {
        // "The client sent form data to a JSON endpoint" is worth surfacing, not silently
        // accepting. §9.5's validation boundary starts here.
        let response = Router::new()
            .route("/x", post(json_login))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/x")
                    .header(axum::http::header::CONTENT_TYPE, URL_ENCODED)
                    .body(Body::from("email=ada%40example.com&password=hunter2"))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(body_of(response).await["code"], "INVALID_CONTENT_TYPE");
    }

    #[tokio::test]
    async fn a_json_extractor_refuses_a_missing_content_type() {
        let response = Router::new()
            .route("/x", post(json_login))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/x")
                    .body(Body::from("{}"))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn malformed_json_is_a_400_and_names_the_problem() {
        let response = Router::new()
            .route("/x", post(json_login))
            .oneshot(post_json("{not json"))
            .await
            .expect("the router answers");
        let status = response.status();
        let body = body_of(response).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "JSON_SYNTAX_ERROR");
    }

    #[tokio::test]
    async fn a_missing_field_is_a_422_that_names_the_field() {
        let response = Router::new()
            .route("/x", post(json_login))
            .oneshot(post_json(r#"{"email":"ada@example.com"}"#))
            .await
            .expect("the router answers");
        let status = response.status();
        let body = body_of(response).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["code"], "VALIDATION_ERROR");
        assert_eq!(body["message"], "The field `password` is required");
    }

    #[tokio::test]
    async fn a_wrong_type_is_a_422_that_does_not_quote_the_payload() {
        // The security property. `serde`'s own message for this is
        // `invalid type: integer \`1234\`, expected a string`, which echoes the value — and
        // for a password field that value is the password.
        let response = Router::new()
            .route("/x", post(json_login))
            .oneshot(post_json(r#"{"email":"ada@example.com","password":1234}"#))
            .await
            .expect("the router answers");
        let status = response.status();
        let body = body_of(response).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let rendered = body.to_string();
        assert!(
            !rendered.contains("1234"),
            "the offending value must not be echoed: {rendered}"
        );
    }

    #[tokio::test]
    async fn an_unrecognised_field_is_a_422() {
        let response = Router::new()
            .route("/x", post(json_login))
            .oneshot(post_json(
                r#"{"email":"ada@example.com","password":"x","is_admin":true}"#,
            ))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn a_rejection_is_the_taxonomy_envelope_and_nothing_else() {
        let response = Router::new()
            .route("/x", post(json_login))
            .oneshot(post_json("{"))
            .await
            .expect("the router answers");
        let body = body_of(response).await;

        let keys: Vec<&str> = body
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["code", "message", "status"]);
        assert_eq!(body["status"], false);
        assert!(
            body.get("data").is_none() && body.get("meta").is_none(),
            "optional fields are omitted, not nulled"
        );
    }

    // ── MenoForm ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_url_encoded_form_is_accepted_by_the_form_extractor() {
        let response = Router::new()
            .route("/x", post(form_login))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/x")
                    .header(axum::http::header::CONTENT_TYPE, URL_ENCODED)
                    .body(Body::from("email=ada%40example.com&password=hunter2"))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_form_extractor_refuses_json() {
        let response = Router::new()
            .route("/x", post(form_login))
            .oneshot(post_json(r#"{"email":"a@b.c","password":"x"}"#))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(body_of(response).await["code"], "INVALID_CONTENT_TYPE");
    }

    // ── MenoBody: dispatch ─────────────────────────────────────────────────

    #[tokio::test]
    async fn the_universal_extractor_accepts_json() {
        let response = Router::new()
            .route("/x", post(universal))
            .oneshot(post_json(r#"{"email":"ada@example.com","password":"x"}"#))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn the_universal_extractor_accepts_a_form() {
        let response = Router::new()
            .route("/x", post(universal))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/x")
                    .header(axum::http::header::CONTENT_TYPE, URL_ENCODED)
                    .body(Body::from("email=ada%40example.com&password=x"))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn the_universal_extractor_refuses_a_media_type_it_cannot_parse() {
        let response = Router::new()
            .route("/x", post(universal))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/x")
                    .header(axum::http::header::CONTENT_TYPE, "application/xml")
                    .body(Body::from("<a/>"))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(body_of(response).await["code"], "UNSUPPORTED_CONTENT_TYPE");
    }

    // ── MenoQuery ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_valid_query_is_accepted() {
        let response = Router::new()
            .route("/x", axum::routing::get(query_login))
            .oneshot(
                Request::builder()
                    .uri("/x?email=ada%40example.com&password=hunter2")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_bad_query_is_a_400_in_the_shared_envelope() {
        // §4.2's point, made concrete: `master` answered this one with
        // `{statusCode, code, message, error}`, a shape no other error used.
        let response = Router::new()
            .route("/x", axum::routing::get(query_login))
            .oneshot(
                Request::builder()
                    .uri("/x?email=not-an-email")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");
        let status = response.status();
        let body = body_of(response).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "INVALID_QUERY_PARAMS");
        let keys: Vec<&str> = body
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["code", "message", "status"]);
    }

    #[tokio::test]
    async fn a_query_rejection_strips_the_extractor_prefix() {
        // The prefix duplicates what the envelope already says.
        let rejection = MenoQueryRejection::new(
            "Failed to deserialize query string: missing field `email`".to_owned(),
        );

        assert_eq!(rejection.message(), "missing field `email`");
        assert_eq!(rejection.into_response().status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn a_query_rejection_keeps_a_message_that_has_no_prefix() {
        let rejection = MenoQueryRejection::new("something else went wrong".to_owned());

        assert_eq!(rejection.message(), "something else went wrong");
    }

    // ── the serde-message helpers ──────────────────────────────────────────

    #[test]
    fn a_missing_field_is_extracted_from_serdes_message() {
        assert_eq!(
            missing_field("missing field `email` at line 1 column 30"),
            Some("email")
        );
    }

    #[test]
    fn a_message_with_no_missing_field_yields_nothing() {
        assert_eq!(missing_field("invalid type: integer `1`"), None);
        assert_eq!(missing_field(""), None);
    }

    #[test]
    fn an_empty_field_name_is_not_extracted() {
        // A degenerate message must not produce a rejection that says "the field `` is
        // required".
        assert_eq!(missing_field("missing field `` at line 1"), None);
    }
}
