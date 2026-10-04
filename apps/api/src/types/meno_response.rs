//! The single response envelope for every endpoint.
//!
//! Copied from `apps/api/src/shared/types/meno_response.rs` on `master` (`903c3ba`) with
//! three deliberate changes, all recorded below.
//!
//! # Why this lives in `apps/api`, not `crates/core`
//!
//! `crates/core` has a hard constraint: no `axum`, no `std::net`, no I/O of any kind
//! (plan §2.1). This type implements `IntoResponse`, which means `axum::Json` and
//! `axum::http::StatusCode`. It therefore cannot move to `core` without breaking that
//! crate's central property — the one that lets its logic be tested with no Docker.
//!
//! What *is* pure and does live in `core` is [`meno_core::ErrorBody`], the status/code/
//! message triple for failures. `core` cannot know how a success envelope is shaped, so
//! the split is: `core` owns the error contract, `apps/api` owns the HTTP rendering of
//! both.
//!
//! # Wire contract
//!
//! ```json
//! { "code": "OK", "message": "…", "status": true, "data": { } }
//! ```
//!
//! Four keys. `data` is omitted when there is no payload — never sent as `null`.
//! Nested payloads *inside* `data` carry their own casing, applied per struct
//! (`CursorPage` emits `nextCursor`/`hasNextPage`).

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use meno_core::error::Meta;
use serde::Serialize;

/// Stable code for a 200 response.
pub const CODE_OK: &str = "OK";

/// Stable code for a 201 response.
pub const CODE_CREATED: &str = "CREATED";

/// The single response shape for every endpoint in the app.
///
/// Matches the `NestJS` contract your clients (Flutter, Next.js) are built against:
/// `{ code, message, status, data? }`
///
/// `data` is `Option<T>`, so endpoints that return nothing (logout, delete, mark-read)
/// can use [`MenoResponse::no_content`] and `data` will be omitted from the JSON.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenoResponse<T: Serialize> {
    /// HTTP status for the response line. Never serialised.
    ///
    /// # Why this is not on the wire
    ///
    /// The status already travels in the HTTP status line, so repeating it in the body
    /// is pure redundancy. Stripe, GitHub and Google AIP-193 all omit it and send only
    /// a stable `code` string plus a message. This field exists solely so
    /// [`IntoResponse`] can set the status line; `#[serde(skip)]` keeps it off the wire.
    #[serde(skip)]
    pub http_status: u16,

    /// Stable, machine-readable code. This is what clients branch on.
    ///
    /// # Code stability
    ///
    /// On `master` the success paths set this to `StatusCode::OK.to_string()` — the
    /// string `"200 OK"` — while the error paths already used stable codes like
    /// `"BAD_REQUEST"`. So one field carried two conventions depending on whether the
    /// request succeeded, and a client could not branch on it uniformly.
    ///
    /// Plan §4.2 requires *"a stable machine-readable `code` so the Flutter and Next.js
    /// clients never string-match on human-readable messages"*, so success uses `"OK"`
    /// and `"CREATED"`. A code may not be renamed or removed without an API version bump.
    pub code: String,

    /// Human-readable message. May be reworded at any time; clients must not match it.
    pub message: String,

    /// `true` on success. Always `false` in [`meno_core::ErrorBody`].
    pub status: bool,

    /// The payload. Omitted entirely from the JSON when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,

    /// Correlation metadata. Omitted when the transport layer supplies none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl<T: Serialize> MenoResponse<T> {
    /// 200 with a payload.
    #[must_use]
    pub fn ok(message: impl Into<String>, data: T) -> Self {
        Self {
            http_status: StatusCode::OK.as_u16(),
            code: CODE_OK.to_string(),
            message: message.into(),
            status: true,
            data: Some(data),
            meta: None,
        }
    }

    /// 201 with the created resource as the payload.
    #[must_use]
    pub fn created(message: impl Into<String>, data: T) -> Self {
        Self {
            http_status: StatusCode::CREATED.as_u16(),
            code: CODE_CREATED.to_string(),
            message: message.into(),
            status: true,
            data: Some(data),
            meta: None,
        }
    }

    /// 200 with an arbitrary status — used when a handler needs a non-standard 2xx,
    /// such as `202 Accepted` for work that continues in the background.
    #[must_use]
    pub fn with_status(
        http_status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
        data: T,
    ) -> Self {
        Self {
            http_status: http_status.as_u16(),
            code: code.to_string(),
            message: message.into(),
            status: true,
            data: Some(data),
            meta: None,
        }
    }

    /// Attach correlation metadata — the request id, so a user reporting a failure
    /// gives you something to grep the logs for.
    ///
    /// The constructors cannot fill this in themselves: only the transport layer knows
    /// the request context. Kept consistent with [`meno_core::ErrorBody::with_meta`], so
    /// a handler that decorates both envelopes does it the same way for each.
    #[must_use]
    pub fn with_meta(mut self, meta: Meta) -> Self {
        self.meta = Some(meta);
        self
    }

    /// Serialise to a WebSocket text frame.
    ///
    /// # WebSocket use
    ///
    /// Yes — this envelope can be sent over a socket, and after the `http_status` field
    /// became `#[serde(skip)]` nothing in the serialised form is HTTP-specific. It is
    /// already a plain `Serialize`, so this is a convenience, not a requirement:
    ///
    /// ```ignore
    /// let frame = Message::Text(response.into_frame().into());
    /// ```
    ///
    /// **Caveat.** Prefer this for *responses* to a client command sent over the socket
    /// (an ACK, a result, a rejection), where `status`/`code` mean what they mean over
    /// HTTP. For **server-pushed events**, `infrastructure/ws::WsPayload` is the better
    /// frame: it carries an `event` discriminant plus opaque `data`, which is what a
    /// client switches on for routing. Reusing this envelope for pushed events would
    /// force every broadcast to be shaped like a successful request-response.
    #[must_use]
    pub fn into_frame(self) -> String {
        // Serialising this type cannot fail: every field is a string, a bool, a u16 or
        // an already-`Serialize` payload.
        serde_json::to_string(&self).unwrap_or_else(|_| {
            format!(
                r#"{{"code":"{}","message":"Response serialisation failed","status":false}}"#,
                CODE_OK
            )
        })
    }
}

impl MenoResponse<()> {
    /// Success with no payload — logout, delete, mark-read, leave broadcast, etc.
    /// `data` is omitted entirely from the JSON (`skip_serializing_if` = `None`).
    #[must_use]
    pub fn no_content(message: impl Into<String>) -> Self {
        Self {
            http_status: StatusCode::OK.as_u16(),
            code: CODE_OK.to_string(),
            message: message.into(),
            status: true,
            data: None,
            meta: None,
        }
    }
}

impl<T: Serialize> IntoResponse for MenoResponse<T> {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.http_status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(self),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the success envelope.
    //!
    //! §4.2 calls this "a prerequisite for both frontends", so the wire shape is a
    //! contract: these assert it key by key rather than round-tripping to a struct and
    //! hoping.

    use super::*;

    fn json_of<T: Serialize>(response: &MenoResponse<T>) -> serde_json::Value {
        serde_json::to_value(response).expect("this type always serialises")
    }

    fn keys_of(value: &serde_json::Value) -> Vec<&str> {
        value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn a_success_response_carries_exactly_the_four_contract_keys() {
        let json = json_of(&MenoResponse::ok("saved", serde_json::json!({"id": 1})));

        let mut keys = keys_of(&json);
        keys.sort_unstable();
        assert_eq!(keys, ["code", "data", "message", "status"]);
    }

    #[test]
    fn the_http_status_is_never_serialised() {
        // §4.2's whole point: the status travels in the status line, not the body.
        // Repeating it is the redundancy Stripe and GitHub decline to ship.
        let json = json_of(&MenoResponse::ok("saved", 1));

        assert!(
            !json
                .as_object()
                .expect("an object")
                .contains_key("http_status"),
            "the body must not repeat the status line: {json}"
        );
    }

    #[test]
    fn success_uses_a_stable_code_not_the_status_text() {
        // The regression §4.2 is about: `master` sent `"200 OK"` here while error paths
        // sent `"BAD_REQUEST"`, so one field carried two conventions.
        assert_eq!(json_of(&MenoResponse::ok("x", 1))["code"], CODE_OK);
        assert_eq!(
            json_of(&MenoResponse::created("x", 1))["code"],
            CODE_CREATED
        );
        assert_eq!(json_of(&MenoResponse::no_content("x"))["code"], CODE_OK);
    }

    #[test]
    fn the_code_is_a_word_not_a_number() {
        // A client branching on `code` must never see it change because a status moved.
        assert_eq!(CODE_OK, "OK");
        assert_eq!(CODE_CREATED, "CREATED");
        assert!(!CODE_OK.chars().any(char::is_numeric));
    }

    #[test]
    fn status_is_always_true_on_success() {
        assert_eq!(json_of(&MenoResponse::ok("x", 1))["status"], true);
        assert_eq!(json_of(&MenoResponse::no_content("x"))["status"], true);
        assert_eq!(json_of(&MenoResponse::created("x", 1))["status"], true);
    }

    #[test]
    fn an_absent_payload_is_omitted_never_sent_as_null() {
        // `data` being `null` vs absent is a real difference to a Flutter client decoding
        // into `T?`, and the contract says omitted.
        let json = json_of(&MenoResponse::no_content("done"));

        assert!(
            !json.as_object().expect("an object").contains_key("data"),
            "`data` must be absent, not null: {json}"
        );
    }

    #[test]
    fn a_present_payload_is_included() {
        assert_eq!(
            json_of(&MenoResponse::ok("x", serde_json::json!({"id": 7})))["data"],
            serde_json::json!({"id": 7})
        );
    }

    #[test]
    fn meta_is_omitted_until_the_transport_layer_supplies_it() {
        let bare = json_of(&MenoResponse::ok("x", 1));
        assert!(!bare.as_object().expect("an object").contains_key("meta"));

        let decorated = MenoResponse::ok("x", 1).with_meta(Meta {
            request_id: Some("req-1".to_owned()),
            api_version: Some("0.2.0".to_owned()),
        });
        assert_eq!(json_of(&decorated)["meta"]["requestId"], "req-1");
    }

    #[test]
    fn with_status_sets_the_line_but_keeps_the_code_explicit() {
        // For `202 Accepted` and friends: the status moves, the code does not change
        // unless the caller says so.
        let json = json_of(&MenoResponse::with_status(
            StatusCode::ACCEPTED,
            "QUEUED",
            "working on it",
            1,
        ));

        assert_eq!(json["code"], "QUEUED");
        assert_eq!(json["status"], true);
    }

    #[test]
    fn into_response_sets_the_status_line() {
        let response = MenoResponse::created("made", 1).into_response();

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
    }

    #[test]
    fn a_no_content_response_is_a_200() {
        assert_eq!(
            MenoResponse::no_content("deleted").into_response().status(),
            StatusCode::OK
        );
    }

    #[test]
    fn into_frame_produces_the_same_json_as_the_body() {
        // A WebSocket frame must not diverge from the HTTP body, so it goes through the
        // same serialiser rather than a hand-written template.
        let response = MenoResponse::ok("saved", serde_json::json!({"id": 1}));
        let expected = json_of(&response);
        let frame: serde_json::Value =
            serde_json::from_str(&response.into_frame()).expect("a frame is JSON");

        assert_eq!(frame, expected);
    }

    #[test]
    fn an_error_body_and_a_success_body_agree_on_the_failure_status_flag() {
        // The two envelopes are documented as a pair; `status` must mean the same thing in
        // each, because a client branches on it.
        let error = meno_core::to_body(&meno_core::Error::NotFound {
            resource: "broadcast",
            code: meno_core::ErrorCode::NotFound,
        });

        assert!(!error.status);
        assert!(json_of(&MenoResponse::no_content("x"))["status"] == serde_json::Value::Bool(true));
    }
}
