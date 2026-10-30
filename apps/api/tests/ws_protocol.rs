//! §10 **Contract** layer: the realtime framing and the handshake gate (Step 78).
//!
//! # Why this suite exists
//!
//! A mock proves that a client can frame bytes; only the server's own types prove that
//! the two sides agree on what a frame *is* (audit C04). The three things asserted here
//! are the three the client branches on: the envelope is exactly `{event, data}` with a
//! camelCase event name, a handshake without a usable token never reaches an upgrade,
//! and a kick is a fatal error frame rather than something a reconnect loop can retry
//! (audit H07).
//!
//! # What this cannot prove
//!
//! Everything that needs a socket: the heartbeat cadence on a live connection, the
//! close handshake, and recovery after a real network change. Those are the six
//! scenarios in `docs/release-checklist.md`, recorded with server and client logs. This
//! suite is the half that can be asserted without Postgres, Redis and LiveKit, so a
//! protocol regression fails a build rather than a device test.
//!
//! # Adding an event
//!
//! Add it to [`WsEvent`] and to the shape test below. An event the server can send and
//! this suite has never serialised is an event whose wire name nobody checked.

// The panicking lints are denied workspace-wide but explicitly *allowed in tests* — see
// `clippy.toml`. Every `expect` below is a fixture that would make the test meaningless
// if it failed, and its message says so.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use axum::extract::rejection::QueryRejection;
use axum::extract::{FromRequestParts, Query};
use axum::http::{Request as HttpRequest, StatusCode};
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use meno_api::infrastructure::ws::{WsErrorCode, WsEvent, WsPayload, WsQuery};
use meno_api::middleware::auth::{AuthState as GuardState, JwtVerifier, TokenBlocklist};
use meno_api::modules::auth::cache::InMemoryAuthCache;
use meno_api::routes::ws::{required_token, token_required};
use meno_api::types::meno_response::Failure;
use meno_core::ErrorCode;
use serde_json::{Value, json};
use uuid::Uuid;

/// Run axum's `Query` extractor against a URI, yielding exactly what `upgrade` sees.
///
/// The real extractor rather than a hand-built `Query`: the rejection for a missing
/// `?token=` is produced by axum, and a test that skipped it would be asserting a token
/// parser nobody calls.
async fn query_for(uri: &str) -> Result<Query<WsQuery>, QueryRejection> {
    let (mut parts, _) = HttpRequest::builder()
        .uri(uri)
        .body(())
        .expect("a valid request")
        .into_parts();

    Query::<WsQuery>::from_request_parts(&mut parts, &()).await
}

/// The JSON body of a response, as the client receives it.
async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is readable")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("the error envelope is JSON")
}

fn broadcast() -> Uuid {
    Uuid::from_u128(0x018f_2b1e_0000_7000_8000_0000_0000_0001)
}

// ── (a) the frame shape ────────────────────────────────────────────────────

#[tokio::test]
async fn a_frame_is_exactly_an_event_and_a_data_body() {
    // A representative room event, built the way the service builds one.
    let frame = WsPayload::participant_joined(json!({
        "id": Uuid::from_u128(1).to_string(),
        "fullName": "Ada Lovelace",
    }));

    let value = serde_json::to_value(&frame).expect("a frame serialises");
    let object = value.as_object().expect("a frame is a JSON object");

    // Two keys, no more: an envelope that grows a third field is a client change, and
    // the client's decoder reads `event` and `data` and nothing else.
    assert_eq!(object.len(), 2, "{value}");
    assert!(object.contains_key("event"), "{value}");
    assert!(object.contains_key("data"), "{value}");

    // camelCase on the wire, and the body is an object rather than a string that has
    // to be decoded twice.
    assert_eq!(value["event"], "participantJoined", "{value}");
    assert!(value["data"].is_object(), "{value}");
    assert_eq!(value["data"]["fullName"], "Ada Lovelace");

    // And it reads back into the type the server sends, so the envelope is round-trip
    // stable rather than write-only.
    let restored: WsPayload =
        serde_json::from_value(value).expect("a frame the server sent is a frame it can read");
    assert_eq!(restored, frame);
}

#[test]
fn every_event_name_is_the_camel_case_identifier_the_client_switches_on() {
    // The client's decoder matches exact strings (§4.8's stable-token rule applied to
    // event names). Asserting a few by hand pins the convention; the enum's own test
    // covers the rest.
    let names = [
        (WsEvent::Heartbeat, "heartbeat"),
        (WsEvent::BroadcastError, "broadcastError"),
        (WsEvent::ParticipantKicked, "participantKicked"),
        (WsEvent::EndedBroadcast, "endedBroadcast"),
    ];

    for (event, expected) in names {
        assert_eq!(
            serde_json::to_value(event).expect("an event serialises"),
            expected,
            "{event:?} must serialise as {expected}"
        );
    }
}

// ── (b) the handshake gate ─────────────────────────────────────────────────

#[tokio::test]
async fn a_handshake_without_a_usable_token_never_reaches_the_upgrade() {
    // Missing, empty and whitespace-only are one refusal with one message, so the
    // response cannot be used as an oracle for which check failed.
    for uri in ["/ws", "/ws?token=", "/ws?token=%20", "/ws?token=%09%0A"] {
        let error = required_token(query_for(uri).await)
            .expect_err("an unauthenticated handshake must be refused");

        assert_eq!(error.code(), ErrorCode::Unauthorized, "{uri}");
        assert_eq!(
            error.to_string(),
            token_required().to_string(),
            "{uri}: every unusable token gets the same message"
        );

        // The refusal a client actually receives: a 401 envelope, not a socket that
        // opens and then closes.
        let response = Failure(error).into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
        let body = body_json(response).await;
        // The envelope's `status` is the success flag, not the code: `false` here is
        // the same `status` a client checks on every other endpoint.
        assert_eq!(body["status"], false, "{uri}: {body}");
        assert!(body["code"].is_string(), "{uri}: {body}");
        assert!(
            body["message"]
                .as_str()
                .is_some_and(|message| message.contains("access token")),
            "{uri}: {body}"
        );
    }
}

#[tokio::test]
async fn a_present_token_reaches_the_guard_trimmed_and_whole() {
    // Extra query parameters are ignored: a client adding a cache-buster or a mode must
    // not be locked out of its own socket.
    let token = required_token(query_for("/ws?token=%20abc.def.ghi%20&mode=live").await)
        .expect("a token is present");
    assert_eq!(token, "abc.def.ghi");
}

#[tokio::test]
async fn a_token_that_does_not_verify_is_refused_by_the_guard() {
    // The second half of the gate, and the half that needs no socket: the guard
    // `upgrade` calls rejects a token that is present but not a signed access token.
    let guard = GuardState::new(
        Arc::new(
            JwtVerifier::new("integration-access-secret")
                .expect("the fixture access secret builds a verifier"),
        ),
        Arc::new(InMemoryAuthCache::new()) as Arc<dyn TokenBlocklist>,
    );

    let error = guard
        .authenticate("not-a-jwt")
        .await
        .expect_err("a forged token must not authenticate");

    assert_eq!(error.code(), ErrorCode::InvalidToken);
    assert_ne!(
        error.code(),
        ErrorCode::Unauthorized,
        "the guard's refusal is distinguishable from the missing-token one on the wire"
    );
}

// ── (c) the kick ───────────────────────────────────────────────────────────

#[test]
fn a_kick_is_a_fatal_error_frame_the_client_must_not_retry() {
    // There is no close frame for a kick: the server sends this and the client ends the
    // session. Asserting the frame *is* the close contract.
    let frame = WsPayload::error(
        broadcast(),
        WsErrorCode::KickedFromRoom,
        "removed by the host",
    );

    let value = serde_json::to_value(&frame).expect("a frame serialises");

    assert_eq!(value["event"], "broadcastError", "{value}");
    assert_eq!(value["data"]["code"], "KICKED_FROM_ROOM", "{value}");
    assert_eq!(value["data"]["recoverable"], false, "{value}");
    assert_eq!(
        value["data"]["broadcastId"],
        broadcast().to_string(),
        "a room-scoped error names its room"
    );

    // The properties a reconnect loop has to respect, read from the code itself.
    assert!(WsErrorCode::KickedFromRoom.is_fatal());
    assert!(!WsErrorCode::KickedFromRoom.is_recoverable());
}

#[test]
fn a_connection_scoped_error_claims_no_room() {
    // The shape `master` got wrong: `Uuid::nil()` serialised as a real room, so a client
    // could not tell a connection-wide problem from one inside a broadcast.
    let frame = WsPayload::connection_error(WsErrorCode::Unsupported, "that event is not handled");
    let value = serde_json::to_value(&frame).expect("a frame serialises");

    assert_eq!(value["event"], "broadcastError", "{value}");
    assert!(
        value["data"].get("broadcastId").is_none(),
        "a connection-scoped error must not claim a broadcast: {value}"
    );
    assert_eq!(value["data"]["code"], "UNSUPPORTED", "{value}");
}
