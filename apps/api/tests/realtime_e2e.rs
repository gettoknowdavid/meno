//! §10 **End-to-end** layer: a real socket, a real Redis, a real room publish.
//!
//! # Why this file exists
//!
//! Every realtime test before this one used a recording double. That is exactly
//! why the room-membership gap (`docs/backend-gaps.md` gap 1) shipped green: the
//! double asserted that a publish *happened*, never that a socket *received* it.
//! The only test that can catch that class of bug is the one here — open a
//! WebSocket, join a room, publish through `WsPubSubBridge`, and read the frame
//! off the wire.
//!
//! # What it runs against
//!
//! `REDIS_URL`. The test spawns an in-process axum server with a minimal `/ws`
//! route (the production handler needs a full `MenoState`, which would add
//! Postgres, LiveKit and auth keys to a test about fan-out). The route registers
//! the socket in the real [`WsService`] registry and joins the real room through
//! [`WsPubSubBridge`], so the path under test is the production one:
//!
//! ```text
//! PUBLISH -> Redis -> PSUBSCRIBE loop -> registry.send_to_room -> socket
//! ```
//!
//! Without `REDIS_URL` the test returns after printing a skip, matching the
//! repository's other live tiers: a contributor with no Docker still gets a green
//! `cargo test`, and CI provides the service.
//!
//! # What it does not prove
//!
//! The production `handle_socket` framing and heartbeat. Those are covered by
//! `ws_protocol.rs` and `routes::ws`'s own unit tests; this suite is about the
//! Redis hop and room delivery, which no unit test can reach.

// The panicking lints are denied workspace-wide but explicitly *allowed in tests*.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use meno_api::infrastructure::redis::{Redis, RedisConfig};
use meno_api::infrastructure::ws::pubsub::WsPubSubBridge;
use meno_api::infrastructure::ws::{WsEvent, WsPayload, WsService};
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as ClientMessage;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use uuid::Uuid;

/// The state the minimal socket route needs. `join` exists so one test can hold a
/// socket that is registered but deliberately outside the room.
#[derive(Clone)]
struct RoomState {
    hub: WsService,
    bridge: WsPubSubBridge,
    user: Uuid,
    broadcast: Uuid,
    join: bool,
}

async fn upgrade(State(state): State<RoomState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| serve_socket(socket, state))
}

/// Register the socket, join the room when asked, and write delivered frames.
///
/// This is the smallest honest stand-in for `handle_socket`: it uses the same
/// registry and the same room-join path, and it writes the same JSON the
/// production write task writes.
async fn serve_socket(socket: WebSocket, state: RoomState) {
    let (mut sink, _stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<Arc<WsPayload>>(16);

    let Some(conn_id) = state.hub.registry().register(state.user, tx) else {
        return;
    };

    if state.join {
        state
            .hub
            .join_room(state.user, state.broadcast, &state.bridge)
            .await;
    }

    while let Some(payload) = rx.recv().await {
        let Ok(text) = serde_json::to_string(&*payload) else {
            continue;
        };
        if sink.send(Message::Text(text.into())).await.is_err() {
            break;
        }
    }

    state.hub.registry().unregister(state.user, conn_id);
}

/// A live server, its bridge, and the ids the test publishes with.
struct Live {
    addr: std::net::SocketAddr,
    hub: WsService,
    bridge: WsPubSubBridge,
    user: Uuid,
    broadcast: Uuid,
    server: JoinHandle<()>,
}

impl Live {
    /// Connect a client, retrying the handshake until the server is accepting.
    async fn client(&self) -> WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>> {
        let request = format!("ws://{}/ws", self.addr)
            .into_client_request()
            .expect("a valid ws URL");
        let (client, _response) = connect_async(request).await.expect("the socket connects");
        client
    }

    fn stop(self) {
        self.server.abort();
    }
}

/// Start the in-process server over `REDIS_URL`, or `None` when it is unset.
async fn live(join: bool) -> Option<Live> {
    let url = std::env::var("REDIS_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())?;

    let redis = Redis::new(RedisConfig::from_url(url.clone()))
        .await
        .expect("connect to REDIS_URL");
    let hub = WsService::new(redis.clone());
    let bridge = WsPubSubBridge::build(&url, hub.clone(), redis.clone())
        .await
        .expect("build the pub/sub bridge");
    bridge.spawn_subscriber_loop();

    let user = Uuid::new_v4();
    let broadcast = Uuid::new_v4();
    let state = RoomState {
        hub: hub.clone(),
        bridge: bridge.clone(),
        user,
        broadcast,
        join,
    };

    let app = Router::new().route("/ws", get(upgrade)).with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a loopback port");
    let addr = listener.local_addr().expect("the listener has an address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Some(Live {
        addr,
        hub,
        bridge,
        user,
        broadcast,
        server,
    })
}

/// Wait until the route has registered the socket, so a rejection is not just a
/// race with the handshake.
async fn wait_for_registration(live: &Live) {
    for _ in 0..100 {
        if live.hub.registry().connection_count(live.user) == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the socket never registered");
}

#[tokio::test]
async fn a_room_publish_reaches_a_connected_member_socket() {
    let Some(live) = live(true).await else {
        return;
    };

    let mut client = live.client().await;
    wait_for_registration(&live).await;

    // The subscriber loop is spawned asynchronously, so the first publish can land
    // before the room subscription is live. Publishing until a frame arrives is the
    // honest way to wait: it proves delivery rather than assuming a delay is enough.
    let mut frame = None;
    for _ in 0..50 {
        live.bridge
            .publish_to_room(
                live.broadcast,
                WsPayload::new(WsEvent::NewMessage, json!({ "id": "message-1" })),
            )
            .await;

        match timeout(Duration::from_millis(100), client.next()).await {
            Ok(Some(Ok(ClientMessage::Text(text)))) => {
                frame = Some(text.as_str().to_owned());
                break;
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(error))) => panic!("the socket failed: {error}"),
            Ok(None) => panic!("the socket closed before a frame arrived"),
            Err(_) => continue,
        }
    }

    let text = frame.expect("a room publish must reach the member's socket");
    let value: serde_json::Value = serde_json::from_str(&text).expect("the frame is JSON");
    assert_eq!(value["event"], "newMessage", "{value}");
    assert_eq!(value["data"]["id"], "message-1", "{value}");

    // Joining is not only local: the deployment-wide membership SET is what the
    // another-replica paths read.
    assert!(
        live.bridge
            .is_room_member(live.broadcast, live.user)
            .await
            .expect("the membership read runs"),
        "a joined socket must be in the shared room SET"
    );

    live.hub
        .leave_room(live.user, live.broadcast, &live.bridge)
        .await;
    assert!(
        !live
            .bridge
            .is_room_member(live.broadcast, live.user)
            .await
            .expect("the membership read runs"),
        "leaving removes the shared membership"
    );

    drop(client);
    live.stop();
}

#[tokio::test]
async fn a_socket_that_never_joined_the_room_receives_nothing() {
    let Some(live) = live(false).await else {
        return;
    };

    let mut client = live.client().await;
    wait_for_registration(&live).await;

    for _ in 0..5 {
        live.bridge
            .publish_to_room(
                live.broadcast,
                WsPayload::new(WsEvent::NewMessage, json!({ "id": "message-2" })),
            )
            .await;

        let seen = timeout(Duration::from_millis(100), client.next()).await;
        assert!(
            seen.is_err(),
            "a socket outside the room must receive nothing, got {seen:?}"
        );
    }

    drop(client);
    live.stop();
}
