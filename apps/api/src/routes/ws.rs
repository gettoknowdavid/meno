//! `GET /ws` — upgrading an authenticated socket and running its lifetime.
//!
//! # Why the token travels in the query string
//!
//! A browser cannot set an `Authorization` header on a WebSocket handshake, so the
//! access token arrives as `?token=` — the shape [`WsQuery`] documents: a missing
//! parameter fails to deserialise, so an unauthenticated handshake is refused before
//! a socket exists. That puts authentication *inside* this handler rather than behind
//! [`crate::middleware::auth::auth_middleware`], but it does not duplicate the gate:
//! both surfaces call [`AuthState::authenticate`], the one method that owns
//! verify-then-blocklist. A rule that lives in two places is a rule that eventually
//! applies to only one of them.
//!
//! # What the pre-upgrade gate enforces, in order
//!
//! 1. A non-empty token ([`required_token`]).
//! 2. Signature, expiry and revocation (`AuthState::authenticate`).
//! 3. A verified email address — `master`'s gate, kept: everything downstream of the
//!    socket (chat, presence) assumes the address was confirmed.
//! 4. A reconnect-rate limit ([`check_reconnect_rate`]), **fail closed** (§4.4): a
//!    limiter whose Redis is down must not become an unthrottled reconnect storm.
//! 5. The per-user connection cap, so an over-limit caller gets a clean 429 rather
//!    than a socket that opens and immediately closes.
//!
//! Each step short-circuits with the §4.2 error envelope, so a refused handshake
//! reads exactly like a refused HTTP request.
//!
//! # What runs on a connection, and what deliberately does not
//!
//! Wired here: presence, offline-buffer replay, the server-side heartbeat (pings every
//! [`HeartbeatConfig::ping_interval_secs`], a pong deadline that closes the socket),
//! frame dispatch through [`WsEvent::is_client_to_server`], and the cleanup that
//! unregisters the socket and clears presence.
//!
//! **Not** wired, because the modules they belong to do not exist in the monorepo yet:
//! room join/leave, the host grace period, and the chat events' real handlers.
//! `master`'s equivalent reached into `modules::broadcast`, `modules::chat` and
//! `jobs/`; here those four events are answered with an honest "not available yet"
//! connection error, and the room bookkeeping attaches to [`handle_socket`] at the
//! same points `master` used when those modules land.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use axum::extract::State;
use axum::extract::rejection::QueryRejection;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, WebSocketUpgrade};
use axum::response::Response;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use meno_core::{Error as MenoError, ErrorCode};
use serde_json::json;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{interval, timeout};
use uuid::Uuid;

use crate::infrastructure::constants::MAX_WS_CONNECTIONS_PER_USER;
use crate::infrastructure::redis::keys::{RedisKey, ttl};
use crate::infrastructure::ws::{
    ClientMessage, HeartbeatConfig, WsEvent, WsPayload, WsQuery, WsService,
};
use crate::middleware::auth::AuthUser;
use crate::modules::auth::handlers::Failure;
use crate::state::MenoState;

/// Reconnect attempts one user may make inside [`ttl::RECONNECT_RATE`].
///
/// Networks flap, and a client whose signal oscillates will burn this budget on its
/// own without doing anything wrong — so it is headroom for honest reconnects, not a
/// product limit. `master` used the same 10/60s. What matters is that *some* bound
/// exists: without it, one client in a reconnect loop can make the server pay a full
/// authenticate (HMAC + blocklist round trip) plus an upgrade per attempt, forever.
const MAX_RECONNECTS_PER_WINDOW: u64 = 10;

/// The message every refused handshake gets when it presents no usable token.
///
/// One message for "missing", "empty" and "whitespace-only" so the response is not an
/// oracle for which check failed (§4.8's stable codes, same reasoning as the bearer
/// parser in `middleware::auth`).
fn token_required() -> MenoError {
    MenoError::Unauthorized {
        code: ErrorCode::Unauthorized,
        message: "an access token is required: connect with ?token=<access token>".to_owned(),
    }
}

/// Pull the upgrade's token out of `?token=`, refusing before any socket exists.
///
/// The `Err` leg covers both a missing parameter (serde rejects the absent field —
/// see [`WsQuery`]) and one that is blank after trimming, so `?token=` is the same
/// refusal as no query at all.
///
/// # Errors
///
/// [`MenoError::Unauthorized`] with [`ErrorCode::Unauthorized`]; the reason is
/// identical for every way the token can be absent.
fn required_token(query: Result<Query<WsQuery>, QueryRejection>) -> Result<String, MenoError> {
    let Query(query) = query.map_err(|_| token_required())?;

    if query.token.trim().is_empty() {
        return Err(token_required());
    }

    // Trimmed the same way `bearer_token` trims a header value: surrounding whitespace
    // is transport noise, not part of the credential.
    Ok(query.token.trim().to_owned())
}

/// Gate one reconnect attempt: no more than [`MAX_RECONNECTS_PER_WINDOW`] per window.
///
/// Counts *authenticated* attempts only — it runs after verification, so an anonymous
/// flood cannot spend the budget of a real user, and cannot turn this counter into a
/// Redis write amplification either.
///
/// # Errors
///
/// - [`MenoError::RateLimited`] when the window's budget is spent (429, with the
///   window as `retry_after_secs`).
/// - [`MenoError::Upstream`] when Redis cannot be consulted. Deliberately **not**
///   allowed through (§4.4): a limiter that fails open turns the exact storm it
///   exists to stop into an unthrottled one, at the moment Redis is least able to
///   absorb it.
async fn check_reconnect_rate(state: &MenoState, user_id: Uuid) -> Result<(), MenoError> {
    let key = RedisKey::reconnect_rate(user_id, ttl::RECONNECT_RATE);

    let count = state
        .redis
        .incr_and_expire_if_first(&key)
        .await
        .map_err(|error| {
            // The driver's own text is logged, never returned (§9.1).
            tracing::error!(
                user.id = %user_id,
                detail = %error,
                "could not check the WS reconnect rate"
            );
            MenoError::Upstream {
                service: "ws-reconnect-limiter",
                detail: error.to_string(),
            }
        })?;

    if count > MAX_RECONNECTS_PER_WINDOW {
        tracing::warn!(
            user.id = %user_id,
            count,
            "WS reconnect storm suppressed"
        );
        return Err(MenoError::RateLimited {
            retry_after_secs: ttl::RECONNECT_RATE.as_secs(),
        });
    }

    Ok(())
}

/// `GET /ws?token=<access_jwt>` — authenticate, then hand the socket to
/// [`handle_socket`].
///
/// The whole pre-upgrade gate lives here rather than in a layer, because none of it
/// reads an `Authorization` header: the credential is in the query, the rate limit is
/// keyed on the *authenticated* user, and the cap reads this process's registry —
/// three things no shared layer sees.
///
/// Everything runs **before** `on_upgrade`, so a caller who fails any step receives an
/// ordinary HTTP status and never a socket.
///
/// # Errors
///
/// The §4.2 envelope for whichever gate refused: 401 for a missing or unusable token,
/// 403 for an unverified address, 429 for the reconnect window or the connection cap,
/// 503 when the limiter's Redis is down. Returned as [`Failure`] — the shared
/// handler error — rather than a rendered [`Response`], for the reason that type's
/// own docs give: an already-rendered error cannot be inspected by anything downstream.
pub async fn upgrade(
    State(state): State<MenoState>,
    query: Result<Query<WsQuery>, QueryRejection>,
    ws: WebSocketUpgrade,
) -> Result<Response, Failure> {
    let token = required_token(query)?;

    let user = state.auth_guard.authenticate(&token).await?;

    if !user.verified {
        return Err(Failure(MenoError::Forbidden {
            code: ErrorCode::EmailNotVerified,
            message: "verify your email address to connect".to_owned(),
        }));
    }

    check_reconnect_rate(&state, user.id).await?;

    if state.ws.registry().connection_count(user.id) >= MAX_WS_CONNECTIONS_PER_USER {
        tracing::warn!(
            user.id = %user.id,
            connections = MAX_WS_CONNECTIONS_PER_USER,
            "WS connection limit reached before upgrade"
        );
        return Err(Failure(MenoError::RateLimited {
            retry_after_secs: ttl::RECONNECT_RATE.as_secs(),
        }));
    }

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, user, state)))
}

/// Drive one authenticated socket until it ends, then clean up after it.
///
/// The lifetime, in order:
///
/// 1. **Register** — publish the outbound channel into the [`WsService`] registry, so
///    every delivery path (local, pub/sub, buffered) can find this socket. The cap is
///    enforced again here: this is the race between the pre-upgrade check and two
///    handshakes landing together.
/// 2. **Presence** — write the online key. A failure degrades the read model; it does
///    not cost the connection.
/// 3. **Replay** — drain the offline ring buffer *before* any task owns the sink, so
///    buffered messages land ahead of live traffic instead of racing it.
/// 4. **Serve** — heartbeat task (server pings), write task (registry channel →
///    socket), read loop (socket → dispatch), running concurrently until the read
///    loop reports why it ended.
/// 5. **Clean up** — abort both tasks, unregister, clear presence. Unregistering
///    happens even on the error path: a leaked entry keeps `is_online` reporting
///    `true` forever, which is what presence and fan-outs branch on.
///
/// Room join/leave would bracket steps 3–4 (`master`'s handler resolved the live
/// broadcast here); they attach when `modules::broadcast` exists to ask.
async fn handle_socket(socket: WebSocket, user: AuthUser, state: MenoState) {
    let user_id = user.id;
    let (mut sink, mut stream) = socket.split();

    let (hub_tx, hub_rx) = mpsc::channel::<Arc<WsPayload>>(WsService::message_buffer_size());

    let Some(conn_id) = state.ws.registry().register(user_id, hub_tx) else {
        tracing::warn!(user.id = %user_id, "WS connection limit reached at register");
        return;
    };

    tracing::info!(user.id = %user_id, ws.conn_id = conn_id, "WebSocket connected");

    let presence_key = RedisKey::presence(user_id, ttl::PRESENCE);
    if let Err(error) = state.redis.set(&presence_key, &"connected").await {
        tracing::warn!(user.id = %user_id, detail = %error, "could not write presence");
    }

    // Offline replay, before the write task exists: single owner of the sink, so the
    // ring buffer's messages cannot interleave with a live frame.
    let buffered = state.ws.drain_message_buffer(user_id).await;
    if !buffered.is_empty() {
        tracing::info!(
            user.id = %user_id,
            count = buffered.len(),
            "replaying offline messages"
        );
        for payload in &buffered {
            match serde_json::to_string(payload) {
                Ok(json) => {
                    if let Err(error) = sink.send(Message::Text(json.into())).await {
                        tracing::debug!(
                            user.id = %user_id,
                            detail = %error,
                            "replay stopped; socket is gone"
                        );
                        break;
                    }
                }
                Err(error) => {
                    // `WsPayload` is `Value`-backed so this is not reachable in
                    // practice; logged rather than skipped silently (§4.8).
                    tracing::error!(
                        user.id = %user_id,
                        detail = %error,
                        "dropping unserialisable buffered message"
                    );
                }
            }
        }
    }

    let heartbeat = HeartbeatConfig::default();
    // No host tier yet: `host_pong_timeout_secs` is the *host's* deadline, and there
    // is no broadcast module to ask whether this caller is the host of anything.
    // Every connection gets the listener deadline; the tier attaches with
    // `modules::broadcast`.
    let pong_timeout = Duration::from_secs(heartbeat.listener_pong_timeout_secs);
    let missed_pongs = Arc::new(AtomicU32::new(0));

    // The sink is shared between the heartbeat and write tasks, serialised by the
    // mutex — two tasks writing one socket without it would interleave frames.
    let sink = Arc::new(Mutex::new(sink));
    let heartbeat_task = start_heartbeat_task(
        Arc::clone(&sink),
        Arc::clone(&missed_pongs),
        heartbeat.clone(),
        user_id,
    );
    let write_task = start_write_task(Arc::clone(&sink), hub_rx);

    let ended = run_read_loop(
        &state,
        &mut stream,
        user_id,
        &missed_pongs,
        pong_timeout,
        heartbeat.max_missed_pings,
    )
    .await;

    // Answer a peer-initiated close before the socket is dropped. The handshake needs
    // both halves: our `Close` frame and their `Close` ack. Tungstenite answers the ack
    // on its own only if the reply is still queued when the stream is dropped, and if
    // it never arrives the client sees 1006 (abnormal closure) rather than a clean 1000 —
    // which reads as a dropped connection and triggers an immediate reconnect loop.
    //
    // This must be `Sink::close`, not `send(Message::Close)`. Reading a `Close` frame
    // already moves tungstenite's state machine to `ClosedByPeer`, and `Sink::start_send`
    // rejects every frame after `ClosedByPeer` ("Sending after closing is not allowed").
    // `WebSocketContext::close` is the one path that still writes the queued reply, and
    // it is a no-op when there is nothing to answer. A failure here just means the peer
    // is already gone.
    if ended == "closed by peer" {
        let mut sink = sink.lock().await;
        if let Err(error) = sink.close().await {
            tracing::debug!(user.id = %user_id, detail = %error, "close reply not sent");
        }
    }

    heartbeat_task.abort();
    write_task.abort();

    // Unregister first: the moment this entry is gone, `is_online` tells the truth to
    // every other caller, and a delivery attempted after this buffers instead of
    // writing into a dead channel.
    state.ws.registry().unregister(user_id, conn_id);
    if let Err(error) = state.redis.del(&presence_key).await {
        tracing::debug!(user.id = %user_id, detail = %error, "could not clear presence");
    }

    tracing::info!(
        user.id = %user_id,
        ws.conn_id = conn_id,
        ended,
        "WebSocket disconnected"
    );
}

/// Ping the client every [`HeartbeatConfig::ping_interval_secs`].
///
/// Stops pinging (rather than closing) when the read loop has declared the peer dead:
/// the read loop owns the socket's end, and two tasks racing to close it is how a
/// clean close becomes an unclean one. The ping itself failing — socket gone — is the
/// other reason to stop.
fn start_heartbeat_task(
    sink: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    missed_pongs: Arc<AtomicU32>,
    config: HeartbeatConfig,
    user_id: Uuid,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // First tick fires immediately (Tokio's interval semantics), so a fresh
        // connection is probed at once rather than sitting silent for a full interval.
        let mut ticks = interval(Duration::from_secs(config.ping_interval_secs));
        loop {
            ticks.tick().await;

            if missed_pongs.load(Ordering::Relaxed) >= config.max_missed_pings {
                tracing::warn!(user.id = %user_id, "missed too many pings; stopping heartbeat");
                break;
            }

            let mut sink = sink.lock().await;
            if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                tracing::debug!(user.id = %user_id, "ping failed; socket is gone");
                break;
            }
        }
    })
}

/// Drain the registry channel onto the socket until the channel closes.
///
/// This is the only writer once the replay has finished: every delivery path — local,
/// pub/sub relay, buffered replay from *other* messages — reaches this socket through
/// the `mpsc` registered in [`handle_socket`].
fn start_write_task(
    sink: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    mut hub_rx: mpsc::Receiver<Arc<WsPayload>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(payload) = hub_rx.recv().await {
            match serde_json::to_string(&*payload) {
                Ok(json) => {
                    let mut sink = sink.lock().await;
                    if sink.send(Message::Text(json.into())).await.is_err() {
                        tracing::debug!(event = %payload.event, "write failed; socket is gone");
                        break;
                    }
                }
                Err(error) => {
                    tracing::error!(
                        event = %payload.event,
                        detail = %error,
                        "failed to serialise WS frame; dropping it"
                    );
                }
            }
        }
    })
}

/// Read from the socket until it ends, dispatching client frames as they arrive.
///
/// Returns why the loop ended, for the disconnect log line — an operator correlating
/// a spike in reconnects needs "heartbeat timeout" to be distinguishable from
/// "closed by peer".
///
/// # The pong deadline
///
/// Any inbound frame proves liveness and resets the count: a client that is sending
/// does not also have to answer pings. Silence for [`Duration`]s up to `max_missed`
/// consecutive deadlines means the peer is gone — carrier NATs drop idle TCP at
/// roughly 30s, which is why the server pings at all, and why one missed deadline is
/// tolerated before the socket is closed.
async fn run_read_loop(
    state: &MenoState,
    stream: &mut SplitStream<WebSocket>,
    user_id: Uuid,
    missed_pongs: &AtomicU32,
    pong_timeout: Duration,
    max_missed: u32,
) -> &'static str {
    let mut missed = 0u32;

    loop {
        match timeout(pong_timeout, stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                missed = 0;
                missed_pongs.store(0, Ordering::Relaxed);
                handle_client_message(state, user_id, text.as_str()).await;
            }
            // Ping/Pong (ours or the client's) and unsolicited binary all prove the
            // peer is alive; none of them is a frame this server dispatches.
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_)))) => {
                missed = 0;
                missed_pongs.store(0, Ordering::Relaxed);
            }
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) => return "closed by peer",
            Ok(Some(Err(error))) => {
                tracing::warn!(user.id = %user_id, detail = %error, "WS read error");
                return "socket error";
            }
            Err(_) => {
                missed += 1;
                missed_pongs.fetch_add(1, Ordering::Relaxed);
                if missed >= max_missed {
                    tracing::warn!(
                        user.id = %user_id,
                        deadline_secs = pong_timeout.as_secs(),
                        "no traffic within the pong deadline"
                    );
                    return "heartbeat timeout";
                }
            }
        }
    }
}

/// Dispatch one frame the client is entitled to send.
///
/// [`WsEvent::is_client_to_server`] is the gate, so a client cannot drive a
/// server-only event (`endedBroadcast`, `notification`, …) by naming it: server-only
/// is a property of the variant, not of an allow-list someone has to remember to
/// extend.
///
/// The four chat events parse as themselves and then answer honestly — `modules::chat`
/// is not ported, and a connection error saying so is the truthful response until it
/// is. `master` routed them into `modules::chat::handlers` here.
async fn handle_client_message(state: &MenoState, user_id: Uuid, raw: &str) {
    let message: ClientMessage = match serde_json::from_str(raw) {
        Ok(message) => message,
        Err(error) => {
            // The offending frame is not echoed back — it is attacker-shaped text —
            // but it is logged, because a malformed frame from a real client is a
            // version-skew bug worth seeing.
            tracing::warn!(user.id = %user_id, detail = %error, "malformed WS frame");
            return;
        }
    };

    if !message.event.is_client_to_server() {
        tracing::warn!(
            user.id = %user_id,
            event = %message.event,
            "server-only event sent by client"
        );
        state
            .ws
            .send_connection_error(
                user_id,
                format!("{} is a server-to-client event", message.event),
            )
            .await;
        return;
    }

    match message.event {
        WsEvent::Heartbeat => {
            // Acknowledged through the registry like any other payload, so the reply
            // exercises the real delivery path rather than a side channel around it.
            let ack = WsPayload::new(
                WsEvent::Heartbeat,
                json!({
                    "status": "ok",
                    "timestamp": time::OffsetDateTime::now_utc().unix_timestamp(),
                    "userId": user_id,
                }),
            );
            state.ws.send_to_user(user_id, ack).await;

            // Activity refreshes the presence TTL the same way it resets the pong
            // count: an active socket must not age out of "online" mid-session.
            let presence_key = RedisKey::presence(user_id, ttl::PRESENCE);
            if let Err(error) = state.redis.expire(&presence_key, ttl::PRESENCE).await {
                tracing::debug!(user.id = %user_id, detail = %error, "could not refresh presence");
            }

            tracing::debug!(user.id = %user_id, "heartbeat received");
        }
        WsEvent::SendMessage
        | WsEvent::EditMessage
        | WsEvent::DeleteMessage
        | WsEvent::SendReaction => {
            tracing::debug!(
                user.id = %user_id,
                event = %message.event,
                "chat event received; the chat module is not ported yet"
            );
            state
                .ws
                .send_connection_error(user_id, format!("{} is not available yet", message.event))
                .await;
        }
        // Unreachable behind `is_client_to_server` — and that is the point: a variant
        // added to that predicate without a handler lands here as a refusal the client
        // can see, not a panic on a request path (§9.1).
        other => {
            tracing::warn!(user.id = %user_id, event = %other, "no handler for client event");
            state
                .ws
                .send_connection_error(user_id, format!("{} is not implemented yet", other))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    //! The pre-upgrade gate, on its own.
    //!
    //! [`upgrade`] itself needs a [`MenoState`] (pool, Redis, pub/sub bridge), so what
    //! is testable without infrastructure is the gate's first and most spoofable
    //! step — the token parser — exercised through axum's real extractor chain: the
    //! `Err` leg below is the same `QueryRejection` a live handshake without `?token=`
    //! produces, which is the rejection that must never reach an upgrade.

    use super::*;
    use axum::extract::FromRequestParts;
    use axum::http::Request;

    /// Run axum's `Query` extractor against a URI, yielding the value `upgrade` sees.
    async fn query_for(uri: &str) -> Result<Query<WsQuery>, QueryRejection> {
        let (mut parts, _) = Request::builder()
            .uri(uri)
            .body(())
            .expect("a valid request")
            .into_parts();

        Query::<WsQuery>::from_request_parts(&mut parts, &()).await
    }

    #[tokio::test]
    async fn a_handshake_without_a_token_is_refused_before_a_socket_exists() {
        let error = required_token(query_for("/ws").await)
            .expect_err("an unauthenticated handshake must be refused");

        assert_eq!(error.code(), ErrorCode::Unauthorized);
        assert!(error.is_client_safe());
    }

    #[tokio::test]
    async fn an_empty_or_blank_token_is_the_same_refusal_as_a_missing_one() {
        // `?token=` parses successfully to an empty string, so the missing-field
        // rejection alone would not catch it — hence the trim check as well, and
        // hence one message for both.
        for uri in ["/ws?token=", "/ws?token=%20", "/ws?token=%09%0A"] {
            let error = required_token(query_for(uri).await).expect_err(uri);

            assert_eq!(error.code(), ErrorCode::Unauthorized, "{uri}");
            // The same message as the missing-parameter refusal: a blank token must
            // not be distinguishable from an absent one.
            assert_eq!(error.to_string(), token_required().to_string(), "{uri}");
        }
    }

    #[tokio::test]
    async fn a_present_token_is_returned_trimmed_and_whole() {
        // Trimmed like the bearer parser; extra parameters ignored, because a client
        // adding `?mode=` or a cache-buster must not be locked out of the socket.
        let token = required_token(query_for("/ws?token=%20abc.def.ghi%20&mode=live").await)
            .expect("a token is present");

        assert_eq!(token, "abc.def.ghi");
    }

    #[test]
    fn the_reconnect_budget_is_small_enough_to_matter_and_large_enough_to_flap() {
        // The two failure directions: a budget of 1 stops a phone crossing a street
        // (every reconnect refused), an unbounded one is no limiter at all.
        const {
            assert!(MAX_RECONNECTS_PER_WINDOW > 0);
            assert!(MAX_RECONNECTS_PER_WINDOW <= 30);
        }
        // The window is the key's own TTL, so the counter and its expiry cannot drift
        // apart — asserted here rather than trusted.
        assert!(!ttl::RECONNECT_RATE.is_zero());
    }
}
