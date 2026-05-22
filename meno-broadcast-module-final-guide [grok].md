# MENO Broadcast Module — Engineering Reference (Final)

**Version**: Consolidated v2 (May 2026)  
**Stack**: Axum 0.7 + Tokio + sqlx + livekit-api 0.4 + Native WebSocket  
**Clients**: Flutter + Next.js  
**Target**: Nigeria-first (unstable networks, background resilience)

---

## 1. Architecture Overview

### 1.1 Two-Layer Communication Model

| Layer              | Technology                  | Responsibility |
|--------------------|-----------------------------|--------------|
| **Media (Audio)**  | LiveKit (WebRTC)            | Low-latency encrypted audio streaming. Backend **only** mints JWT tokens. |
| **App Logic**      | Axum Native WebSocket + Redis | Chat, lifecycle events (`newBroadcast`, `endedBroadcast`), listener counts, notifications, presence. |

**Key Principle**: Backend is source of truth. LiveKit handles media. Your WS handles orchestration.

### 1.2 LiveKit URL Strategy (ENV-Only)
- Use **one** `LIVEKIT_HOST` (e.g. `wss://your-project.livekit.cloud`) from environment.
- LiveKit Cloud auto-routes users to nearest edge (excellent for Nigeria).
- **Never** return URL in API responses. Clients read from their own config.
- Future self-hosted: additive `livekit_url` field in `BroadcastSessionDto`.

**ENV Vars**:
```env
LIVEKIT_API_KEY=...
LIVEKIT_API_SECRET=...
LIVEKIT_HOST=wss://your-project.livekit.cloud
DAILY_BROADCAST_LIMIT_SECS=1800   # 30 minutes default
```

---

## 2. Streamlined & Resilient Lifecycle

### Go-Live (2 steps — Figma aligned)
1. `POST /broadcasts` → Draft
2. `PUT /broadcasts/:id/go-live` → Atomic:
   - Create LiveKit room (idempotent)
   - Mint **HOST** token
   - Set `status=ACTIVE`
   - Insert host into `broadcast_listeners`
   - Notifications + WS `newBroadcast`
   - Return `BroadcastSessionDto { broadcast, livekit_token }`

### Join (2 steps)
`POST /broadcasts/:id/join` → Returns token + `BroadcastDto` (with `viewer_role`)

### End / Leave
Kept as **WebSocket events** (`endBroadcast`, `leaveBroadcast`) for crash/disconnect resilience.

**Nigeria Resilience Features**:
- **Tiered Host Grace Period**: 120s → 90s → 60s → 30s (based on disconnect count)
- **WS Heartbeats**: 25s ping, adaptive pong timeout (60s for hosts)
- **Message Buffer**: Redis ring buffer replays last 50 events on reconnect
- **Reconnect Rate Limit**: Prevent storms
- **HTTP Keepalive**: `/broadcasts/:id/keepalive` for background service
- **LiveKit Webhooks**: Sync on media-layer drops

---

## 3. Domain Models & DTOs

### 3.1 Core Models (`models.rs`)
```rust
#[derive(sqlx::FromRow, Clone)]
pub struct Broadcast { /* id, title, description, status, creator_id, ... */ }

#[derive(sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "BroadcastRole", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BroadcastRole { Host, Cohost, Listener }

#[derive(sqlx::Type, Serialize, Deserialize)]
pub enum BroadcastStatus { Active, Inactive }
```

### 3.2 Rich `BroadcastDto` v2 (FE State Signals)
```rust
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastDto {
    pub id: Uuid,
    pub title: String,
    // ... core fields

    // State signals
    pub broadcast_state: BroadcastState,      // LIVE | RECONNECTING | ENDED | SCHEDULED | DRAFT
    pub viewer_role: ViewerRole,              // HOST | COHOST | LISTENER | NONE
    pub viewer_is_in_room: bool,
    pub is_subscribed_to_creator: bool,
    pub is_bookmarked: bool,

    // Counts
    pub live_listener_count: i64,
    pub total_listeners: i64,

    // Recording & Continue Listening
    pub recording_url: Option<String>,
    pub time_remaining_seconds: Option<i64>,
    pub creator_quota: Option<QuotaDto>,
    // ...
}
```

**Enums** (SCREAMING_SNAKE_CASE in JSON):
- `BroadcastState`, `ViewerRole`, `EndReason`

---

## 4. Error Handling

**`BroadcastError`** (strongly typed, user-friendly + machine-readable `code`):

- `DAILY_QUOTA_EXCEEDED`, `NOT_CREATOR`, `HOST_DISCONNECTED`, `COHOST_LIMIT_REACHED`, etc.
- Converted to consistent JSON: `{ statusCode, code, message, data? }`

**WS Errors**: `broadcastError` event with `WsErrorCode` (`TOKEN_EXPIRED`, `KICKED`, etc.)

---

## 5. WebSocket Hub (`shared/ws/`)

- `DashMap<Uuid, Vec<ConnectionSender>>`
- `Arc<WsPayload>` for efficient fan-out
- Redis bridge for horizontal scaling
- Message buffer + replay on reconnect
- Heartbeat + tiered grace period logic

**Key Events** (preserved from Socket.IO where possible):
- `newBroadcast`, `endedBroadcast`, `hostDisconnected`, `hostReconnected`
- `newBroadcastListener`, `numberOfLiveListeners`
- `cohostInvitation`, `newCohost`, `broadcastError`

---

## 6. Repository, Service & Handlers

**Repository**: Thin SQL with `QueryBuilder` for dynamic search.

**Service**: Business logic (`go_live`, `join`, `end_broadcast`, quota checks, etc.).

**Handlers**: Thin, auth middleware, validation, error mapping.

**Full Search** (`GET /broadcasts`):
- `status`, `creator_id`, `only_subscriptions`, `keywords`, `recently_ended`, `sort_by`, `order`, `page`, `limit`

**Convenience Endpoints**:
- `/live-for-you`, `/now-live`, `/recently-live`, `/continue-listening`, `/listen-later`, `/active-session`, `/quota`

---

## 7. Figma-Aligned Features

- **Daily Quota**: Redis per-user daily key + mid-broadcast watcher
- **Recording**: LiveKit Egress → S3 → `/publish` → presigned URLs
- **Cohost Flow**: Invitation → Accept/Decline (two-step)
- **Listen Later**: `broadcast_bookmarks` table
- **Continue Listening**: `last_listen_position_seconds`
- **Mini-player**: `GET /broadcasts/active-session`
- **Context Menu**: Fully supported via rich DTO fields

---

## 8. Background & Nigeria Resilience

- Extended host pong timeout
- Grace period + reconnect
- Keepalive endpoint
- Message replay buffer
- Quota enforcement

---

## 9. Complete Endpoint Reference

**HTTP Endpoints** (key ones):

- `GET /broadcasts` — Full search
- `PUT /broadcasts/:id/go-live`
- `POST /broadcasts/:id/join`
- `POST /broadcasts/:id/token` (refresh)
- `POST /broadcasts/:id/keepalive`
- `POST /broadcasts/:id/publish`
- Cohost invitation endpoints, quota, active-session, etc.

**WebSocket Events**:
- Client → Server: `endBroadcast`, `leaveBroadcast`
- Server → Client: `newBroadcast`, `hostDisconnected`, `numberOfLiveListeners`, `broadcastError`, etc.

---

## 10. Recommendations & Next Steps

1. **Implementation Order**: Core Axum → Auth → Broadcasts + WS together.
2. **LiveKit Cloud** for launch.
3. **Testing Focus**: Grace period, background, Nigeria network simulation.
4. **Monitoring**: Prometheus for rooms, WS connections, egress.
5. **Migration**: Parallel run with NestJS.

**This is your single source of truth.**

Download this file for offline use.  
``` 

The file has been written. You can download it using the link below.