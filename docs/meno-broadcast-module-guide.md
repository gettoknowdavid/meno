**Meno — Broadcast Module Guide **LiveKit + WebSocket Integration

**MENO**

**Broadcast Module — Engineering Reference**

LiveKit · WebSocket · Axum · Rust

| **Stack**       | Axum 0.7 + Tokio             |
| --------------- | ---------------------------- |
| **LiveKit SDK** | livekit-api ^0.4             |
| **Real-time**   | Native Axum WS + DashMap Hub |
| **Clients**     | Flutter + Next.js            |

# **1. Architecture Overview**

This document is the definitive engineering reference for the Meno Broadcast Module. It covers LiveKit integration, the native WebSocket hub, all DTOs and domain models, and full pseudo-code for every handler, service, repository, and utility function.

## **1.1 The Two Communication Layers**

The broadcast module operates on two distinct real-time layers. Understanding this split is critical before touching any code.

| **Layer**                     | **Technology**        | **Purpose**                                                                                                       |
| ----------------------------- | --------------------- | ----------------------------------------------------------------------------------------------------------------- |
| **Audio/Video Media**         | LiveKit (WebRTC)      | Encrypted audio streaming between participants. The BE never touches media bytes — it only mints tokens.          |
| **App Events ****&**** Chat** | Axum Native WebSocket | Chat messages, listener counts, broadcast lifecycle events, notifications, presence. Replaces Socket.IO entirely. |

| **Key Insight: LiveKit vs. Your WebSocket** LiveKit handles ALL media transport via WebRTC. Your Axum WebSocket is for everything else: chat, event notifications (broadcast started/ended/joined), presence updates, and listener counts. They are completely independent connections on the client — Flutter opens both simultaneously. |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |

## **1.2 Streamlined Broadcast Lifecycle**

The original flow required 5 round-trips to start a broadcast (create → start HTTP → LiveKit init → socket emit → BE acknowledges). The redesigned flow below reduces this to 2 round-trips. This is the industry-standard approach used by platforms like Clubhouse and Twitter Spaces.

### **Starting a Broadcast — Before (5 steps) vs After (2 steps)**

| BEFORE (Original — cumbersome):                                          |
| ------------------------------------------------------------------------ |
| 1. FE: POST /broadcasts → BE returns broadcast (no token)                |
| 2. FE: PUT /broadcasts/:id/start → BE returns broadcast + broadcastToken |
| 3. FE: LiveKit SDK init(token) → media channel established               |
| 4. FE: WS emit 'startedBroadcast' → tells BE 'I am live'                 |
| 5. BE: Sets status=active, emits 'newBroadcast' to subscribers           |
|                                                                          |
| AFTER (Redesigned — 2 steps):                                            |
| 1. FE: POST /broadcasts → BE returns broadcast (saved as draft)          |
| 2. FE: PUT /broadcasts/:id/go-live → BE atomically:                      |
| a) Creates LiveKit room                                                  |
| b) Mints HOST token                                                      |
| c) Sets status = active                                                  |
| d) Creates notifications for subscribers                                 |
| e) Emits 'newBroadcast' WS event                                         |
| f) Returns { broadcast, livekit_token, livekit_url }                     |
| 3. FE: Uses livekit_token to connect LiveKit SDK — done.                 |
|                                                                          |
| The FE no longer needs to emit a socket event to notify the BE.          |
| The BE is the source of truth. LiveKit SDK connects after the response.  |

### **Joining — Before (3 steps) vs After (2 steps)**

| BEFORE:                                                       |
| ------------------------------------------------------------- |
| 1. FE: POST /broadcasts/:id/join → broadcast + broadcastToken |
| 2. FE: LiveKit SDK init(token)                                |
| 3. FE: WS emit 'joinBroadcast' → BE logs the join             |
|                                                               |
| AFTER:                                                        |
| 1. FE: POST /broadcasts/:id/join → BE atomically:             |
| a) Upserts broadcast_listeners row                            |
| b) Mints LISTENER token                                       |
| c) Emits 'newBroadcastListener' WS event to room              |
| d) Returns { broadcast, livekit_token, livekit_url }          |
| 2. FE: Uses livekit_token to connect LiveKit SDK — done.      |

### **Ending ****&**** Leaving — Kept as WebSocket Events**

Ending and leaving are intentionally kept as WebSocket events. Here is why:

- Ending: The host may lose internet. A socket disconnect (hostDisconnected event) triggers the same cleanup path as an intentional endBroadcast event, which would not be possible with HTTP.

- Leaving: Listeners may close the app without pressing leave. The WS close handler cleans up broadcast_listeners automatically.

- This is how Clubhouse, Discord, and Twitter Spaces all handle it. The key principle is that the server cleans up on disconnect, not on explicit FE action.

| ENDING (WebSocket, unchanged but clarified):                        |
| ------------------------------------------------------------------- |
| FE: WS send { event: 'endBroadcast', broadcastId: '...' }           |
| OR: WS connection drops → BE detects via handle_socket cleanup      |
| BE: Sets status=inactive, end_time=now(), deletes LiveKit room,     |
| emits 'endedBroadcast' to all room participants                     |
|                                                                     |
| LEAVING (WebSocket, unchanged but clarified):                       |
| FE: WS send { event: 'leaveBroadcast', broadcastId: '...' }         |
| OR: WS connection drops → BE cleanup removes listener automatically |
| BE: Deletes broadcast_listeners row, emits 'broadcastListenerLeft'  |

# **2. LiveKit — Setup ****&**** Integration**

## **2.1 Cargo Dependencies**

The backend only needs the server-side SDK for token minting and room management. The client (Flutter, Next.js) uses their respective LiveKit client SDKs. The Rust server never joins a room as a participant.

| # apps/api/Cargo.toml                                               |
| ------------------------------------------------------------------- |
| [dependencies]                                                      |
| # LiveKit server SDK — token generation and room management API     |
| livekit-api = "0.4"                                                 |
|                                                                     |
| # NOTE: Do NOT add the 'livekit' client crate (0.7.x).              |
| # That is for Rust processes that JOIN rooms as media participants. |
| # Your backend only mints tokens and calls the LiveKit server API.  |
| # The 'livekit-api' crate is the correct one for backends.          |

## **2.2 LiveKit Service — State ****&**** Initialisation**

The LiveKitService wraps the two LiveKit API clients and lives inside AppState. One client mints tokens (synchronous, no network), the other makes gRPC calls to the LiveKit server (delete room, list participants, etc.).

| // apps/api/src/shared/livekit/service.rs                        |
| ---------------------------------------------------------------- |
|                                                                  |
| pub struct LiveKitService {                                      |
| api_key: String,                                                 |
| api_secret: String,                                              |
| livekit_host: String, // e.g. https://your-project.livekit.cloud |
| room_client: RoomClient, // from livekit_api::services::room     |
| }                                                                |
|                                                                  |
| // In build_state():                                             |
| // let room_client = RoomClient::new(&config.livekit_host)       |
| // .expect('Invalid LiveKit host URL');                          |
| // The RoomClient is created once and cloned into Arc<AppState>. |
| // It is cheaply cloneable — just an Arc internally.             |

## **2.3 Token Minting — Per Role**

Every participant needs a JWT token minted by YOUR server using YOUR LiveKit API key+secret. The token encodes the room name, the participant identity, and their permissions (can_publish, can_subscribe, room_admin). The client presents this token to LiveKit Cloud to join.

| FUNCTION mint_token(user_id, user_name, broadcast_id, role) -> Result<String> |
| ----------------------------------------------------------------------------- |
|                                                                               |
| // The room name is deterministic — always the broadcast's UUID as a string.  |
| // This means you can always reconstruct the room name from the broadcast ID. |
| room_name = broadcast_id.to_string()                                          |
|                                                                               |
| // Participant identity must be unique within the room.                       |
| // Using user_id (UUID string) guarantees uniqueness.                         |
| identity = user_id.to_string()                                                |
|                                                                               |
| // Build grants based on role                                                 |
| grants = match role {                                                         |
| HOST => VideoGrants {                                                         |
| room_join: true,                                                              |
| room: room_name.clone(),                                                      |
| can_publish: true, // can send audio/video                                    |
| can_subscribe: true, // can receive audio/video                               |
| room_admin: true, // can mute/kick participants                               |
| ..Default::default()                                                          |
| },                                                                            |
| COHOST => VideoGrants {                                                       |
| room_join: true,                                                              |
| room: room_name.clone(),                                                      |
| can_publish: true,                                                            |
| can_subscribe: true,                                                          |
| room_admin: false,                                                            |
| ..Default::default()                                                          |
| },                                                                            |
| LISTENER => VideoGrants {                                                     |
| room_join: true,                                                              |
| room: room_name.clone(),                                                      |
| can_publish: false, // cannot send audio — listen only                        |
| can_subscribe: true,                                                          |
| room_admin: false,                                                            |
| ..Default::default()                                                          |
| },                                                                            |
| }                                                                             |
|                                                                               |
| token = AccessToken::with_api_key(&api_key, &api_secret)                      |
| .with_identity(&identity)                                                     |
| .with_name(&user_name)                                                        |
| .with_grants(grants)                                                          |
| .with_ttl(Duration::from_secs(6 * 3600)) // 6 hour TTL                        |
| .to_jwt()?                                                                    |
|                                                                               |
| return Ok(token)                                                              |
|                                                                               |
| // Note on TTL: 6 hours is enough for any realistic broadcast.                |
| // If a broadcast runs longer, the FE must call /broadcasts/:id/token         |
| // to get a refreshed token (see refresh endpoint in Section 5).              |

## **2.4 Room Management Operations**

| FUNCTION create_livekit_room(broadcast_id) -> Result<()>                      |
| ----------------------------------------------------------------------------- |
| // Called when host goes live. Creates the room on LiveKit server.            |
| // If the room already exists, LiveKit ignores the call (idempotent).         |
| room_client.create_room(broadcast_id.to_string(), CreateRoomOptions {         |
| max_participants: 2000,                                                       |
| empty_timeout: 300, // secs before auto-close if empty                        |
| metadata: broadcast_id.to_string(), // useful for webhooks                    |
| ..Default::default()                                                          |
| }).await?                                                                     |
|                                                                               |
| FUNCTION delete_livekit_room(broadcast_id) -> Result<()>                      |
| // Called when broadcast ends. Kicks everyone and closes the room.            |
| room_client.delete_room(broadcast_id.to_string()).await?                      |
|                                                                               |
| FUNCTION list_live_participants(broadcast_id) -> Result<Vec<ParticipantInfo>> |
| // Returns participants currently in the LiveKit room.                        |
| // Used for GET /broadcasts/:id/live-listeners endpoint.                      |
| room_client.list_participants(broadcast_id.to_string()).await?                |
|                                                                               |
| FUNCTION remove_participant(broadcast_id, user_id) -> Result<()>              |
| // Kicks a specific participant (e.g. when removing a cohost mid-broadcast).  |
| room_client.remove_participant(                                               |
| broadcast_id.to_string(),                                                     |
| user_id.to_string() // identity = user_id                                     |
| ).await?                                                                      |
|                                                                               |
| FUNCTION mute_participant(broadcast_id, user_id, track_sid) -> Result<()>     |
| // Server-side mute. Useful for host muting a listener who spams audio.       |
| room_client.mute_published_track(                                             |
| broadcast_id.to_string(),                                                     |
| user_id.to_string(),                                                          |
| track_sid,                                                                    |
| true // muted = true                                                          |
| ).await?                                                                      |

# **3. WebSocket Hub — Architecture ****&**** Events**

## **3.1 Why Native Axum WS Over Socket.IO**

| **Socket.IO (NestJS)**             | **Axum Native WS (Rust)**            |
| ---------------------------------- | ------------------------------------ |
| JS runtime overhead per connection | Zero-cost Tokio tasks per connection |
| Polling fallback adds latency      | Pure WebSocket — no fallback needed  |
| JS serialisation bottleneck        | serde_json at Rust speed             |
| socketio-adapter for Redis scaling | Custom Redis bridge (simpler, typed) |
| Rooms concept (implicit)           | Explicit WsHub with typed events     |

## **3.2 WsHub — Data Structures**

| // apps/api/src/shared/ws/hub.rs                                           |
| -------------------------------------------------------------------------- |
|                                                                            |
| // Each connected client gets a dedicated mpsc channel.                    |
| // The hub stores sender halves, keyed by user_id.                         |
| // One user can have multiple senders (web tab + mobile app).              |
|                                                                            |
| pub struct WsHub {                                                         |
| // DashMap = concurrent HashMap, no lock contention for reads              |
| clients: DashMap<Uuid, Vec<ConnectionSender>>,                             |
| }                                                                          |
|                                                                            |
| pub struct ConnectionSender {                                              |
| id: usize, // unique per-connection ID                                     |
| sender: mpsc::Sender<Arc<WsPayload>>, // Arc avoids cloning large messages |
| }                                                                          |
|                                                                            |
| // WsPayload is what gets serialised to JSON and sent to the client.       |
| // The 'event' field mirrors your existing Socket.IO event names exactly   |
| // so the Flutter/Next.js clients need minimal changes.                    |
| #[derive(Serialize, Clone)]                                                |
| pub struct WsPayload {                                                     |
| pub event: String, // e.g. 'newBroadcast', 'endedBroadcast'                |
| pub data: serde_json::Value,                                               |
| }                                                                          |

## **3.3 Complete WebSocket Event Reference**

These event names are preserved 1:1 from your existing Socket.IO implementation (confirmed from the Postman screenshots). The Flutter and Next.js clients subscribe to these exact strings.

| **Event Name**         | **Direction** | **Sent To**                    | **Payload / Trigger**                                      |
| ---------------------- | ------------- | ------------------------------ | ---------------------------------------------------------- |
| endBroadcast           | FE → BE       | —                              | FE sends {broadcastId}. BE ends broadcast.                 |
| leaveBroadcast         | FE → BE       | —                              | FE sends {broadcastId}. BE removes listener.               |
| newBroadcast           | BE → FE       | All subscribers of creator     | Broadcast went live. Payload: BroadcastDto                 |
| endedBroadcast         | BE → FE       | All room participants          | Broadcast ended. Payload: {broadcastId, reason}            |
| newBroadcastListener   | BE → FE       | Host + cohosts in room         | A listener joined. Payload: UserSummaryDto                 |
| broadcastListenerLeft  | BE → FE       | Host + cohosts in room         | A listener left. Payload: {userId, broadcastId}            |
| numberOfLiveBroadcasts | BE → FE       | Connected client               | Count of active broadcasts. Payload: {count}               |
| numberOfLiveListeners  | BE → FE       | Room participants              | Live listener count update. Payload: {broadcastId, count}  |
| newCohost              | BE → FE       | Targeted user (new cohost)     | You were made a cohost. Payload: {broadcastId, token, url} |
| removedCohost          | BE → FE       | Targeted user (removed cohost) | You were removed as cohost.                                |
| newMessage             | BE → FE       | All room participants          | New chat message. Payload: ChatMessageDto                  |
| hostDisconnected       | BE → FE       | All room participants          | Host WS dropped. Payload: {broadcastId}                    |
| hostReconnected        | BE → FE       | All room participants          | Host WS reconnected. Payload: {broadcastId}                |
| notification           | BE → FE       | Targeted user                  | A new in-app notification. Payload: NotificationDto        |

## **3.4 WebSocket Handler — Full Pseudo-code**

The WebSocket connection is authenticated at upgrade time via a JWT token in the query string (same approach as your existing Postman collection: ?token={{access_token}}).

| // GET /ws?token=<access_jwt>                                                    |
| -------------------------------------------------------------------------------- |
| HANDLER ws_upgrade(ws: WebSocketUpgrade, Query{token}, State(state)) -> Response |
|                                                                                  |
| // 1. Authenticate BEFORE upgrading. Reject unauthenticated connections.         |
| claims = state.jwt.decode_access(&token)                                         |
| ELSE return 401 Unauthorized (not a WS upgrade)                                  |
|                                                                                  |
| user = state.user_repo.find_by_id(claims.sub).await?                             |
| ELSE return 400 'User does not exist'                                            |
|                                                                                  |
| // 2. Upgrade to WebSocket and hand off to handle_socket                         |
| ws.on_upgrade(│socket│ handle_socket(socket, user, state))                       |
|                                                                                  |
| ─────────────────────────────────────────────────────                            |
|                                                                                  |
| ASYNC FUNCTION handle_socket(socket, user, state)                                |
|                                                                                  |
| (ws_sender, ws_receiver) = socket.split()                                        |
| (hub_tx, hub_rx) = mpsc::channel::<Arc<WsPayload>>(128)                          |
| connection_id = generate_unique_id() // atomic counter                           |
|                                                                                  |
| // 3. Register in hub (user can have multiple connections)                       |
| state.ws_hub.register(user.id, connection_id, hub_tx)                            |
|                                                                                  |
| // 4. Mark presence: if this is user's FIRST connection, notify subscribers      |
| if state.ws_hub.connection_count(user.id) == 1 {                                 |
| // Do NOT emit a WS event for presence — just track in Redis                     |
| state.redis.set('presence:{user.id}', 'online', EX 90).await                     |
| }                                                                                |
|                                                                                  |
| // 5. Spawn write task: hub messages → WebSocket frames                          |
| write_task = tokio::spawn(async {                                                |
| loop {                                                                           |
| match hub_rx.recv().await {                                                      |
| Some(payload) => {                                                               |
| json = serde_json::to_string(&payload)?                                          |
| ws_sender.send(Message::Text(json)).await?                                       |
| }                                                                                |
| None => break // hub dropped sender, connection closing                          |
| }                                                                                |
| }                                                                                |
| })                                                                               |
|                                                                                  |
| // 6. Read loop: process messages FROM the client                                |
| loop {                                                                           |
| match ws_receiver.next().await {                                                 |
| Some(Ok(Message::Text(text))) => {                                               |
| handle_client_message(text, user.id, state).await                                |
| }                                                                                |
| Some(Ok(Message::Ping(data))) => {                                               |
| // Axum handles Pong automatically — no action needed                            |
| }                                                                                |
| Some(Ok(Message::Close(_))) │ None => break                                      |
| Some(Err(e)) => { tracing::warn!('WS error: {}', e); break }                     |
| _ => {}                                                                          |
| }                                                                                |
| }                                                                                |
|                                                                                  |
| // 7. Cleanup on disconnect                                                      |
| write_task.abort()                                                               |
| state.ws_hub.unregister(user.id, connection_id)                                  |
|                                                                                  |
| // If this was the user's LAST connection:                                       |
| if state.ws_hub.connection_count(user.id) == 0 {                                 |
| state.redis.del('presence:{user.id}').await                                      |
| // Check if user was a host of an active broadcast                               |
| if let Some(broadcast) = state.broadcast_repo                                    |
| .find_active_hosted_by(user.id).await {                                          |
| // Emit hostDisconnected — does NOT end the broadcast                            |
| // The broadcast stays active; host has 60s grace period to reconnect            |
| broadcast_service.on_host_disconnected(broadcast.id, state).await                |
| }                                                                                |
| // Check if user was a listener                                                  |
| broadcast_service.on_listener_disconnected(user.id, state).await                 |
| }                                                                                |

## **3.5 Client Message Handler**

| ASYNC FUNCTION handle_client_message(raw_text, user_id, state)                         |
| -------------------------------------------------------------------------------------- |
|                                                                                        |
| // Parse the incoming JSON                                                             |
| msg: ClientMessage = serde_json::from_str(&raw_text)                                   |
| ELSE { tracing::warn!('Invalid WS message from {}', user_id); return }                 |
|                                                                                        |
| // ClientMessage shape (mirrors existing Socket.IO emit format):                       |
| // { 'event': 'endBroadcast', 'data': { 'broadcastId': '...' } }                       |
|                                                                                        |
| match msg.event.as_str() {                                                             |
| 'endBroadcast' => {                                                                    |
| broadcast_id = parse_uuid(msg.data['broadcastId'])?                                    |
| broadcast_service.end_broadcast(broadcast_id, user_id, EndReason::Normal, state).await |
| }                                                                                      |
| 'leaveBroadcast' => {                                                                  |
| broadcast_id = parse_uuid(msg.data['broadcastId'])?                                    |
| broadcast_service.leave_broadcast(broadcast_id, user_id, state).await                  |
| }                                                                                      |
| _ => tracing::warn!('Unknown WS event: {}', msg.event)                                 |
| }                                                                                      |

# **4. Domain Models ****&**** DTOs**

## **4.1 Domain Models — models.rs**

These structs map 1:1 to database rows. They are the output of sqlx queries. They are NEVER sent directly to the client — always converted to DTOs.

| // modules/broadcast/models.rs                                            |
| ------------------------------------------------------------------------- |
|                                                                           |
| #[derive(Debug, Clone, sqlx::FromRow)]                                    |
| pub struct Broadcast {                                                    |
| pub id: Uuid,                                                             |
| pub title: String,                                                        |
| pub description: String,                                                  |
| pub status: BroadcastStatus, // enum: active │ inactive                   |
| pub broadcast_token: Option<String>, // LiveKit JWT, stored while live    |
| pub time_zone: String,                                                    |
| pub image_id: Option<String>,                                             |
| pub image_url: Option<String>,                                            |
| pub start_time: Option<DateTime<Utc>>, // scheduled start                 |
| pub end_time: Option<DateTime<Utc>>,                                      |
| pub created_at: Option<DateTime<Utc>>,                                    |
| pub deleted_at: Option<DateTime<Utc>>,                                    |
| pub creator_id: Uuid,                                                     |
| }                                                                         |
|                                                                           |
| #[derive(Debug, Clone, sqlx::FromRow)]                                    |
| pub struct BroadcastListener {                                            |
| pub listener_id: Uuid,                                                    |
| pub broadcast_id: Uuid,                                                   |
| pub joined_at: DateTime<Utc>,                                             |
| pub role: BroadcastRole, // HOST │ COHOST │ LISTENER                      |
| }                                                                         |
|                                                                           |
| #[derive(Debug, Clone, sqlx::FromRow)]                                    |
| pub struct BroadcastCohost {                                              |
| pub cohost_id: Uuid,                                                      |
| pub broadcast_id: Uuid,                                                   |
| }                                                                         |
|                                                                           |
| #[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize, PartialEq)]    |
| #[sqlx(type_name = "Status", rename_all = "lowercase")]                   |
| pub enum BroadcastStatus { Active, Inactive }                             |
|                                                                           |
| #[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize, PartialEq)]    |
| #[sqlx(type_name = "BroadcastRole", rename_all = "SCREAMING_SNAKE_CASE")] |
| pub enum BroadcastRole { Host, Cohost, Listener }                         |

## **4.2 Request DTOs — dto.rs (Inbound)**

| // modules/broadcast/dto.rs — Request types (FE → BE)           |
| --------------------------------------------------------------- |
|                                                                 |
| #[derive(Debug, Deserialize, Validate)]                         |
| pub struct CreateBroadcastDto {                                 |
| #[validate(length(min = 1, max = 100))]                         |
| pub title: String,                                              |
| #[validate(length(min = 1, max = 244))]                         |
| pub description: String,                                        |
| pub time_zone: Option<String>, // defaults to 'Etc/UTC'         |
| pub start_time: Option<DateTime<Utc>>, // None = no scheduling  |
| pub image_url: Option<String>,                                  |
| pub image_id: Option<String>,                                   |
| }                                                               |
|                                                                 |
| #[derive(Debug, Deserialize, Validate)]                         |
| pub struct UpdateBroadcastDto {                                 |
| #[validate(length(min = 1, max = 100))]                         |
| pub title: Option<String>,                                      |
| #[validate(length(min = 1, max = 244))]                         |
| pub description: Option<String>,                                |
| pub time_zone: Option<String>,                                  |
| pub start_time: Option<DateTime<Utc>>,                          |
| pub image_url: Option<String>,                                  |
| pub image_id: Option<String>,                                   |
| }                                                               |
|                                                                 |
| #[derive(Debug, Deserialize)]                                   |
| pub struct GetBroadcastsQuery {                                 |
| pub status: Option<BroadcastStatus>,                            |
| pub creator_id: Option<Uuid>,                                   |
| pub only_subscriptions: Option<bool>, // filter by who I follow |
| pub keywords: Option<String>,                                   |
| pub page: Option<i64>, // defaults to 1                         |
| pub limit: Option<i64>, // defaults to 20, max 100              |
| }                                                               |
|                                                                 |
| #[derive(Debug, Deserialize)]                                   |
| pub struct AddCohostDto {                                       |
| pub user_id: Uuid,                                              |
| }                                                               |

## **4.3 Response DTOs — dto.rs (Outbound)**

| // Response types (BE → FE)                                        |
| ------------------------------------------------------------------ |
|                                                                    |
| // Standard broadcast shape sent to clients.                       |
| // Computed fields (listener counts etc) are optional.             |
| #[derive(Debug, Serialize, Clone)]                                 |
| #[serde(rename_all = "camelCase")]                                 |
| pub struct BroadcastDto {                                          |
| pub id: Uuid,                                                      |
| pub title: String,                                                 |
| pub description: String,                                           |
| pub status: BroadcastStatus,                                       |
| pub time_zone: String,                                             |
| pub image_url: Option<String>,                                     |
| pub start_time: Option<DateTime<Utc>>,                             |
| pub end_time: Option<DateTime<Utc>>,                               |
| pub created_at: Option<DateTime<Utc>>,                             |
| pub creator_id: Uuid,                                              |
| pub creator: Option<UserSummaryDto>, // joined when needed         |
| pub total_listeners: Option<i64>, // aggregate, joined when needed |
| pub cohosts: Option<Vec<UserSummaryDto>>,                          |
| }                                                                  |
|                                                                    |
| // Returned by go-live and join endpoints.                         |
| // Contains everything the FE needs to connect to LiveKit.         |
| #[derive(Debug, Serialize)]                                        |
| #[serde(rename_all = "camelCase")]                                 |
| pub struct BroadcastSessionDto {                                   |
| pub broadcast: BroadcastDto,                                       |
| pub livekit_token: String, // JWT for the LiveKit SDK              |
| pub livekit_url: String, // wss://your-project.livekit.cloud       |
| }                                                                  |
|                                                                    |
| // Cohost-specific session (includes their specific token)         |
| #[derive(Debug, Serialize)]                                        |
| #[serde(rename_all = "camelCase")]                                 |
| pub struct CohostSessionDto {                                      |
| pub user: UserSummaryDto,                                          |
| pub livekit_token: String,                                         |
| pub livekit_url: String,                                           |
| }                                                                  |
|                                                                    |
| // Compact user shape used within broadcast responses              |
| #[derive(Debug, Serialize, Clone)]                                 |
| #[serde(rename_all = "camelCase")]                                 |
| pub struct UserSummaryDto {                                        |
| pub id: Uuid,                                                      |
| pub full_name: String,                                             |
| pub image_url: Option<String>,                                     |
| }                                                                  |
|                                                                    |
| // WS payload shapes (serialised into WsPayload.data)              |
| #[derive(Serialize, Clone)]                                        |
| #[serde(rename_all = "camelCase")]                                 |
| pub struct BroadcastEndedPayload {                                 |
| pub broadcast_id: Uuid,                                            |
| pub reason: EndReason, // Normal │ HostDisconnected │ AdminForced  |
| }                                                                  |
|                                                                    |
| #[derive(Serialize, Clone)]                                        |
| pub enum EndReason { Normal, HostDisconnected, AdminForced }       |

# **5. Route Handlers — routes.rs**

Handlers are intentionally thin. They extract, validate, call service, return. All business logic lives in service.rs.

## **5.1 Router Registration**

| // modules/broadcast/mod.rs                                          |
| -------------------------------------------------------------------- |
|                                                                      |
| pub fn broadcast_router() -> Router<Arc<AppState>> {                 |
| Router::new()                                                        |
| // Public routes (no auth)                                           |
| .route('/broadcasts', get(list_broadcasts))                          |
| .route('/broadcasts/:id', get(get_broadcast))                        |
| .route('/broadcasts/:id/listeners', get(get_broadcast_listeners))    |
| .route('/broadcasts/:id/live-listeners', get(get_live_listeners))    |
|                                                                      |
| // Authenticated routes                                              |
| .route('/broadcasts', post(create_broadcast))                        |
| .route('/broadcasts/:id', put(update_broadcast))                     |
| .route('/broadcasts/:id', delete(delete_broadcast))                  |
| .route('/broadcasts/:id/go-live', put(go_live))                      |
| .route('/broadcasts/:id/join', post(join_broadcast))                 |
| .route('/broadcasts/:id/token', post(refresh_token))                 |
| .route('/broadcasts/:id/co-hosts', post(add_cohost))                 |
| .route('/broadcasts/:id/co-hosts/:user_id', delete(remove_cohost))   |
| .route_layer(middleware::from_fn_with_state(state, auth_middleware)) |
| }                                                                    |

## **5.2 Handler Pseudo-code**

| HANDLER create_broadcast(AuthUser(user), Json(dto: CreateBroadcastDto)) -> BroadcastDto           |
| ------------------------------------------------------------------------------------------------- |
| dto.validate()?                                                                                   |
| broadcast = broadcast_service.create(user.id, dto, &state).await?                                 |
| return 201 Json(BroadcastDto::from(broadcast))                                                    |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER go_live(Path(id), AuthUser(user)) -> BroadcastSessionDto                                  |
| // The key endpoint. Atomically starts the broadcast and returns the token.                       |
| session = broadcast_service.go_live(id, user.id, &state).await?                                   |
| return 200 Json(session)                                                                          |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER join_broadcast(Path(id), AuthUser(user)) -> BroadcastSessionDto                           |
| session = broadcast_service.join(id, user.id, &state).await?                                      |
| return 200 Json(session)                                                                          |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER list_broadcasts(Query(params: GetBroadcastsQuery), OptionalAuth(user))                    |
| // OptionalAuth: the user MAY be authenticated (for subscription filter)                          |
| broadcasts = broadcast_service.list(params, user.map(│u│ u.id), &state).await?                    |
| return 200 Json(PaginatedResponse<BroadcastDto>)                                                  |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER get_broadcast(Path(id)) -> BroadcastDto                                                   |
| broadcast = broadcast_service.get_by_id(id, &state).await?                                        |
| return 200 Json(BroadcastDto::from(broadcast))                                                    |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER update_broadcast(Path(id), AuthUser(user), Json(dto)) -> BroadcastDto                     |
| dto.validate()?                                                                                   |
| broadcast = broadcast_service.update(id, user.id, dto, &state).await?                             |
| return 200 Json(BroadcastDto::from(broadcast))                                                    |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER delete_broadcast(Path(id), AuthUser(user)) -> ()                                          |
| broadcast_service.delete(id, user.id, &state).await?                                              |
| return 200                                                                                        |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER get_broadcast_listeners(Path(id), Query(pagination)) -> PaginatedResponse<UserSummaryDto> |
| // Returns all-time listeners from DB (broadcast_listeners table)                                 |
| result = broadcast_service.get_listeners(id, pagination, &state).await?                           |
| return 200 Json(result)                                                                           |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER get_live_listeners(Path(id)) -> Vec<UserSummaryDto>                                       |
| // Calls LiveKit to get currently connected participants                                          |
| participants = broadcast_service.get_live_participants(id, &state).await?                         |
| return 200 Json(participants)                                                                     |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER add_cohost(Path(id), AuthUser(user), Json(dto: AddCohostDto)) -> CohostSessionDto         |
| session = broadcast_service.add_cohost(id, user.id, dto.user_id, &state).await?                   |
| return 201 Json(session)                                                                          |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER remove_cohost(Path(id, cohost_id), AuthUser(user)) -> ()                                  |
| broadcast_service.remove_cohost(id, user.id, cohost_id, &state).await?                            |
| return 200                                                                                        |
|                                                                                                   |
| ─────────────────────────────────────────────────────                                             |
|                                                                                                   |
| HANDLER refresh_token(Path(id), AuthUser(user)) -> BroadcastSessionDto                            |
| // Called by FE when an existing LiveKit token is near expiry (< 5 min left).                     |
| // FE should check token expiry and call this proactively.                                        |
| session = broadcast_service.refresh_token(id, user.id, &state).await?                             |
| return 200 Json(session)                                                                          |

# **6. Service Layer — service.rs**

This is where all business logic lives. Services orchestrate repositories, LiveKit, the WS hub, and the notification system.

## **6.1 create()**

| ASYNC FUNCTION create(creator_id, dto, state) -> Result<Broadcast>             |
| ------------------------------------------------------------------------------ |
|                                                                                |
| // No LiveKit call here. The broadcast is just a DB record (a 'draft').        |
| // This matches the pattern of creating a scheduled event before it goes live. |
| broadcast = broadcast_repo.create(CreateBroadcastInput {                       |
| title: dto.title,                                                              |
| description: dto.description,                                                  |
| time_zone: dto.time_zone.unwrap_or('Etc/UTC'),                                 |
| start_time: dto.start_time,                                                    |
| image_url: dto.image_url,                                                      |
| image_id: dto.image_id,                                                        |
| creator_id,                                                                    |
| status: BroadcastStatus::Inactive,                                             |
| }, &state.db).await?                                                           |
|                                                                                |
| // If a start_time is set, schedule the apalis background job                  |
| if let Some(start_time) = dto.start_time {                                     |
| schedule_broadcast_start_job(broadcast.id, start_time, &state).await?          |
| }                                                                              |
|                                                                                |
| return Ok(broadcast)                                                           |

## **6.2 go_live() — The Critical Path**

This is the most important function. It must be atomic: either everything succeeds, or nothing does. Use a database transaction to protect the DB state. LiveKit operations are outside the transaction but are idempotent (safe to retry).

| ASYNC FUNCTION go_live(broadcast_id, user_id, state) -> Result<BroadcastSessionDto>                    |
| ------------------------------------------------------------------------------------------------------ |
|                                                                                                        |
| // 1. Fetch and validate                                                                               |
| broadcast = broadcast_repo.find_by_id(broadcast_id, &db).await?                                        |
| .ok_or(AppError::NotFound('Broadcast not found'))?                                                     |
|                                                                                                        |
| if broadcast.creator_id != user_id {                                                                   |
| return Err(AppError::Forbidden('Only the creator can start this broadcast'))                           |
| }                                                                                                      |
| if broadcast.status == BroadcastStatus::Active {                                                       |
| return Err(AppError::Conflict('Broadcast is already live'))                                            |
| }                                                                                                      |
|                                                                                                        |
| user = user_repo.find_by_id(user_id, &db).await?                                                       |
| .ok_or(AppError::NotFound('User not found'))?                                                          |
|                                                                                                        |
| // 2. Create the LiveKit room BEFORE updating DB.                                                      |
| // If LiveKit fails, we never update the DB — safe rollback.                                           |
| livekit_service.create_room(broadcast_id).await?                                                       |
|                                                                                                        |
| // 3. Mint the HOST token                                                                              |
| host_token = livekit_service.mint_token(                                                               |
| user_id, user.full_name, broadcast_id, BroadcastRole::Host                                             |
| )?                                                                                                     |
|                                                                                                        |
| // 4. DB transaction: update broadcast + insert host into listeners                                    |
| let mut tx = db.begin().await?                                                                         |
|                                                                                                        |
| broadcast = broadcast_repo.set_active(broadcast_id, &host_token, &mut tx).await?                       |
|                                                                                                        |
| // Insert host as a listener with role=HOST (so they appear in participant lists)                      |
| broadcast_repo.upsert_listener(UpsertListenerInput {                                                   |
| listener_id: user_id,                                                                                  |
| broadcast_id,                                                                                          |
| role: BroadcastRole::Host,                                                                             |
| joined_at: Utc::now(),                                                                                 |
| }, &mut tx).await?                                                                                     |
|                                                                                                        |
| tx.commit().await?                                                                                     |
|                                                                                                        |
| // 5. Fan-out notifications and WS events (fire-and-forget, don't block response)                      |
| // These run in background tasks so the response returns fast.                                         |
| let state_clone = Arc::clone(&state)                                                                   |
| let broadcast_clone = broadcast.clone()                                                                |
| tokio::spawn(async move {                                                                              |
| // 5a. Get all subscriber IDs for this creator                                                         |
| subscriber_ids = subscriber_repo.get_subscriber_ids(broadcast_clone.creator_id, &state_clone.db).await |
|                                                                                                        |
| // 5b. Create in-app notifications for each subscriber                                                 |
| notification_service.create_bulk(                                                                      |
| NotificationType::LiveBroadcastStarted,                                                                |
| broadcast_clone.id,                                                                                    |
| subscriber_ids.clone(),                                                                                |
| &state_clone                                                                                           |
| ).await                                                                                                |
|                                                                                                        |
| // 5c. Emit 'newBroadcast' WS event to all online subscribers                                          |
| payload = WsPayload {                                                                                  |
| event: 'newBroadcast'.into(),                                                                          |
| data: serde_json::to_value(&BroadcastDto::from(broadcast_clone)).unwrap(),                             |
| }                                                                                                      |
| state_clone.ws_hub.send_to_users(&subscriber_ids, Arc::new(payload)).await                             |
|                                                                                                        |
| // 5d. Update live broadcast count for all connected clients                                           |
| count = broadcast_repo.count_active(&state_clone.db).await.unwrap_or(0)                                |
| state_clone.ws_hub.broadcast_all(WsPayload {                                                           |
| event: 'numberOfLiveBroadcasts'.into(),                                                                |
| data: json!({ 'count': count }),                                                                       |
| }).await                                                                                               |
| })                                                                                                     |
|                                                                                                        |
| // 6. Return session immediately                                                                       |
| return Ok(BroadcastSessionDto {                                                                        |
| broadcast: BroadcastDto::from(broadcast),                                                              |
| livekit_token: host_token,                                                                             |
| livekit_url: state.config.livekit_host.clone(),                                                        |
| })                                                                                                     |

## **6.3 join()**

| ASYNC FUNCTION join(broadcast_id, user_id, state) -> Result<BroadcastSessionDto>      |
| ------------------------------------------------------------------------------------- |
|                                                                                       |
| broadcast = broadcast_repo.find_by_id(broadcast_id, &db).await?                       |
| .ok_or(AppError::NotFound('Broadcast not found'))?                                    |
|                                                                                       |
| if broadcast.status != BroadcastStatus::Active {                                      |
| return Err(AppError::BadRequest('Broadcast is not currently live'))                   |
| }                                                                                     |
| if broadcast.deleted_at.is_some() {                                                   |
| return Err(AppError::NotFound('Broadcast not found'))                                 |
| }                                                                                     |
|                                                                                       |
| user = user_repo.find_by_id(user_id, &db).await?                                      |
| .ok_or(AppError::NotFound('User not found'))?                                         |
|                                                                                       |
| // Determine role: check if user is a cohost                                          |
| is_cohost = broadcast_repo.is_cohost(broadcast_id, user_id, &db).await?               |
| is_creator = broadcast.creator_id == user_id                                          |
| role = if is_creator { BroadcastRole::Host }                                          |
| else if is_cohost { BroadcastRole::Cohost }                                           |
| else { BroadcastRole::Listener }                                                      |
|                                                                                       |
| // Mint token BEFORE DB write (same atomicity principle)                              |
| token = livekit_service.mint_token(user_id, user.full_name, broadcast_id, role)?      |
|                                                                                       |
| // Upsert listener record (ON CONFLICT DO UPDATE joined_at, role)                     |
| broadcast_repo.upsert_listener(UpsertListenerInput {                                  |
| listener_id: user_id,                                                                 |
| broadcast_id,                                                                         |
| role,                                                                                 |
| joined_at: Utc::now(),                                                                |
| }, &db).await?                                                                        |
|                                                                                       |
| // Notify host/cohosts via WS (fire-and-forget)                                       |
| tokio::spawn(async move {                                                             |
| host_cohost_ids = broadcast_repo                                                      |
| .get_host_and_cohost_ids(broadcast_id, &db).await                                     |
| payload = WsPayload {                                                                 |
| event: 'newBroadcastListener'.into(),                                                 |
| data: serde_json::to_value(UserSummaryDto::from(user)).unwrap(),                      |
| }                                                                                     |
| state.ws_hub.send_to_users(&host_cohost_ids, Arc::new(payload)).await                 |
|                                                                                       |
| // Update live listener count                                                         |
| count = broadcast_repo.count_listeners(broadcast_id, &db).await.unwrap_or(0)          |
| all_participant_ids = broadcast_repo.get_all_participant_ids(broadcast_id, &db).await |
| state.ws_hub.send_to_users(&all_participant_ids, Arc::new(WsPayload {                 |
| event: 'numberOfLiveListeners'.into(),                                                |
| data: json!({ 'broadcastId': broadcast_id, 'count': count }),                         |
| })).await                                                                             |
| })                                                                                    |
|                                                                                       |
| return Ok(BroadcastSessionDto {                                                       |
| broadcast: BroadcastDto::from(broadcast),                                             |
| livekit_token: token,                                                                 |
| livekit_url: state.config.livekit_host.clone(),                                       |
| })                                                                                    |

## **6.4 end_broadcast()**

| ASYNC FUNCTION end_broadcast(broadcast_id, user_id, reason: EndReason, state)      |
| ---------------------------------------------------------------------------------- |
|                                                                                    |
| broadcast = broadcast_repo.find_by_id(broadcast_id, &db).await?                    |
| .ok_or(AppError::NotFound)?                                                        |
|                                                                                    |
| // Auth check: creator OR admin                                                    |
| if broadcast.creator_id != user_id && !user_is_admin(user_id, &db).await {         |
| return Err(AppError::Forbidden('Not authorised to end this broadcast'))            |
| }                                                                                  |
|                                                                                    |
| if broadcast.status != BroadcastStatus::Active {                                   |
| return Ok(()) // Idempotent: already ended                                         |
| }                                                                                  |
|                                                                                    |
| // 1. Get all participant IDs BEFORE clearing DB (for WS notification)             |
| participant_ids = broadcast_repo.get_all_participant_ids(broadcast_id, &db).await? |
|                                                                                    |
| // 2. DB transaction: set inactive, clear listener rows                            |
| let mut tx = db.begin().await?                                                     |
| broadcast_repo.set_inactive(broadcast_id, &mut tx).await?                          |
| broadcast_repo.clear_all_listeners(broadcast_id, &mut tx).await?                   |
| tx.commit().await?                                                                 |
|                                                                                    |
| // 3. Delete LiveKit room (kicks everyone from media layer)                        |
| livekit_service.delete_room(broadcast_id).await?                                   |
| // Log warning if this fails — do NOT fail the whole operation                     |
| // LiveKit auto-cleans empty rooms anyway                                          |
|                                                                                    |
| // 4. Emit 'endedBroadcast' to all participants                                    |
| payload = WsPayload {                                                              |
| event: 'endedBroadcast'.into(),                                                    |
| data: serde_json::to_value(BroadcastEndedPayload {                                 |
| broadcast_id,                                                                      |
| reason,                                                                            |
| }).unwrap(),                                                                       |
| }                                                                                  |
| state.ws_hub.send_to_users(&participant_ids, Arc::new(payload)).await              |
|                                                                                    |
| // 5. Update live count                                                            |
| count = broadcast_repo.count_active(&db).await.unwrap_or(0)                        |
| state.ws_hub.broadcast_all(WsPayload {                                             |
| event: 'numberOfLiveBroadcasts'.into(),                                            |
| data: json!({ 'count': count }),                                                   |
| }).await                                                                           |

## **6.5 leave_broadcast()**

| ASYNC FUNCTION leave_broadcast(broadcast_id, user_id, state)                                  |
| --------------------------------------------------------------------------------------------- |
|                                                                                               |
| // Delete listener record (no error if row doesn't exist — idempotent)                        |
| broadcast_repo.remove_listener(broadcast_id, user_id, &db).await?                             |
|                                                                                               |
| // Notify host/cohosts                                                                        |
| host_cohost_ids = broadcast_repo.get_host_and_cohost_ids(broadcast_id, &db).await?            |
| payload = WsPayload {                                                                         |
| event: 'broadcastListenerLeft'.into(),                                                        |
| data: json!({ 'userId': user_id, 'broadcastId': broadcast_id }),                              |
| }                                                                                             |
| state.ws_hub.send_to_users(&host_cohost_ids, Arc::new(payload)).await                         |
|                                                                                               |
| // Update listener count for all participants                                                 |
| count = broadcast_repo.count_listeners(broadcast_id, &db).await.unwrap_or(0)                  |
| all_ids = broadcast_repo.get_all_participant_ids(broadcast_id, &db).await.unwrap_or_default() |
| state.ws_hub.send_to_users(&all_ids, Arc::new(WsPayload {                                     |
| event: 'numberOfLiveListeners'.into(),                                                        |
| data: json!({ 'broadcastId': broadcast_id, 'count': count }),                                 |
| })).await                                                                                     |

## **6.6 add_cohost() ****&**** remove_cohost()**

| ASYNC FUNCTION add_cohost(broadcast_id, requester_id, cohost_user_id, state)                |
| ------------------------------------------------------------------------------------------- |
| -> Result<CohostSessionDto>                                                                 |
|                                                                                             |
| broadcast = broadcast_repo.find_by_id(broadcast_id).await?.ok_or(NotFound)?                 |
| if broadcast.creator_id != requester_id { return Err(Forbidden) }                           |
|                                                                                             |
| cohost_user = user_repo.find_by_id(cohost_user_id).await?.ok_or(NotFound)?                  |
|                                                                                             |
| // Insert cohost record                                                                     |
| broadcast_repo.add_cohost(broadcast_id, cohost_user_id, &db).await?                         |
|                                                                                             |
| // Create a notification for the new cohost                                                 |
| notification_service.create(                                                                |
| NotificationType::AddedAsCoHost,                                                            |
| owner_id: cohost_user_id,                                                                   |
| broadcast_id: Some(broadcast_id),                                                           |
| state                                                                                       |
| ).await?                                                                                    |
|                                                                                             |
| // If broadcast is currently live: mint a COHOST token and notify via WS                    |
| if broadcast.status == BroadcastStatus::Active {                                            |
| token = livekit_service.mint_token(                                                         |
| cohost_user_id, cohost_user.full_name, broadcast_id, BroadcastRole::Cohost                  |
| )?                                                                                          |
|                                                                                             |
| // Also upsert them into broadcast_listeners with COHOST role                               |
| broadcast_repo.upsert_listener(UpsertListenerInput {                                        |
| listener_id: cohost_user_id, broadcast_id,                                                  |
| role: BroadcastRole::Cohost, joined_at: Utc::now(),                                         |
| }, &db).await?                                                                              |
|                                                                                             |
| // Emit 'newCohost' to the targeted user (they need the token)                              |
| state.ws_hub.send_to_user(cohost_user_id, Arc::new(WsPayload {                              |
| event: 'newCohost'.into(),                                                                  |
| data: json!({                                                                               |
| 'broadcastId': broadcast_id,                                                                |
| 'livekitToken': token,                                                                      |
| 'livekitUrl': state.config.livekit_host,                                                    |
| }),                                                                                         |
| })).await                                                                                   |
|                                                                                             |
| return Ok(CohostSessionDto { user: cohost_user.into(), livekit_token: token, ... })         |
| }                                                                                           |
|                                                                                             |
| // Broadcast not live yet — just return user info, no token                                 |
| return Ok(CohostSessionDto { user: cohost_user.into(), livekit_token: String::new(), ... }) |
|                                                                                             |
| ─────────────────────────────────────────────────────                                       |
|                                                                                             |
| ASYNC FUNCTION remove_cohost(broadcast_id, requester_id, cohost_id, state)                  |
|                                                                                             |
| broadcast = broadcast_repo.find_by_id(broadcast_id).await?.ok_or(NotFound)?                 |
| if broadcast.creator_id != requester_id { return Err(Forbidden) }                           |
|                                                                                             |
| broadcast_repo.remove_cohost(broadcast_id, cohost_id, &db).await?                           |
| broadcast_repo.remove_listener(broadcast_id, cohost_id, &db).await?                         |
|                                                                                             |
| // If broadcast is live: kick from LiveKit room                                             |
| if broadcast.status == BroadcastStatus::Active {                                            |
| livekit_service.remove_participant(broadcast_id, cohost_id).await                           |
| // Warn on failure; do not propagate                                                        |
| }                                                                                           |
|                                                                                             |
| // Notify the removed cohost via WS                                                         |
| state.ws_hub.send_to_user(cohost_id, Arc::new(WsPayload {                                   |
| event: 'removedCohost'.into(),                                                              |
| data: json!({ 'broadcastId': broadcast_id }),                                               |
| })).await                                                                                   |

## **6.7 on_host_disconnected() ****&**** Grace Period**

When the host's WebSocket drops, we do NOT immediately end the broadcast. A 60-second grace period allows reconnection. This is critical for handling brief network interruptions on mobile.

| ASYNC FUNCTION on_host_disconnected(broadcast_id, state)                           |
| ---------------------------------------------------------------------------------- |
|                                                                                    |
| participant_ids = broadcast_repo.get_all_participant_ids(broadcast_id, &db).await? |
|                                                                                    |
| // Notify listeners that host dropped (so they show 'reconnecting...' UI)          |
| state.ws_hub.send_to_users(&participant_ids, Arc::new(WsPayload {                  |
| event: 'hostDisconnected'.into(),                                                  |
| data: json!({ 'broadcastId': broadcast_id }),                                      |
| })).await                                                                          |
|                                                                                    |
| // Store grace period flag in Redis                                                |
| // Key: 'host_grace:{broadcast_id}', TTL: 60 seconds                               |
| state.redis.set_ex('host_grace:{broadcast_id}', 'pending', 60).await?              |
|                                                                                    |
| // Spawn a delayed task: check after 60s if host reconnected                       |
| tokio::spawn(async move {                                                          |
| tokio::time::sleep(Duration::from_secs(60)).await                                  |
|                                                                                    |
| // If key still exists, host never reconnected — end the broadcast                 |
| if state.redis.exists('host_grace:{broadcast_id}').await.unwrap_or(false) {        |
| state.redis.del('host_grace:{broadcast_id}').await.ok()                            |
| end_broadcast(broadcast_id, broadcast.creator_id,                                  |
| EndReason::HostDisconnected, state).await.ok()                                     |
| }                                                                                  |
| // Otherwise: host reconnected; on_host_reconnected() cleared the key              |
| })                                                                                 |
|                                                                                    |
| ─────────────────────────────────────────────────────                              |
|                                                                                    |
| ASYNC FUNCTION on_host_reconnected(user_id, state)                                 |
| // Called from handle_socket when same user registers to ws_hub again              |
| // and they were previously the host of an active broadcast                        |
|                                                                                    |
| broadcast = broadcast_repo.find_active_hosted_by(user_id, &db).await?              |
| if let Some(b) = broadcast {                                                       |
| // Clear grace period                                                              |
| state.redis.del('host_grace:{b.id}').await.ok()                                    |
|                                                                                    |
| participant_ids = broadcast_repo.get_all_participant_ids(b.id, &db).await?         |
| state.ws_hub.send_to_users(&participant_ids, Arc::new(WsPayload {                  |
| event: 'hostReconnected'.into(),                                                   |
| data: json!({ 'broadcastId': b.id }),                                              |
| })).await                                                                          |
| }                                                                                  |

# **7. Repository Layer — repository.rs**

All SQL queries live here. No business logic. Functions take a PgPool or &mut PgTransaction reference. Return domain models (not DTOs).

## **7.1 Core Repository Functions**

| FUNCTION create(input: CreateBroadcastInput, db) -> Result<Broadcast>                              |
| -------------------------------------------------------------------------------------------------- |
| // INSERT INTO broadcasts (...) VALUES (...) RETURNING *                                           |
| // Returns the full Broadcast row.                                                                 |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION find_by_id(id, db) -> Result<Option<Broadcast>>                                           |
| // SELECT * FROM broadcasts WHERE id = $1 AND deleted_at IS NULL                                   |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION set_active(id, token, tx) -> Result<Broadcast>                                            |
| // UPDATE broadcasts                                                                               |
| // SET status = 'active', broadcast_token = $2                                                     |
| // WHERE id = $1                                                                                   |
| // RETURNING *                                                                                     |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION set_inactive(id, tx) -> Result<()>                                                        |
| // UPDATE broadcasts                                                                               |
| // SET status = 'inactive', broadcast_token = NULL, end_time = now()                               |
| // WHERE id = $1                                                                                   |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION update(id, input: UpdateBroadcastInput, db) -> Result<Broadcast>                          |
| // Build dynamic UPDATE using sqlx QueryBuilder.                                                   |
| // Only include fields that are Some(). Pattern:                                                   |
| // let mut builder = QueryBuilder::new('UPDATE broadcasts SET');                                   |
| // if let Some(t) = input.title { builder.push(' title = ').push_bind(t); }                        |
| // ...etc...                                                                                       |
| // builder.push(' WHERE id = ').push_bind(id);                                                     |
| // builder.push(' RETURNING *');                                                                   |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION soft_delete(id, db) -> Result<()>                                                         |
| // UPDATE broadcasts SET deleted_at = now() WHERE id = $1                                          |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION list(filters: BroadcastFilters, pagination, db) -> Result<(Vec<Broadcast>, i64)>          |
| // Returns (rows, total_count) for pagination.                                                     |
| // Builds dynamic query based on filters:                                                          |
| // Base: SELECT b.* FROM broadcasts b WHERE b.deleted_at IS NULL                                   |
| // + if status: AND b.status = $N                                                                  |
| // + if creator_id: AND b.creator_id = $N                                                          |
| // + if only_subscriptions: JOIN user_subscribers us ON us.subscription_id = b.creator_id          |
| // AND us.subscriber_id = $N (viewer_id)                                                           |
| // + if keywords: AND to_tsvector('english', b.title ││ ' ' ││ b.description)                      |
| // @@ plainto_tsquery('english', $N)                                                               |
| // ORDER BY b.created_at DESC                                                                      |
| // LIMIT $N OFFSET $N                                                                              |
| //                                                                                                 |
| // Run COUNT(*) with same WHERE (minus LIMIT/OFFSET) for total.                                    |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION count_active(db) -> Result<i64>                                                           |
| // SELECT COUNT(*) FROM broadcasts WHERE status = 'active' AND deleted_at IS NULL                  |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION find_active_hosted_by(user_id, db) -> Result<Option<Broadcast>>                           |
| // SELECT b.* FROM broadcasts b                                                                    |
| // WHERE b.creator_id = $1 AND b.status = 'active' AND b.deleted_at IS NULL                        |
| // LIMIT 1                                                                                         |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION upsert_listener(input: UpsertListenerInput, db) -> Result<()>                             |
| // INSERT INTO broadcast_listeners (listener_id, broadcast_id, joined_at, role)                    |
| // VALUES ($1, $2, $3, $4)                                                                         |
| // ON CONFLICT (listener_id, broadcast_id)                                                         |
| // DO UPDATE SET joined_at = EXCLUDED.joined_at, role = EXCLUDED.role                              |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION remove_listener(broadcast_id, listener_id, db) -> Result<()>                              |
| // DELETE FROM broadcast_listeners                                                                 |
| // WHERE broadcast_id = $1 AND listener_id = $2                                                    |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION clear_all_listeners(broadcast_id, tx) -> Result<()>                                       |
| // DELETE FROM broadcast_listeners WHERE broadcast_id = $1                                         |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION get_all_participant_ids(broadcast_id, db) -> Result<Vec<Uuid>>                            |
| // SELECT listener_id FROM broadcast_listeners WHERE broadcast_id = $1                             |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION get_host_and_cohost_ids(broadcast_id, db) -> Result<Vec<Uuid>>                            |
| // SELECT listener_id FROM broadcast_listeners                                                     |
| // WHERE broadcast_id = $1 AND role IN ('HOST', 'COHOST')                                          |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION count_listeners(broadcast_id, db) -> Result<i64>                                          |
| // SELECT COUNT(*) FROM broadcast_listeners WHERE broadcast_id = $1                                |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION get_listeners_paginated(broadcast_id, pagination, db) -> Result<(Vec<UserWithRole>, i64)> |
| // JOIN broadcast_listeners with users table to get user details                                   |
| // SELECT u.id, u.full_name, u.image_url, bl.role, bl.joined_at                                    |
| // FROM broadcast_listeners bl                                                                     |
| // JOIN users u ON u.id = bl.listener_id                                                           |
| // WHERE bl.broadcast_id = $1                                                                      |
| // ORDER BY bl.joined_at ASC                                                                       |
| // LIMIT $2 OFFSET $3                                                                              |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION add_cohost(broadcast_id, user_id, db) -> Result<()>                                       |
| // INSERT INTO broadcast_cohosts (broadcast_id, cohost_id) VALUES ($1, $2)                         |
| // ON CONFLICT DO NOTHING                                                                          |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION remove_cohost(broadcast_id, user_id, db) -> Result<()>                                    |
| // DELETE FROM broadcast_cohosts WHERE broadcast_id = $1 AND cohost_id = $2                        |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION is_cohost(broadcast_id, user_id, db) -> Result<bool>                                      |
| // SELECT EXISTS(                                                                                  |
| // SELECT 1 FROM broadcast_cohosts                                                                 |
| // WHERE broadcast_id = $1 AND cohost_id = $2                                                      |
| // )                                                                                               |
|                                                                                                    |
| ─────────────────────────────────────────────────────                                              |
|                                                                                                    |
| FUNCTION get_cohosts_with_users(broadcast_id, db) -> Result<Vec<User>>                             |
| // SELECT u.* FROM broadcast_cohosts bc                                                            |
| // JOIN users u ON u.id = bc.cohost_id                                                             |
| // WHERE bc.broadcast_id = $1                                                                      |

# **8. Utility Functions ****&**** Additional Implementations**

## **8.1 WsHub — Full Implementation Guide**

| pub struct WsHub {                                                                 |
| ---------------------------------------------------------------------------------- |
| clients: DashMap<Uuid, Vec<ConnectionSender>>,                                     |
| conn_seq: AtomicUsize, // monotonically increasing connection ID                   |
| }                                                                                  |
|                                                                                    |
| FUNCTION register(user_id, sender) -> usize                                        |
| // Assigns a unique connection ID, pushes sender into Vec for this user.           |
| // Returns the connection_id so the caller can unregister by ID later.             |
| conn_id = conn_seq.fetch_add(1, Ordering::Relaxed)                                 |
| clients.entry(user_id).or_default().push(ConnectionSender { id: conn_id, sender }) |
| return conn_id                                                                     |
|                                                                                    |
| FUNCTION unregister(user_id, conn_id)                                              |
| // Removes the specific sender from the Vec.                                       |
| // If Vec is now empty, removes the user entry entirely.                           |
| if let Some(mut senders) = clients.get_mut(&user_id) {                             |
| senders.retain(│s│ s.id != conn_id)                                                |
| }                                                                                  |
| if clients.get(&user_id).map(│v│ v.is_empty()).unwrap_or(false) {                  |
| clients.remove(&user_id)                                                           |
| }                                                                                  |
|                                                                                    |
| ASYNC FUNCTION send_to_user(user_id, payload: Arc<WsPayload>)                      |
| // Sends to ALL connections for this user (web + mobile).                          |
| // Dead senders are silently ignored (they'll be cleaned up on disconnect).        |
| if let Some(senders) = clients.get(&user_id) {                                     |
| for sender in senders.iter() {                                                     |
| sender.sender.send(Arc::clone(&payload)).await.ok()                                |
| }                                                                                  |
| }                                                                                  |
|                                                                                    |
| ASYNC FUNCTION send_to_users(user_ids: &[Uuid], payload: Arc<WsPayload>)           |
| // Iterates and calls send_to_user for each. The Arc means one allocation          |
| // per event, not one per recipient.                                               |
| for id in user_ids { send_to_user(*id, Arc::clone(&payload)).await }               |
|                                                                                    |
| ASYNC FUNCTION broadcast_all(payload: WsPayload)                                   |
| // Sends to every connected client.                                                |
| // Used for global counts (numberOfLiveBroadcasts).                                |
| let payload = Arc::new(payload)                                                    |
| for entry in clients.iter() {                                                      |
| for sender in entry.value().iter() {                                               |
| sender.sender.send(Arc::clone(&payload)).await.ok()                                |
| }                                                                                  |
| }                                                                                  |
|                                                                                    |
| FUNCTION connection_count(user_id) -> usize                                        |
| // Returns number of active WS connections for a user.                             |
| clients.get(&user_id).map(│v│ v.len()).unwrap_or(0)                                |
|                                                                                    |
| FUNCTION is_online(user_id) -> bool                                                |
| // Checks if user has any active WS connection.                                    |
| clients.contains_key(&user_id)                                                     |

## **8.2 Broadcast Draft Handling (Flutter FE Storage → BE Migration)**

Currently you handle drafts on the Flutter side. Here is the recommended migration to persist drafts in the backend, which enables cross-device sync and crash recovery.

| // No schema change needed — drafts ARE inactive broadcasts.                   |
| ------------------------------------------------------------------------------ |
| // The difference between a 'draft' and a 'scheduled' broadcast                |
| // is whether start_time is set.                                               |
|                                                                                |
| // From the API perspective:                                                   |
| // POST /broadcasts → creates a draft (status=inactive, no start_time)         |
| // POST /broadcasts → creates scheduled (status=inactive, start_time set)      |
| // PUT /broadcasts/:id/go-live → makes it live (status=active)                 |
|                                                                                |
| // In the list endpoint, add a 'mine' query param:                             |
| // GET /broadcasts?creatorId={me}&status=inactive                              |
| // Returns all drafts/scheduled for the current user.                          |
|                                                                                |
| // Flutter app changes:                                                        |
| // 1. Remove local draft storage (SharedPreferences/Hive for broadcast drafts) |
| // 2. Call POST /broadcasts when user starts creating (with partial data)      |
| // 3. Call PATCH /broadcasts/:id as user edits fields (debounced)              |
| // 4. Draft list screen calls GET /broadcasts?creatorId=me&status=inactive     |
| // 5. 'Go Live' button calls PUT /broadcasts/:id/go-live                       |
|                                                                                |
| // Auto-save pattern for Flutter:                                              |
| // - Create draft on screen open (POST /broadcasts with empty/default values)  |
| // - Debounce PATCH calls (500ms after last keystroke)                         |
| // - Store broadcast_id in local state only (not draft content)                |
| // - On screen close: if never went live AND is empty → DELETE /broadcasts/:id |

## **8.3 LiveKit Webhook Handler**

LiveKit sends webhooks to your backend for room events. This is important for detecting when participants drop at the media layer (independent of your WS connection).

| // POST /webhooks/livekit                                                        |
| -------------------------------------------------------------------------------- |
| // Register this URL in LiveKit Cloud dashboard.                                 |
|                                                                                  |
| HANDLER livekit_webhook(headers, body: Bytes, State(state))                      |
|                                                                                  |
| // 1. Verify the webhook signature                                               |
| auth_token = headers.get('Authorization')?                                       |
| livekit_service.verify_webhook(auth_token, &body)?                               |
| ELSE return 401                                                                  |
|                                                                                  |
| // 2. Parse the event                                                            |
| event: WebhookEvent = serde_json::from_slice(&body)?                             |
|                                                                                  |
| match event.event.as_str() {                                                     |
| 'room_finished' => {                                                             |
| // LiveKit auto-ended the room (e.g. empty timeout)                              |
| broadcast_id = parse_uuid(&event.room.name)?                                     |
| broadcast_service.on_room_finished(broadcast_id, state).await.ok()               |
| }                                                                                |
| 'participant_left' => {                                                          |
| // A participant dropped at the media layer (network loss, etc.)                 |
| // Note: this is complementary to your WS disconnect handler.                    |
| // The WS disconnect handles your app-layer events;                              |
| // this handles pure media-layer drops.                                          |
| broadcast_id = parse_uuid(&event.room.name)?                                     |
| user_id = parse_uuid(&event.participant.identity)?                               |
| // Only log; your WS hub already handles the app-layer cleanup.                  |
| tracing::info!('LiveKit participant {} left room {}', user_id, broadcast_id)     |
| }                                                                                |
| _ => {} // Ignore other events                                                   |
| }                                                                                |
|                                                                                  |
| return 200                                                                       |
|                                                                                  |
| ─────────────────────────────────────────────────────                            |
|                                                                                  |
| ASYNC FUNCTION on_room_finished(broadcast_id, state)                             |
| // Same as end_broadcast but triggered by LiveKit webhook.                       |
| // Check if broadcast is still active in DB before acting.                       |
| broadcast = broadcast_repo.find_by_id(broadcast_id, &db).await?                  |
| if let Some(b) = broadcast {                                                     |
| if b.status == BroadcastStatus::Active {                                         |
| end_broadcast(b.id, b.creator_id, EndReason::HostDisconnected, state).await.ok() |
| }                                                                                |
| }                                                                                |

## **8.4 Token Refresh Endpoint**

LiveKit tokens expire. The FE should proactively refresh tokens before they expire. A 6-hour TTL means this is rarely needed, but it must exist.

| // POST /broadcasts/:id/token                                                             |
| ----------------------------------------------------------------------------------------- |
| // FE calls this when: token_expiry - now() < 5 minutes                                   |
|                                                                                           |
| ASYNC FUNCTION refresh_token(broadcast_id, user_id, state) -> Result<BroadcastSessionDto> |
|                                                                                           |
| broadcast = broadcast_repo.find_by_id(broadcast_id, &db).await?.ok_or(NotFound)?          |
| if broadcast.status != BroadcastStatus::Active {                                          |
| return Err(AppError::BadRequest('Broadcast is not live'))                                 |
| }                                                                                         |
|                                                                                           |
| user = user_repo.find_by_id(user_id, &db).await?.ok_or(NotFound)?                         |
|                                                                                           |
| // Determine current role from broadcast_listeners table                                  |
| listener = broadcast_repo.find_listener(broadcast_id, user_id, &db).await?                |
| .ok_or(AppError::BadRequest('You are not in this broadcast'))?                            |
|                                                                                           |
| new_token = livekit_service.mint_token(                                                   |
| user_id, user.full_name, broadcast_id, listener.role                                      |
| )?                                                                                        |
|                                                                                           |
| return Ok(BroadcastSessionDto {                                                           |
| broadcast: BroadcastDto::from(broadcast),                                                 |
| livekit_token: new_token,                                                                 |
| livekit_url: state.config.livekit_host.clone(),                                           |
| })                                                                                        |

## **8.5 Background Job — Scheduled Broadcasts**

| // modules/broadcast/jobs.rs                                                        |
| ----------------------------------------------------------------------------------- |
| // Uses apalis for job scheduling backed by Postgres.                               |
| // This runs as a separate worker in the same process.                              |
|                                                                                     |
| #[derive(Serialize, Deserialize)]                                                   |
| pub struct NotifyScheduledBroadcastJob {                                            |
| pub broadcast_id: Uuid,                                                             |
| }                                                                                   |
|                                                                                     |
| ASYNC FUNCTION execute(job: NotifyScheduledBroadcastJob, ctx: JobContext)           |
| // This job fires at the scheduled start_time.                                      |
| // It does NOT start the broadcast — that requires the creator to press go-live.    |
| // It only sends a 'starting soon' notification to subscribers.                     |
|                                                                                     |
| state = ctx.data::<Arc<AppState>>()?                                                |
| broadcast = broadcast_repo.find_by_id(job.broadcast_id, &state.db).await?           |
|                                                                                     |
| if let Some(b) = broadcast {                                                        |
| if b.status == BroadcastStatus::Inactive {                                          |
| subscriber_ids = subscriber_repo.get_subscriber_ids(b.creator_id, &state.db).await? |
| notification_service.create_bulk(                                                   |
| NotificationType::ScheduledBroadcast,                                               |
| b.id,                                                                               |
| subscriber_ids,                                                                     |
| &state                                                                              |
| ).await?                                                                            |
| }                                                                                   |
| }                                                                                   |
|                                                                                     |
| FUNCTION schedule_broadcast_start_job(broadcast_id, start_time, state)              |
| // Creates an apalis job scheduled for start_time.                                  |
| // If start_time changes (PATCH /broadcasts/:id), cancel old job and reschedule.    |
| // Use broadcast_id as the job ID so it can be found and cancelled.                 |
| // Store job_id in Redis: 'broadcast_job:{broadcast_id}' -> job_id                  |

# **9. Frontend Integration Guide**

## **9.1 Flutter — WebSocket Connection**

| // Flutter: Connect to Axum WebSocket                                  |
| ---------------------------------------------------------------------- |
| // Package: web_socket_channel ^2.4                                    |
|                                                                        |
| class MenoWebSocket {                                                  |
| late WebSocketChannel _channel                                         |
| final StreamController _eventController = StreamController.broadcast() |
|                                                                        |
| Future<void> connect(String baseUrl, String accessToken) async {       |
| // Connect with token in query string (matches Postman collection)     |
| final wsUrl = baseUrl                                                  |
| .replaceFirst('https', 'wss')                                          |
| .replaceFirst('http', 'ws')                                            |
| _channel = WebSocketChannel.connect(                                   |
| Uri.parse('$wsUrl/ws?token=$accessToken')                              |
| )                                                                      |
| _channel.stream.listen(                                                |
| (raw) {                                                                |
| final msg = jsonDecode(raw) as Map<String, dynamic>                    |
| final event = msg['event'] as String                                   |
| final data = msg['data']                                               |
| _eventController.add(WsEvent(event: event, data: data))                |
| },                                                                     |
| onDone: () => _reconnect(), // implement exponential backoff           |
| onError: (e) => _reconnect(),                                          |
| )                                                                      |
| }                                                                      |
|                                                                        |
| // Subscribe to specific events                                        |
| Stream<dynamic> on(String eventName) =>                                |
| _eventController.stream                                                |
| .where((e) => e.event == eventName)                                    |
| .map((e) => e.data)                                                    |
|                                                                        |
| // Send messages to backend (endBroadcast, leaveBroadcast, etc.)       |
| void emit(String event, Map<String, dynamic> data) {                   |
| _channel.sink.add(jsonEncode({ 'event': event, 'data': data }))        |
| }                                                                      |
| }                                                                      |
|                                                                        |
| // Usage in broadcast screen:                                          |
| ws.on('endedBroadcast').listen((data) {                                |
| final broadcastId = data['broadcastId']                                |
| final reason = data['reason'] // 'Normal' │ 'HostDisconnected'         |
| // Navigate away, show reason to user                                  |
| })                                                                     |
|                                                                        |
| ws.on('newBroadcastListener').listen((data) {                          |
| final user = UserSummaryDto.fromJson(data)                             |
| // Add to UI listener list                                             |
| })                                                                     |
|                                                                        |
| // End broadcast (host only):                                          |
| ws.emit('endBroadcast', { 'broadcastId': broadcast.id })               |
|                                                                        |
| // Leave broadcast (listener):                                         |
| ws.emit('leaveBroadcast', { 'broadcastId': broadcast.id })             |

## **9.2 Flutter — LiveKit Integration**

| // Package: livekit_client ^2.x (official Flutter SDK)                    |
| ------------------------------------------------------------------------- |
|                                                                           |
| // After receiving BroadcastSessionDto from go-live or join endpoint:     |
| Future<void> connectToLiveKit(BroadcastSessionDto session) async {        |
| final room = Room()                                                       |
| final listener = room.createListener()                                    |
|                                                                           |
| await room.connect(                                                       |
| session.livekitUrl, // wss://your-project.livekit.cloud                   |
| session.livekitToken, // JWT from backend                                 |
| roomOptions: const RoomOptions(                                           |
| adaptiveStream: true,                                                     |
| dynacast: true,                                                           |
| ),                                                                        |
| )                                                                         |
|                                                                           |
| // For HOST/COHOST: enable microphone                                     |
| if (userRole == BroadcastRole.host ││ userRole == BroadcastRole.cohost) { |
| await room.localParticipant?.setMicrophoneEnabled(true)                   |
| }                                                                         |
|                                                                           |
| // Listen for participant events (optional — for UI updates)              |
| listener                                                                  |
| ..on<RoomConnectedEvent>((e) { /* room ready */ })                        |
| ..on<ParticipantConnectedEvent>((e) { /* new participant */ })            |
| ..on<ParticipantDisconnectedEvent>((e) { /* participant left */ })        |
|                                                                           |
| // NO need to emit WS events here — the BE already handled everything     |
| // via the HTTP response flow                                             |
| }                                                                         |
|                                                                           |
| // Token refresh: monitor expiry                                          |
| Timer.periodic(Duration(minutes: 1), (timer) async {                      |
| final expiresIn = jwtExpiryFromToken(currentToken) - DateTime.now()       |
| if (expiresIn.inMinutes < 5) {                                            |
| final session = await api.refreshBroadcastToken(broadcastId)              |
| await room.refreshToken(session.livekitToken)                             |
| }                                                                         |
| })                                                                        |

## **9.3 Next.js — WebSocket Connection**

| // hooks/useWebSocket.ts                                                                         |
| ------------------------------------------------------------------------------------------------ |
| // Package: native browser WebSocket (no library needed)                                         |
|                                                                                                  |
| export function useMenoWebSocket(accessToken: string) {                                          |
| const wsRef = useRef<WebSocket │ null>(null)                                                     |
| const listenersRef = useRef<Map<string, Set<Function>>>(new Map())                               |
|                                                                                                  |
| useEffect(() => {                                                                                |
| const url = `${process.env.NEXT_PUBLIC_API_URL.replace('https', 'wss')}/ws?token=${accessToken}` |
| const ws = new WebSocket(url)                                                                    |
|                                                                                                  |
| ws.onmessage = (e) => {                                                                          |
| const msg = JSON.parse(e.data) as { event: string; data: unknown }                               |
| listenersRef.current.get(msg.event)?.forEach(fn => fn(msg.data))                                 |
| }                                                                                                |
| ws.onclose = () => { /* reconnect with backoff */ }                                              |
| wsRef.current = ws                                                                               |
| return () => ws.close()                                                                          |
| }, [accessToken])                                                                                |
|                                                                                                  |
| const on = (event: string, callback: Function) => {                                              |
| if (!listenersRef.current.has(event)) {                                                          |
| listenersRef.current.set(event, new Set())                                                       |
| }                                                                                                |
| listenersRef.current.get(event)!.add(callback)                                                   |
| return () => listenersRef.current.get(event)?.delete(callback)                                   |
| }                                                                                                |
|                                                                                                  |
| const emit = (event: string, data: unknown) => {                                                 |
| wsRef.current?.send(JSON.stringify({ event, data }))                                             |
| }                                                                                                |
|                                                                                                  |
| return { on, emit }                                                                              |
| }                                                                                                |
|                                                                                                  |
| // Next.js LiveKit:                                                                              |
| // Package: @livekit/components-react or livekit-client                                          |
| import { Room } from 'livekit-client'                                                            |
|                                                                                                  |
| const room = new Room()                                                                          |
| await room.connect(session.livekitUrl, session.livekitToken)                                     |
| // For host:                                                                                     |
| await room.localParticipant.setMicrophoneEnabled(true)                                           |

# **10. Updated Crate Versions ****&**** Cargo.toml**

| # apps/api/Cargo.toml — Broadcast Module Dependencies                       |
| --------------------------------------------------------------------------- |
| [dependencies]                                                              |
|                                                                             |
| # LiveKit server SDK (token minting + room management API)                  |
| # Do NOT add the 'livekit' crate — that is for Rust media clients.          |
| livekit-api = "0.4"                                                         |
|                                                                             |
| # WebSocket (built into Axum — no extra crate needed)                       |
| axum = { version = "0.7", features = ["ws", "multipart", "macros"] }        |
|                                                                             |
| # Concurrent HashMap for WsHub (zero-lock reads)                            |
| dashmap = "6"                                                               |
|                                                                             |
| # Atomic operations for connection IDs                                      |
| # Built into std — no extra crate needed                                    |
|                                                                             |
| # Background jobs (scheduled broadcasts, notification fans)                 |
| apalis = { version = "0.6", features = ["postgres", "tokio-comp"] }         |
| apalis-sql = "0.6"                                                          |
|                                                                             |
| # Validation on DTOs                                                        |
| validator = { version = "0.18", features = ["derive"] }                     |
|                                                                             |
| # Timezone handling                                                         |
| chrono = { version = "0.4", features = ["serde"] }                          |
|                                                                             |
| # Redis (for grace period, presence, pub/sub bridge)                        |
| fred = { version = "9", features = ["tokio-runtime", "subscriber-client"] } |
|                                                                             |
| # All other core deps inherited from workspace Cargo.toml:                  |
| # axum, tokio, sqlx, serde, serde_json, uuid, tracing,                      |
| # anyhow, thiserror, tower, tower-http                                      |

## **Summary of Key Design Decisions**

| **Decision**                              | **Rationale**                                                                                               |
| ----------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| **go-live returns token directly**        | Eliminates the startedBroadcast socket round-trip. BE is source of truth for broadcast state.               |
| **join returns token directly**           | Eliminates the joinBroadcast socket round-trip. Cleaner, fewer network hops.                                |
| **end/leave kept as WS events**           | Handles network drops and app kills transparently. Same path for intentional and unintentional disconnects. |
| **60s host grace period**                 | Mobile networks are unreliable. This prevents false 'broadcast ended' events on brief disconnections.       |
| **livekit-api only (not livekit)**        | The backend never participates in media. Only tokens and room management via gRPC. Keeps binary small.      |
| **Arc****<****WsPayload****>**** in hub** | Single allocation per event regardless of recipient count. Critical for broadcasts to 1000+ listeners.      |
| **DashMap for WsHub**                     | Concurrent reads without mutex contention. Multiple Tokio tasks can read the map simultaneously.            |
| **Drafts as inactive broadcasts**         | No separate draft table needed. list?status=inactive&creatorId=me returns drafts naturally.                 |
| **Room name = broadcast UUID**            | Deterministic — no need to store the LiveKit room name. Always derivable from broadcast.id.                 |

Page | Confidential — Meno Engineering
