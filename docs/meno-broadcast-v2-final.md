**MENO · Broadcast Module v2 **LiveKit · WebSocket · Resilience · Full Feature Reference

**MENO**

**Broadcast Module — Engineering Reference v2**

LiveKit · WebSocket · Nigeria Resilience · Full Figma Coverage

| **Topic**                                      | **Status**                                                       |
| ---------------------------------------------- | ---------------------------------------------------------------- |
| LiveKit multi-region vs ENV-only               | Resolved — detailed decision with free-tier path                 |
| Nigeria network edge cases                     | Fully documented — grace period, heartbeat, reconnect, queuing   |
| BroadcastError taxonomy                        | Complete — HTTP + WS error types, FE-actionable codes            |
| Background app BE support                      | Documented — ping tolerance, participant persistence, keepalive  |
| Full broadcast search API                      | Complete — all filters, sort, pagination, convenience endpoints  |
| FE state signals                               | BroadcastDto v2 — viewer_role, broadcast_state, connection_state |
| Live listener count strategy                   | Hybrid: DB total + Redis live counter + WS delta in room         |
| Figma features: daily quota                    | Full implementation — Redis quota, mid-broadcast watcher         |
| Figma features: recording & publish            | LiveKit Egress + S3 + presigned URLs + webhook                   |
| Figma features: cohost accept flow             | Two-step invite → accept/decline                                 |
| Figma features: Listen Later                   | NEW — save broadcast, queue endpoint                             |
| Figma features: Continue Listening + time left | NEW — time_remaining_seconds field                               |
| Figma features: Now Playing mini-player        | NEW — active_session endpoint                                    |
| Figma features: broadcast context menu         | Share, Copy link, Unsubscribe, Listen Later                      |

# **1. LiveKit URL — Multi-Region vs ENV-Only**

This section resolves the multi-region question definitively so you can make the right call for launch and scale.

## **1.1 How LiveKit Cloud Multi-Region Actually Works**

LiveKit Cloud is not like traditional cloud services where you pick a region and get a URL per region. It works differently:

| **LiveKit Cloud Auto-Routing** LiveKit Cloud uses a global edge network with automatic geo-routing built in. Every participant connecting to your single project URL (e.g. wss://your-project.livekit.cloud) is automatically routed to the nearest edge node. You do NOT configure multiple regions or get multiple URLs. The routing is invisible to you and your clients. This is fundamentally different from self-hosted LiveKit, where you deploy servers per region and manage geo-DNS yourself. |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |

| **Approach**                | **How it works**                                                                                                                   | **Free tier**                     | **Your action**                     |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- | ----------------------------------- |
| LiveKit Cloud (recommended) | Single URL. Cloud auto-routes each participant to nearest edge. Nigeria users get a West Africa or Europe edge node automatically. | 5,000 WebRTC minutes/month + 50GB | Nothing. Just use one URL.          |
| Self-hosted multi-region    | You run LiveKit servers in e.g. Lagos (via AWS AF) + London. Use geo-DNS (Cloudflare) to route. BE returns per-region URL.         | Free (your infra cost only)       | Significant DevOps — not for launch |

| **Decision: Start ENV-Only, Add Region Logic When You Scale** For launch in Nigeria: use one LiveKit Cloud project. Store LIVEKIT_HOST in your server ENV. LiveKit Cloud handles geo-routing automatically — Nigerian users will get routed to their nearest edge. When you eventually self-host for cost reasons (at scale), THEN you add a region field to the BroadcastSessionDto so the BE can return the closest server URL per user. The API contract change is additive (new optional field) so clients don't break. |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |

## **1.2 BroadcastSessionDto — Correct v2 Shape**

| // Launch (ENV-only, LiveKit Cloud handles geo automatically):                 |
| ------------------------------------------------------------------------------ |
| #[derive(Serialize)]                                                           |
| #[serde(rename_all = "camelCase")]                                             |
| pub struct BroadcastSessionDto {                                               |
| pub broadcast: BroadcastDto,                                                   |
| pub livekit_token: String,                                                     |
| // No livekit_url — client reads LIVEKIT_HOST from its own config.             |
| }                                                                              |
|                                                                                |
| // Future self-hosted scale path (additive — old clients ignore new field):    |
| pub struct BroadcastSessionDto {                                               |
| pub broadcast: BroadcastDto,                                                   |
| pub livekit_token: String,                                                     |
| pub livekit_url: Option<String>, // Some(url) when self-hosted, None for Cloud |
| }                                                                              |
|                                                                                |
| // Flutter: reads from config when livekit_url is None                         |
| // final url = session.livekitUrl ?? AppConfig.livekitHost;                    |
|                                                                                |
| // ENV vars needed:                                                            |
| // LIVEKIT_API_KEY = <from LiveKit Cloud dashboard>                            |
| // LIVEKIT_API_SECRET = <from LiveKit Cloud dashboard>                         |
| // LIVEKIT_HOST = wss://your-project.livekit.cloud                             |

# **2. Network Resilience — Nigeria-Grade Robustness**

Nigeria's mobile network reality: frequent 2G/3G fallbacks in dense urban areas, packet loss during handoffs between towers, carrier NAT timeouts as short as 20 seconds, and radio scheduling delays when apps are backgrounded on iOS. Every failure mode must be handled gracefully.

## **2.1 Failure Mode Taxonomy**

| **Failure**                             | **Cause**                                      | **Detection**                    | **Recovery**                                         |
| --------------------------------------- | ---------------------------------------------- | -------------------------------- | ---------------------------------------------------- |
| Host WS drops mid-broadcast             | Network switch, phone screen off, backgrounded | Missed heartbeat pings           | 90s grace period → reconnect → hostReconnected event |
| Listener WS drops                       | Network loss, app close                        | Connection close / missed pings  | Auto-remove from room, update count                  |
| Flutter app crashes (force kill)        | OOM, OS kill, crash                            | WS TCP close without Close frame | Same as WS drop — cleanup on disconnect              |
| Rapid reconnect loops                   | Unstable network                               | Redis reconnect counter          | Exponential backoff enforced server-side             |
| LiveKit token expires in-session        | Broadcast > 6 hours                            | Token TTL check on join          | FE polls expiry, calls refresh endpoint              |
| LiveKit media drops (WS stays alive)    | Different network path for WebRTC              | LiveKit participant_left webhook | Log only — LiveKit SDK handles media reconnect       |
| Double join race                        | User taps Join twice                           | Redis per-user lock              | Second call gets 409 Conflict                        |
| Cohost added while offline              | Creator acts on absent user                    | is_online() check on hub         | WS event queued in Redis buffer                      |
| Simultaneous end_broadcast calls        | Host + grace-period task both fire             | DB transaction idempotency       | Second call is a no-op (status already inactive)     |
| Broadcast deleted between list and join | Race condition                                 | DB find_by_id returns None       | 404 BroadcastError::NotFound                         |
| Quota exhausted mid-broadcast           | 30-min limit reached                           | Quota watcher task               | WS error event → auto-end broadcast                  |

## **2.2 WebSocket Heartbeat — Tuned for Nigerian Networks**

| // Timing rationale:                                                                           |
| ---------------------------------------------------------------------------------------------- |
| // - Nigerian mobile NATs drop idle TCP after ~30s → ping every 25s                            |
| // - iOS background budget: up to 30s before processing is frozen → 60s pong timeout for hosts |
| // - Listeners get 20s timeout (more aggressive, less critical)                                |
| // - 2 missed pongs before declaring dead (handles one brief packet loss event)                |
|                                                                                                |
| pub struct HeartbeatConfig {                                                                   |
| pub ping_interval_secs: u64, // 25                                                             |
| pub host_pong_timeout: u64, // 60 — hosts get extended window (backgrounded app)               |
| pub listener_pong_timeout: u64, // 20                                                          |
| pub max_missed_pings: u32, // 2                                                                |
| }                                                                                              |
|                                                                                                |
| ASYNC FUNCTION handle_socket(socket, user, state)                                              |
|                                                                                                |
| is_active_host = broadcast_repo.find_active_hosted_by(user.id).await?.is_some()                |
| pong_timeout = if is_active_host { 60 } else { 20 }                                            |
|                                                                                                |
| missed = Arc::new(AtomicU32::new(0))                                                           |
| missed_clone = Arc::clone(&missed)                                                             |
|                                                                                                |
| heartbeat_task = tokio::spawn(async move {                                                     |
| let mut interval = tokio::time::interval(Duration::from_secs(25))                              |
| loop {                                                                                         |
| interval.tick().await                                                                          |
| let m = missed_clone.fetch_add(1, Ordering::Relaxed)                                           |
| if m >= 2 {                                                                                    |
| tracing::warn!('User {} missed {} pings — forcibly disconnecting', user.id, m)                 |
| // The write task will close naturally once hub drops the sender                               |
| break                                                                                          |
| }                                                                                              |
| ws_tx.send(Message::Ping(vec![])).await.ok()                                                   |
| }                                                                                              |
| })                                                                                             |
|                                                                                                |
| loop {                                                                                         |
| let timeout = tokio::time::timeout(                                                            |
| Duration::from_secs(pong_timeout),                                                             |
| ws_rx.next()                                                                                   |
| ).await                                                                                        |
|                                                                                                |
| match timeout {                                                                                |
| Ok(Some(Ok(Message::Pong(_)))) => {                                                            |
| missed.store(0, Ordering::Relaxed) // alive                                                    |
| }                                                                                              |
| Ok(Some(Ok(Message::Text(text)))) => {                                                         |
| // Also resets missed counter — any inbound message = alive                                    |
| missed.store(0, Ordering::Relaxed)                                                             |
| handle_client_message(text, user.id, state).await                                              |
| }                                                                                              |
| Ok(Some(Ok(Message::Close(_)))) │ Ok(None) => break, // intentional close                      |
| Err(_timeout_elapsed) => {                                                                     |
| tracing::warn!('User {} pong timeout after {}s', user.id, pong_timeout)                        |
| break // treat timeout as disconnect                                                           |
| }                                                                                              |
| _ => {}                                                                                        |
| }                                                                                              |
| }                                                                                              |
|                                                                                                |
| heartbeat_task.abort()                                                                         |
| on_disconnect(user.id, state).await                                                            |

## **2.3 Host Grace Period — The Full State Machine**

| **The Outside-The-Box Approach: Tiered Grace Periods** Rather than a single fixed grace period, use a tiered system that adapts to the host's behaviour pattern during the session. First disconnect: 120s (generous — probably a network blip). Subsequent disconnects: 90s, 60s, 30s. This rewards reliable hosts and gracefully terminates sessions where the host is clearly on a broken connection. It is still generous enough for Nigerian networks while preventing zombie broadcasts from running indefinitely. |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |

| #[derive(Debug, Clone, PartialEq, Serialize)]                                             |
| ----------------------------------------------------------------------------------------- |
| #[serde(rename_all = "SCREAMING_SNAKE_CASE")]                                             |
| pub enum BroadcastConnectionState {                                                       |
| Live, // Host connected and active                                                        |
| Reconnecting, // Host WS dropped, within grace period                                     |
| Ended,                                                                                    |
| }                                                                                         |
|                                                                                           |
| // Redis keys:                                                                            |
| // 'host_grace:{bid}' → grace_secs (u64 as string), TTL = grace_secs                      |
| // 'host_disconnect_count:{bid}' → number of disconnects this session                     |
| // 'host_reconnect_at:{bid}' → timestamp of last reconnect (for analytics)                |
|                                                                                           |
| ASYNC FUNCTION on_host_disconnected(broadcast_id, host_id, state)                         |
|                                                                                           |
| // 1. How many times has this host disconnected in this session?                          |
| count_key = format!('host_disconnect_count:{}', broadcast_id)                             |
| disconnect_count: u64 = state.redis.incr(&count_key).await.unwrap_or(1)                   |
| state.redis.expire(&count_key, 3600).await.ok() // reset after 1h                         |
|                                                                                           |
| // 2. Tiered grace period                                                                 |
| grace_secs = match disconnect_count {                                                     |
| 1 => 120, // first time: very generous                                                    |
| 2 => 90, // second time: still generous                                                   |
| 3 => 60, // third time: firm                                                              |
| _ => 30, // fourth+ time: minimal tolerance                                               |
| }                                                                                         |
|                                                                                           |
| // 3. Set grace flag with TTL                                                             |
| state.redis.set_ex(                                                                       |
| &format!('host_grace:{}', broadcast_id),                                                  |
| grace_secs.to_string(),                                                                   |
| grace_secs,                                                                               |
| ).await?                                                                                  |
|                                                                                           |
| // 4. Store start time for reconnect window calculation (for FE countdown)                |
| state.redis.set_ex(                                                                       |
| &format!('host_grace_started:{}', broadcast_id),                                          |
| Utc::now().timestamp().to_string(),                                                       |
| grace_secs + 10,                                                                          |
| ).await?                                                                                  |
|                                                                                           |
| // 5. Notify all participants — include grace_secs so FE can show countdown               |
| all_ids = broadcast_repo.get_all_participant_ids(broadcast_id).await?                     |
| state.ws_hub.send_to_users(&all_ids, WsPayload {                                          |
| event: 'hostDisconnected'.into(),                                                         |
| data: json!({                                                                             |
| 'broadcastId': broadcast_id,                                                              |
| 'gracePeriodSecs': grace_secs,                                                            |
| 'disconnectCount': disconnect_count,                                                      |
| }),                                                                                       |
| }).await                                                                                  |
|                                                                                           |
| // 6. Spawn grace-period watcher                                                          |
| let state_c = Arc::clone(&state)                                                          |
| tokio::spawn(async move {                                                                 |
| tokio::time::sleep(Duration::from_secs(grace_secs)).await                                 |
|                                                                                           |
| // Is the grace key still there? (If host reconnected, it was deleted.)                   |
| if state_c.redis.exists(&format!('host_grace:{}', broadcast_id)).await.unwrap_or(false) { |
| state_c.redis.del(&format!('host_grace:{}', broadcast_id)).await.ok()                     |
| tracing::info!('Host grace expired for broadcast {} — ending', broadcast_id)              |
| broadcast_service.end_broadcast(                                                          |
| broadcast_id,                                                                             |
| host_id,                                                                                  |
| EndReason::HostDisconnected,                                                              |
| &state_c,                                                                                 |
| ).await.ok()                                                                              |
| }                                                                                         |
| })                                                                                        |
|                                                                                           |
| ────────────────────────────────────────────────────────                                  |
|                                                                                           |
| ASYNC FUNCTION on_host_reconnected(user_id, state)                                        |
|                                                                                           |
| broadcast = broadcast_repo.find_active_hosted_by(user_id).await?                          |
| if let Some(b) = broadcast {                                                              |
| grace_key = format!('host_grace:{}', b.id)                                                |
| was_in_grace = state.redis.del(&grace_key).await.unwrap_or(0) > 0                         |
|                                                                                           |
| // Drain any queued WS messages for this user                                             |
| drain_message_buffer(user_id, state).await                                                |
|                                                                                           |
| if was_in_grace {                                                                         |
| all_ids = broadcast_repo.get_all_participant_ids(b.id).await?                             |
| state.ws_hub.send_to_users(&all_ids, WsPayload {                                          |
| event: 'hostReconnected'.into(),                                                          |
| data: json!({ 'broadcastId': b.id }),                                                     |
| }).await                                                                                  |
| }                                                                                         |
| }                                                                                         |

## **2.4 Message Buffer — Missed Events on Reconnect**

| // Problem: client drops WS for 30s then reconnects. They missed 'newMessage' events. |
| ------------------------------------------------------------------------------------- |
| // Solution: buffer last 50 events per user in Redis. Replay on reconnect.            |
| // This solves the 'chat is empty when I rejoin' problem.                             |
|                                                                                       |
| const BUFFER_KEY: fn(Uuid) -> String = │id│ format!('ws_buf:{}', id);                 |
| const BUFFER_SIZE: isize = 50; // ring buffer of last 50 events                       |
| const BUFFER_TTL: u64 = 300; // 5 minutes — no point replaying older events           |
|                                                                                       |
| // WRITING: called in send_to_user() when user is offline                             |
| ASYNC FUNCTION buffer_message(user_id, payload, state)                                |
| if state.ws_hub.is_online(user_id) {                                                  |
| // Online: deliver directly                                                           |
| state.ws_hub.deliver(user_id, Arc::new(payload)).await                                |
| return                                                                                |
| }                                                                                     |
| // Offline: push to Redis ring buffer                                                 |
| let key = BUFFER_KEY(user_id)                                                         |
| let json = serde_json::to_string(&payload)?                                           |
| // LPUSH + LTRIM = ring buffer (newest at index 0)                                    |
| state.redis.lpush(&key, &json).await?                                                 |
| state.redis.ltrim(&key, 0, BUFFER_SIZE - 1).await?                                    |
| state.redis.expire(&key, BUFFER_TTL).await?                                           |
|                                                                                       |
| // READING: called after register in handle_socket                                    |
| ASYNC FUNCTION drain_message_buffer(user_id, state)                                   |
| let key = BUFFER_KEY(user_id)                                                         |
|                                                                                       |
| // Atomic GETDEL equivalent: LRANGE then DEL                                          |
| // Use a Lua script for atomicity:                                                    |
| let script = r#'                                                                      |
| local items = redis.call('LRANGE', KEYS[1], 0, -1)                                    |
| redis.call('DEL', KEYS[1])                                                            |
| return items                                                                          |
| '#                                                                                    |
| let buffered: Vec<String> = state.redis.eval(script, &[key], &[]).await?              |
|                                                                                       |
| // Replay in chronological order (LPUSH = LIFO, so reverse)                           |
| for json in buffered.into_iter().rev() {                                              |
| if let Ok(payload) = serde_json::from_str::<WsPayload>(&json) {                       |
| // Deliver directly now that user is registered in hub                                |
| state.ws_hub.deliver(user_id, Arc::new(payload)).await                                |
| }                                                                                     |
| }                                                                                     |

## **2.5 Reconnect Rate Limiting**

| // Prevent reconnect storms: if a client disconnects and reconnects 10+ times  |
| ------------------------------------------------------------------------------ |
| // in 60 seconds, impose a 30-second backoff before allowing hub registration. |
| // This protects the server from clients in a crash loop.                      |
|                                                                                |
| ASYNC FUNCTION check_reconnect_rate(user_id, state) -> Result<()>              |
| key = format!('reconnect_rate:{}', user_id)                                    |
| count: i64 = state.redis.incr(&key).await?                                     |
| if count == 1 {                                                                |
| state.redis.expire(&key, 60).await? // start 60s window on first connect       |
| }                                                                              |
| if count > 10 {                                                                |
| tracing::warn!('User {} is reconnecting too fast ({}/60s)', user_id, count)    |
| return Err(AppError::TooManyRequests(                                          |
| 'Too many reconnections. Wait 30 seconds before retrying.'                     |
| ))                                                                             |
| }                                                                              |
| Ok(())                                                                         |
|                                                                                |
| // Called in ws_upgrade before socket.on_upgrade()                             |
| // If err, return 429 status before upgrading to WS                            |

# **3. BroadcastError — Complete Error System**

All broadcast errors are strongly typed in Rust and serialised to a consistent JSON shape. The machine-readable 'code' field is what Flutter/Next.js switches on — never parse the 'message' string.

## **3.1 HTTP Error Type**

| // modules/broadcast/errors.rs                                             |
| -------------------------------------------------------------------------- |
|                                                                            |
| #[derive(Debug, thiserror::Error)]                                         |
| pub enum BroadcastError {                                                  |
| // ── 400 Bad Request ─────────────────────────────────────────────        |
| #[error('Broadcast title is required')]                                    |
| MissingTitle,                                                              |
|                                                                            |
| #[error('Description exceeds 244 characters (got {0} chars)')]             |
| DescriptionTooLong(usize),                                                 |
|                                                                            |
| #[error('Broadcast is not currently live')]                                |
| NotLive,                                                                   |
|                                                                            |
| #[error('Broadcast is already live')]                                      |
| AlreadyLive,                                                               |
|                                                                            |
| #[error('You are not a participant in this broadcast')]                    |
| NotParticipant,                                                            |
|                                                                            |
| #[error('A join request is already in progress')]                          |
| JoinInProgress,                                                            |
|                                                                            |
| #[error('Invalid time zone: {0}')]                                         |
| InvalidTimeZone(String),                                                   |
|                                                                            |
| #[error('start_time must be in the future')]                               |
| StartTimeInPast,                                                           |
|                                                                            |
| #[error('Recording not available for this broadcast')]                     |
| RecordingNotAvailable,                                                     |
|                                                                            |
| #[error('Cannot publish a broadcast that is still live')]                  |
| BroadcastStillLive,                                                        |
|                                                                            |
| #[error('Daily broadcast time exhausted. Resets in {resets_in_mins} min')] |
| DailyQuotaExceeded { resets_in_mins: u32 },                                |
|                                                                            |
| // ── 403 Forbidden ───────────────────────────────────────────────        |
| #[error('Only the broadcast creator can perform this action')]             |
| NotCreator,                                                                |
|                                                                            |
| #[error('Only the creator or admin can end this broadcast')]               |
| CannotEnd,                                                                 |
|                                                                            |
| #[error('Cohost limit reached. A broadcast supports 1 co-host')]           |
| CohostLimitReached,                                                        |
|                                                                            |
| #[error('This invitation is not addressed to you')]                        |
| InvitationNotYours,                                                        |
|                                                                            |
| // ── 404 Not Found ───────────────────────────────────────────────        |
| #[error('Broadcast not found')]                                            |
| NotFound,                                                                  |
|                                                                            |
| #[error('Cohost invitation not found')]                                    |
| InvitationNotFound,                                                        |
|                                                                            |
| // ── 409 Conflict ────────────────────────────────────────────────        |
| #[error('User is already a co-host of this broadcast')]                    |
| AlreadyCohost,                                                             |
|                                                                            |
| #[error('This user already has a pending co-host invitation')]             |
| DuplicateInvitation,                                                       |
|                                                                            |
| // ── 429 Too Many Requests ────────────────────────────────────────       |
| #[error('Too many reconnection attempts. Wait 30 seconds')]                |
| TooManyReconnects,                                                         |
|                                                                            |
| // ── 503 Service Unavailable ─────────────────────────────────────        |
| #[error('Could not connect to media server. Please try again')]            |
| LiveKitUnavailable,                                                        |
|                                                                            |
| // ── 500 Internal ────────────────────────────────────────────────        |
| #[error('Database error')]                                                 |
| Database(#[from] sqlx::Error),                                             |
|                                                                            |
| #[error('Internal error: {0}')]                                            |
| Internal(#[from] anyhow::Error),                                           |
| }                                                                          |
|                                                                            |
| // IntoResponse implementation — consistent JSON for all variants:         |
| // {                                                                       |
| // 'statusCode': 403,                                                      |
| // 'code': 'NOT_CREATOR', ← Flutter/JS switches on this                    |
| // 'message': 'Only the broadcast...', ← shown to user if needed           |
| // 'data': { 'resetsInMins': 47 } ← variant-specific extra data (optional) |
| // }                                                                       |
|                                                                            |
| // The 'code' field is the SCREAMING_SNAKE_CASE variant name.              |
| // Flutter pattern:                                                        |
| // switch (error.code) {                                                   |
| // case 'DAILY_QUOTA_EXCEEDED':                                            |
| // showQuotaDialog(resetsIn: error.data['resetsInMins'])                   |
| // case 'COHOST_LIMIT_REACHED':                                            |
| // showSnackbar('Only 1 co-host allowed per broadcast')                    |
| // case 'ALREADY_LIVE':                                                    |
| // navigateToBroadcast() // it's already running, just join it             |
| // }                                                                       |

## **3.2 WebSocket Error Events (Runtime Errors)**

Some errors can only be communicated after the HTTP response has been sent — e.g. token expiry mid-session, admin force-ending the broadcast, or quota exhaustion. These arrive as typed WS events.

| #[derive(Serialize, Clone)]                                                   |
| ----------------------------------------------------------------------------- |
| #[serde(rename_all = "camelCase")]                                            |
| pub struct BroadcastWsError {                                                 |
| pub broadcast_id: Uuid,                                                       |
| pub code: WsErrorCode,                                                        |
| pub message: String,                                                          |
| pub recoverable: bool, // true = FE should retry, false = navigate away       |
| pub data: Option<serde_json::Value>, // extra context per code                |
| }                                                                             |
|                                                                               |
| #[derive(Serialize, Clone)]                                                   |
| #[serde(rename_all = "SCREAMING_SNAKE_CASE")]                                 |
| pub enum WsErrorCode {                                                        |
| TokenExpired, // recoverable: true — call /broadcasts/:id/token               |
| BroadcastForciblyEnded, // recoverable: false — admin ended it, navigate away |
| KickedFromRoom, // recoverable: false — host removed you                      |
| DailyQuotaExceeded, // recoverable: false — host's time is up                 |
| RoomNotFound, // recoverable: false — LiveKit room gone                       |
| MediaServerError, // recoverable: true — retry connection                     |
| }                                                                             |
|                                                                               |
| // Wire format emitted as:                                                    |
| // { 'event': 'broadcastError', 'data': BroadcastWsError }                    |
|                                                                               |
| // Flutter handler:                                                           |
| // ws.on('broadcastError').listen((data) {                                    |
| // final err = BroadcastWsError.fromJson(data)                                |
| // switch (err.code) {                                                        |
| // case WsErrorCode.tokenExpired:                                             |
| // final session = await api.refreshToken(err.broadcastId)                    |
| // await room.refreshToken(session.livekitToken) // LiveKit SDK method        |
| // case WsErrorCode.broadcastForciblyEnded:                                   |
| // case WsErrorCode.kickedFromRoom:                                           |
| // navigateHome(reason: err.message)                                          |
| // case WsErrorCode.dailyQuotaExceeded:                                       |
| // showQuotaExhaustedScreen()                                                 |
| // }                                                                          |
| // })                                                                         |

# **4. Background App Support — BE Contributions**

The FE foreground service keeps audio alive. The BE's job is to not kill the session while the app is in the background.

| **FE Responsibility**                           | **BE Responsibility**                                             |
| ----------------------------------------------- | ----------------------------------------------------------------- |
| Flutter foreground service (audio continues)    | Extended pong timeout for active hosts (60s vs 20s for listeners) |
| WS reconnection with exponential backoff        | Message buffer replays missed events on reconnect (5-min window)  |
| LiveKit SDK media reconnection                  | Tiered grace period: 120s first disconnect → 30s fourth+          |
| Show persistent notification while broadcasting | Participant state in DB — re-join after crash returns same role   |
| Re-acquire microphone on foreground             | Keepalive heartbeat endpoint for non-WS health check              |
| Re-authenticate WS after cold app restart       | Token refresh endpoint with role inference from DB                |

## **4.1 Non-WS Keepalive Endpoint**

Some network environments kill WebSockets but allow HTTP. Adding a simple HTTP keepalive lets the FE background service verify the session is alive without needing the WS connection.

| // POST /broadcasts/:id/keepalive                                                      |
| -------------------------------------------------------------------------------------- |
| // Called by the FE background service every 30 seconds when WS is not reachable.      |
| // This tells the server 'I am still in this broadcast'.                               |
|                                                                                        |
| HANDLER broadcast_keepalive(Path(id), AuthUser(user)) -> Json<KeepaliveResponse>       |
|                                                                                        |
| broadcast = broadcast_repo.find_by_id(id).await?.ok_or(NotFound)?                      |
| if broadcast.status != Active { return Err(NotLive) }                                  |
|                                                                                        |
| listener = broadcast_repo.find_listener(id, user.id).await?                            |
| if listener.is_none() { return Err(NotParticipant) }                                   |
|                                                                                        |
| // Update Redis last-seen timestamp                                                    |
| state.redis.set_ex(                                                                    |
| &format!('participant_last_seen:{}:{}', id, user.id),                                  |
| Utc::now().timestamp().to_string(),                                                    |
| 300, // 5-min TTL                                                                      |
| ).await?                                                                               |
|                                                                                        |
| // If host: also refresh the grace period if they're in one                            |
| if broadcast.creator_id == user.id {                                                   |
| // Calling keepalive during grace period = host is alive, extend grace                 |
| grace_key = format!('host_grace:{}', id)                                               |
| if state.redis.exists(&grace_key).await? {                                             |
| on_host_reconnected(user.id, state).await?                                             |
| }                                                                                      |
| }                                                                                      |
|                                                                                        |
| Ok(Json(KeepaliveResponse {                                                            |
| broadcast_id: id,                                                                      |
| connection_state: BroadcastConnectionState::Live,                                      |
| live_listener_count: state.redis.get(format!('live_count:{}', id)).await.unwrap_or(0), |
| token_expires_at: /* decoded from current token */,                                    |
| }))                                                                                    |
|                                                                                        |
| #[derive(Serialize)]                                                                   |
| pub struct KeepaliveResponse {                                                         |
| pub broadcast_id: Uuid,                                                                |
| pub connection_state: BroadcastConnectionState,                                        |
| pub live_listener_count: i64,                                                          |
| pub token_expires_at: DateTime<Utc>, // FE checks if token refresh needed              |
| }                                                                                      |

# **5. Broadcast Search ****&**** Query API**

All the query parameters from the NestJS implementation, plus additions from the Figma designs.

## **5.1 Full Request DTO**

| #[derive(Debug, Deserialize, Default)]                                            |
| --------------------------------------------------------------------------------- |
| #[serde(rename_all = "camelCase")]                                                |
| pub struct BroadcastQuery {                                                       |
| // ── Filtering ────────────────────────────────────────────────                  |
| pub id: Option<Uuid>, // fetch single by ID in list context                       |
| pub status: Option<BroadcastStatus>, // active │ inactive                         |
| pub creator_id: Option<Uuid>,                                                     |
| pub only_subscriptions: Option<bool>, // 'Live for you'                           |
| pub keywords: Option<String>, // full-text on title + description                 |
| pub recently_ended: Option<bool>, // ended < 24h ago ('Recently live')            |
| pub has_recording: Option<bool>, // has published recording                       |
| pub exclude_creator_id: Option<Uuid>, // exclude a specific creator               |
|                                                                                   |
| // ── Sorting ─────────────────────────────────────────────────                   |
| pub sort_by: Option<BroadcastSortField>, // default: created_at                   |
| pub order: Option<SortOrder>, // default: desc                                    |
|                                                                                   |
| // ── Pagination ──────────────────────────────────────────────                   |
| pub page: Option<i64>, // 1-indexed, default 1                                    |
| pub limit: Option<i64>, // default 20, max 100                                    |
| }                                                                                 |
|                                                                                   |
| #[derive(Debug, Deserialize, Default, Clone, Copy, PartialEq)]                    |
| #[serde(rename_all = "snake_case")]                                               |
| pub enum BroadcastSortField {                                                     |
| #[default] CreatedAt,                                                             |
| Title, // alphabetical sort                                                       |
| StartTime, // when scheduled                                                      |
| EndTime, // recently-ended sort                                                   |
| TotalListeners, // most popular (uses Redis counter for live, DB count for ended) |
| }                                                                                 |
|                                                                                   |
| #[derive(Debug, Deserialize, Default, Clone, Copy)]                               |
| #[serde(rename_all = "snake_case")]                                               |
| pub enum SortOrder { #[default] Desc, Asc }                                       |

## **5.2 Home Section Convenience Endpoints**

These map directly to the Figma sections. Each is a thin wrapper around the same list() function with pre-configured query presets.

| **Endpoint**                       | **Auth** | **Figma Section**        | **Query Preset**                                             |
| ---------------------------------- | -------- | ------------------------ | ------------------------------------------------------------ |
| GET /broadcasts                    | —        | Search / Discover        | All params available                                         |
| GET /broadcasts/live-for-you       | ✓        | Live for you ✨          | status=active, only_subscriptions=true, sort=created_at desc |
| GET /broadcasts/now-live           | —        | Now live 🔥              | status=active, sort=total_listeners desc                     |
| GET /broadcasts/recently-live      | —        | Recently live ⚡         | recently_ended=true, sort=end_time desc                      |
| GET /broadcasts/continue-listening | ✓        | Continue listening       | Viewer's past joins, has_recording=true or recently ended    |
| GET /broadcasts/listen-later       | ✓        | Listen Later (bookmarks) | User's saved broadcasts                                      |

## **5.3 Query Builder (Repository)**

| ASYNC FUNCTION list(query, viewer_id, db) -> Result<(Vec<BroadcastRow>, i64)> |
| ----------------------------------------------------------------------------- |
|                                                                               |
| // Base query: always filter deleted_at IS NULL                               |
| // COUNT(*) total_listeners via subquery to avoid JOIN inflation              |
| let mut qb = QueryBuilder::new(                                               |
| 'SELECT b.*,                                                                  |
| (SELECT COUNT(*) FROM broadcast_listeners bl WHERE bl.broadcast_id = b.id)    |
| AS total_listeners                                                            |
| FROM broadcasts b                                                             |
| WHERE b.deleted_at IS NULL'                                                   |
| )                                                                             |
|                                                                               |
| // id filter                                                                  |
| if let Some(id) = query.id {                                                  |
| qb.push(' AND b.id = ').push_bind(id)                                         |
| }                                                                             |
|                                                                               |
| // status filter                                                              |
| if let Some(s) = query.status {                                               |
| qb.push(' AND b.status = ').push_bind(s)                                      |
| }                                                                             |
|                                                                               |
| // creator filter                                                             |
| if let Some(cid) = query.creator_id {                                         |
| qb.push(' AND b.creator_id = ').push_bind(cid)                                |
| }                                                                             |
|                                                                               |
| // subscriptions filter (Live for you)                                        |
| if query.only_subscriptions.unwrap_or(false) {                                |
| if let Some(vid) = viewer_id {                                                |
| qb.push(' AND b.creator_id IN (                                               |
| SELECT subscription_id FROM user_subscribers WHERE subscriber_id = ')         |
| .push_bind(vid).push(')')                                                     |
| }                                                                             |
| }                                                                             |
|                                                                               |
| // full-text search                                                           |
| if let Some(ref kw) = query.keywords {                                        |
| qb.push(                                                                      |
| ' AND to_tsvector(''english'', b.title ││ '' '' ││ b.description)             |
| @@ plainto_tsquery(''english'', '                                             |
| ).push_bind(kw).push(')')                                                     |
| }                                                                             |
|                                                                               |
| // recently ended (Recently live section)                                     |
| if query.recently_ended.unwrap_or(false) {                                    |
| qb.push(                                                                      |
| ' AND b.status = ''inactive''                                                 |
| AND b.end_time > now() - interval ''24 hours'''                               |
| )                                                                             |
| }                                                                             |
|                                                                               |
| // has recording (Continue listening / published)                             |
| if let Some(true) = query.has_recording {                                     |
| qb.push(' AND b.recording_url IS NOT NULL AND b.published_at IS NOT NULL')    |
| }                                                                             |
|                                                                               |
| // ORDER BY                                                                   |
| let sort_col = match query.sort_by.unwrap_or_default() {                      |
| BroadcastSortField::Title => 'b.title',                                       |
| BroadcastSortField::StartTime => 'b.start_time',                              |
| BroadcastSortField::EndTime => 'COALESCE(b.end_time, b.created_at)',          |
| BroadcastSortField::TotalListeners => 'total_listeners',                      |
| BroadcastSortField::CreatedAt => 'b.created_at',                              |
| }                                                                             |
| let order = match query.order.unwrap_or_default() {                           |
| SortOrder::Asc => 'ASC NULLS LAST',                                           |
| SortOrder::Desc => 'DESC NULLS LAST',                                         |
| }                                                                             |
| qb.push(format!(' ORDER BY {} {}', sort_col, order))                          |
|                                                                               |
| // LIMIT / OFFSET                                                             |
| let limit = query.limit.unwrap_or(20).clamp(1, 100)                           |
| let offset = (query.page.unwrap_or(1).max(1) - 1) * limit                     |
| qb.push(' LIMIT ').push_bind(limit).push(' OFFSET ').push_bind(offset)        |
|                                                                               |
| let rows = qb.build_query_as::<BroadcastRow>().fetch_all(db).await?           |
| let total = count_with_same_filters(query, viewer_id, db).await?              |
| Ok((rows, total))                                                             |

# **6. FE State Signals — The Complete BroadcastDto v2**

The root cause of the 'am I the host?' confusion in NestJS was missing explicit state fields. Every response now answers these questions directly, without the FE having to compute them.

## **6.1 Questions the DTO Must Answer**

| **FE Question**                                 | **Field in BroadcastDto v2**                    |
| ----------------------------------------------- | ----------------------------------------------- |
| Is this broadcast currently live?               | broadcast_state == 'LIVE'                       |
| Is it live but host is reconnecting?            | connection_state == 'RECONNECTING'              |
| Am I the host of this broadcast?                | viewer_role == 'HOST'                           |
| Am I a cohost?                                  | viewer_role == 'COHOST'                         |
| Am I currently in the room?                     | viewer_is_in_room == true                       |
| Have I subscribed to this creator?              | is_subscribed_to_creator == true                |
| How long was this broadcast?                    | duration_seconds                                |
| How many people are live right now?             | live_listener_count (from Redis)                |
| How many total have ever listened?              | total_listeners (from DB)                       |
| Was this recorded and published?                | published_at != null && recording_url != null   |
| How much time is left if I joined halfway?      | time_remaining_seconds (continue listening)     |
| How much broadcast quota does the creator have? | creator_quota (in host-specific responses only) |

## **6.2 Full BroadcastDto v2**

| #[derive(Debug, Serialize, Clone)]                                          |
| --------------------------------------------------------------------------- |
| #[serde(rename_all = "camelCase")]                                          |
| pub struct BroadcastDto {                                                   |
| // ── Identity ────────────────────────────────────────────────             |
| pub id: Uuid,                                                               |
| pub title: String,                                                          |
| pub description: String,                                                    |
| pub time_zone: String,                                                      |
| pub image_url: Option<String>,                                              |
| pub image_id: Option<String>,                                               |
| pub creator_id: Uuid,                                                       |
|                                                                             |
| // ── Timestamps ──────────────────────────────────────────────             |
| pub created_at: Option<DateTime<Utc>>,                                      |
| pub start_time: Option<DateTime<Utc>>, // nil for instant broadcasts        |
| pub end_time: Option<DateTime<Utc>>,                                        |
| pub published_at: Option<DateTime<Utc>>,                                    |
| pub duration_seconds: Option<i64>, // computed: end - start when ended      |
|                                                                             |
| // ── State signals (FE switches on these) ────────────────────             |
| pub broadcast_state: BroadcastState,                                        |
| // LIVE — status=active, no grace period                                    |
| // RECONNECTING — status=active, host_grace Redis key exists                |
| // ENDED — status=inactive, end_time is set                                 |
| // SCHEDULED — status=inactive, start_time is in the future                 |
| // DRAFT — status=inactive, no start_time, no end_time                      |
|                                                                             |
| // ── Viewer-specific signals ──────────────────────────────────            |
| pub viewer_role: ViewerRole,                                                |
| // HOST, COHOST, LISTENER, NONE                                             |
| // Always present (NONE for unauthenticated viewers)                        |
|                                                                             |
| pub viewer_is_in_room: bool, // currently joined as participant             |
| pub is_subscribed_to_creator: bool, // viewer follows creator               |
|                                                                             |
| // ── Counts ──────────────────────────────────────────────────             |
| pub live_listener_count: i64, // from Redis (0 when not live)               |
| pub total_listeners: i64, // from DB (all-time joins)                       |
|                                                                             |
| // ── Recording ───────────────────────────────────────────────             |
| pub recording_enabled: bool,                                                |
| pub recording_url: Option<String>, // presigned S3 URL if published         |
| pub end_reason: Option<EndReason>, // why it ended                          |
|                                                                             |
| // ── Continue listening (context-specific) ───────────────────             |
| pub time_remaining_seconds: Option<i64>,                                    |
| // Only populated in /continue-listening endpoint.                          |
| // = duration_seconds - viewer's last_listen_position_seconds               |
| // FE shows '45 mins left'                                                  |
|                                                                             |
| pub last_listened_at: Option<DateTime<Utc>>,                                |
| // When viewer last joined this broadcast.                                  |
|                                                                             |
| // ── Quota (host-only, only in go-live response) ──────────────            |
| pub creator_quota: Option<QuotaDto>,                                        |
|                                                                             |
| // ── Relations (conditionally populated) ─────────────────────             |
| pub creator: Option<UserSummaryDto>,                                        |
| pub cohosts: Option<Vec<CohostDto>>, // includes cohost role badge          |
| }                                                                           |
|                                                                             |
| // All enum variants in SCREAMING_SNAKE_CASE in the JSON                    |
| #[derive(Serialize, Clone, PartialEq)]                                      |
| #[serde(rename_all = "SCREAMING_SNAKE_CASE")]                               |
| pub enum BroadcastState { Live, Reconnecting, Ended, Scheduled, Draft }     |
|                                                                             |
| #[derive(Serialize, Clone, PartialEq)]                                      |
| #[serde(rename_all = "SCREAMING_SNAKE_CASE")]                               |
| pub enum ViewerRole { Host, Cohost, Listener, None }                        |
|                                                                             |
| #[derive(Serialize, Clone)]                                                 |
| #[serde(rename_all = "SCREAMING_SNAKE_CASE")]                               |
| pub enum EndReason { Normal, HostDisconnected, AdminForced, QuotaExceeded } |
|                                                                             |
| #[derive(Serialize, Clone)]                                                 |
| #[serde(rename_all = "camelCase")]                                          |
| pub struct CohostDto {                                                      |
| pub id: Uuid,                                                               |
| pub full_name: String,                                                      |
| pub image_url: Option<String>,                                              |
| pub is_cohost: bool, // always true — explicit for FE clarity               |
| }                                                                           |
|                                                                             |
| #[derive(Serialize, Clone)]                                                 |
| #[serde(rename_all = "camelCase")]                                          |
| pub struct QuotaDto {                                                       |
| pub daily_limit_seconds: i64,                                               |
| pub used_today_seconds: i64,                                                |
| pub remaining_seconds: i64,                                                 |
| pub resets_at: DateTime<Utc>,                                               |
| }                                                                           |

## **6.3 broadcast_to_dto() — Computing All Fields**

| ASYNC FUNCTION broadcast_to_dto(broadcast, viewer_id, context, state) -> BroadcastDto     |
| ----------------------------------------------------------------------------------------- |
|                                                                                           |
| // ── broadcast_state ─────────────────────────────────────────────                       |
| broadcast_state = {                                                                       |
| let grace = state.redis.exists(format!('host_grace:{}', broadcast.id)).await?             |
| match (&broadcast.status, broadcast.end_time, broadcast.start_time) {                     |
| (Active, _, _) if grace => BroadcastState::Reconnecting,                                  |
| (Active, _, _) => BroadcastState::Live,                                                   |
| (Inactive, Some(_), _) => BroadcastState::Ended,                                          |
| (Inactive, None, Some(st)) if st > Utc::now() => BroadcastState::Scheduled,               |
| _ => BroadcastState::Draft,                                                               |
| }                                                                                         |
| }                                                                                         |
|                                                                                           |
| // ── viewer_role ──────────────────────────────────────────────────                      |
| viewer_role = match viewer_id {                                                           |
| Some(vid) if vid == broadcast.creator_id => ViewerRole::Host,                             |
| Some(vid) => {                                                                            |
| match broadcast_repo.find_listener(broadcast.id, vid).await? {                            |
| Some(l) if l.role == BroadcastRole::Cohost => ViewerRole::Cohost,                         |
| Some(_) => ViewerRole::Listener,                                                          |
| None => ViewerRole::None,                                                                 |
| }                                                                                         |
| }                                                                                         |
| None => ViewerRole::None,                                                                 |
| }                                                                                         |
|                                                                                           |
| // ── viewer_is_in_room ────────────────────────────────────────────                      |
| viewer_is_in_room = viewer_role != ViewerRole::None                                       |
|                                                                                           |
| // ── is_subscribed_to_creator ─────────────────────────────────────                      |
| is_subscribed = match viewer_id {                                                         |
| Some(vid) => subscriber_repo                                                              |
| .is_subscribed(vid, broadcast.creator_id).await?,                                         |
| None => false,                                                                            |
| }                                                                                         |
|                                                                                           |
| // ── live_listener_count ──────────────────────────────────────────                      |
| live_count = state.redis                                                                  |
| .get::<i64>(format!('live_count:{}', broadcast.id)).await                                 |
| .unwrap_or(0)                                                                             |
|                                                                                           |
| // ── duration_seconds ─────────────────────────────────────────────                      |
| duration_secs = match (broadcast.start_time, broadcast.end_time) {                        |
| (Some(s), Some(e)) => Some((e - s).num_seconds()),                                        |
| _ => None,                                                                                |
| }                                                                                         |
|                                                                                           |
| // ── time_remaining_seconds (continue listening only) ─────────────                      |
| time_remaining = if context.is_continue_listening {                                       |
| match (duration_secs, viewer_id) {                                                        |
| (Some(dur), Some(vid)) => {                                                               |
| let pos = broadcast_repo.get_listen_position(broadcast.id, vid).await?                    |
| Some((dur - pos).max(0))                                                                  |
| }                                                                                         |
| _ => None,                                                                                |
| }                                                                                         |
| } else { None }                                                                           |
|                                                                                           |
| BroadcastDto {                                                                            |
| broadcast_state, viewer_role, viewer_is_in_room, is_subscribed_to_creator: is_subscribed, |
| live_listener_count: live_count, duration_seconds: duration_secs,                         |
| time_remaining_seconds: time_remaining,                                                   |
| // ... all other fields mapped 1:1                                                        |
| }                                                                                         |

# **7. Live Listener Counts — Three-Tier Hybrid**

Home page shows 'LIVE 60K', 'LIVE 2.5K' on broadcast cards. Inside a room the count updates in real-time. Here is exactly how to implement both efficiently.

| **Tier**                 | **Storage**        | **When Used**                                | **Accuracy**                   |
| ------------------------ | ------------------ | -------------------------------------------- | ------------------------------ |
| DB total_listeners       | Postgres COUNT(*)  | All list responses (cards, search, discover) | Exact — all-time joins         |
| Redis live_count         | Atomic INCR/DECR   | List responses for active broadcasts         | Near-exact — may lag 1 event   |
| WS numberOfLiveListeners | Computed in memory | Inside an active broadcast room only         | Exact — pushed on every change |

## **7.1 Redis Counter Lifecycle**

| // ── On go_live() ────────────────────────────────────────────────                       |
| ----------------------------------------------------------------------------------------- |
| // Host counts as 1 (they're in the room)                                                 |
| state.redis.set(&format!('live_count:{}', broadcast_id), '1').await?                      |
| // No TTL — will be manually deleted on end_broadcast()                                   |
|                                                                                           |
| // ── On join() ───────────────────────────────────────────────────                       |
| let new_count = state.redis.incr(&format!('live_count:{}', broadcast_id)).await?          |
| // Emit to room participants (inside-room update only)                                    |
| emit_listener_count_update(broadcast_id, new_count, state).await                          |
|                                                                                           |
| // ── On leave() / disconnect ─────────────────────────────────────                       |
| let new_count = state.redis.decr(&format!('live_count:{}', broadcast_id)).await?          |
| // Guard: never go below 0                                                                |
| if new_count < 0 {                                                                        |
| state.redis.set(&format!('live_count:{}', broadcast_id), '0').await?                      |
| }                                                                                         |
| emit_listener_count_update(broadcast_id, new_count.max(0), state).await                   |
|                                                                                           |
| // ── On end_broadcast() ──────────────────────────────────────────                       |
| state.redis.del(&format!('live_count:{}', broadcast_id)).await.ok()                       |
|                                                                                           |
| // ── Batch fetch for list responses (ONE Redis round trip) ────────                      |
| ASYNC FUNCTION batch_get_live_counts(broadcast_ids: &[Uuid], state) -> HashMap<Uuid, i64> |
| let keys: Vec<String> = broadcast_ids                                                     |
| .iter().map(│id│ format!('live_count:{}', id)).collect()                                  |
| let values: Vec<Option<String>> = state.redis.mget(&keys).await?                          |
| broadcast_ids.iter().zip(values)                                                          |
| .filter_map(│(id, v)│ v.and_then(│s│ s.parse().ok()).map(│n│ (*id, n)))                   |
| .collect()                                                                                |
|                                                                                           |
| // ── ASYNC FUNCTION emit_listener_count_update ─────────────────                         |
| ASYNC FUNCTION emit_listener_count_update(broadcast_id, count, state)                     |
| // Only sends to CURRENT ROOM PARTICIPANTS, not the home page.                            |
| // Home page gets the count from the list endpoint's Redis batch fetch.                   |
| let participant_ids = broadcast_repo.get_all_participant_ids(broadcast_id).await?         |
| state.ws_hub.send_to_users(&participant_ids, WsPayload {                                  |
| event: 'numberOfLiveListeners'.into(),                                                    |
| data: json!({ 'broadcastId': broadcast_id, 'count': count }),                             |
| }).await                                                                                  |

# **8. Figma-Discovered Features — Full Implementation**

This section covers all features identified from the Figma designs, including those not in the original spec.

## **8.1 Daily Broadcast Quota**

Figma: 'Remaining time today: 0hr 30min. Your daily broadcast time will reset in 24hrs.' Uses Redis for tracking with zero DB writes per second.

| // Redis strategy: one key per user per day (UTC date in key)                               |
| ------------------------------------------------------------------------------------------- |
| // Key: 'quota:{user_id}:{YYYY-MM-DD}' → seconds used                                       |
| // TTL: 86400 (auto-expires at midnight UTC)                                                |
|                                                                                             |
| #[derive(Serialize, Clone)]                                                                 |
| #[serde(rename_all = "camelCase")]                                                          |
| pub struct QuotaDto {                                                                       |
| pub daily_limit_seconds: i64, // from ENV: DAILY_BROADCAST_LIMIT_SECS (default 1800)        |
| pub used_today_seconds: i64,                                                                |
| pub remaining_seconds: i64,                                                                 |
| pub remaining_display: String, // '0hr 30min' — preformatted for FE                         |
| pub resets_at: DateTime<Utc>, // next midnight UTC                                          |
| }                                                                                           |
|                                                                                             |
| ASYNC FUNCTION get_quota(user_id, state) -> Result<QuotaDto>                                |
| let today = Utc::now().format('%Y-%m-%d').to_string()                                       |
| let key = format!('quota:{}:{}', user_id, today)                                            |
| let used: i64 = state.redis.get(&key).await.unwrap_or(0)                                    |
| let limit: i64 = state.config.daily_broadcast_limit_secs // 1800                            |
| let remaining = (limit - used).max(0)                                                       |
| let resets_at = next_midnight_utc()                                                         |
| QuotaDto {                                                                                  |
| remaining_display: format_duration(remaining), // '0hr 30min'                               |
| ...                                                                                         |
| }                                                                                           |
|                                                                                             |
| // Dedicated endpoint (for the Go live now screen UI):                                      |
| // GET /broadcasts/quota → QuotaDto                                                         |
| // AUTH REQUIRED                                                                            |
|                                                                                             |
| // Quota check in go_live() service — BEFORE creating LiveKit room:                         |
| ASYNC FUNCTION check_and_start(broadcast_id, user_id, state) -> Result<BroadcastSessionDto> |
| let quota = get_quota(user_id, state).await?                                                |
| if quota.remaining_seconds <= 0 {                                                           |
| return Err(BroadcastError::DailyQuotaExceeded {                                             |
| resets_in_mins: (quota.resets_at - Utc::now()).num_minutes() as u32,                        |
| })                                                                                          |
| }                                                                                           |
| // ... rest of go_live() logic ...                                                          |
| // Store broadcast start time for later deduction:                                          |
| state.redis.set_ex(                                                                         |
| &format!('broadcast_start:{}', broadcast_id),                                               |
| Utc::now().timestamp().to_string(),                                                         |
| 86400,                                                                                      |
| ).await?                                                                                    |
|                                                                                             |
| // Deduct on end_broadcast():                                                               |
| ASYNC FUNCTION deduct_quota(broadcast_id, user_id, state)                                   |
| let start: i64 = state.redis.get(format!('broadcast_start:{}', broadcast_id))               |
| .await.unwrap_or(Utc::now().timestamp())                                                    |
| let elapsed = (Utc::now().timestamp() - start).max(0)                                       |
| let today = Utc::now().format('%Y-%m-%d').to_string()                                       |
| let key = format!('quota:{}:{}', user_id, today)                                            |
| state.redis.incr_by(&key, elapsed).await?                                                   |
| state.redis.expire(&key, 86400).await?                                                      |
| state.redis.del(&format!('broadcast_start:{}', broadcast_id)).await.ok()                    |
|                                                                                             |
| // Mid-broadcast quota watcher (spawned in go_live()):                                      |
| // Checks every 60s. Emits broadcastError{DAILY_QUOTA_EXCEEDED} and ends broadcast          |
| // when quota is exhausted during a live session.                                           |

## **8.2 Recording ****&**** Publish Flow**

Figma flow: End broadcast → 'Your live broadcast is complete! Great job.' → 'Publish broadcast' → progress spinner → 'Broadcast Published! Now you and other people can go back and listen to this broadcast.'

| // DB columns needed (migration 0002):                                                  |
| --------------------------------------------------------------------------------------- |
| // broadcasts.recording_enabled BOOLEAN DEFAULT false                                   |
| // broadcasts.recording_key TEXT -- S3 object key                                       |
| // broadcasts.recording_url TEXT -- public/presigned URL                                |
| // broadcasts.published_at TIMESTAMPTZ                                                  |
|                                                                                         |
| // ── Step 1: Toggle recording in CreateBroadcastDto ───────────────                    |
| pub struct CreateBroadcastDto {                                                         |
| // ... other fields ...                                                                 |
| pub recording_enabled: Option<bool>, // default false                                   |
| }                                                                                       |
|                                                                                         |
| // ── Step 2: Start LiveKit Egress on go_live() ────────────────────                    |
| ASYNC FUNCTION start_egress_if_enabled(broadcast, state) -> Result<()>                  |
| if !broadcast.recording_enabled { return Ok(()) }                                       |
| let recording_key = format!('recordings/{}/{}.mp4', broadcast.id, Uuid::new_v4())       |
| let egress_id = livekit_service.start_room_composite_egress(                            |
| broadcast.id.to_string(),                                                               |
| S3EgressOutput {                                                                        |
| access_key: state.config.s3_access_key,                                                 |
| secret: state.config.s3_secret_key,                                                     |
| region: state.config.s3_region,                                                         |
| bucket: state.config.s3_bucket,                                                         |
| key: recording_key.clone(),                                                             |
| }                                                                                       |
| ).await?                                                                                |
| // Store recording_key in DB immediately so we can track it                             |
| broadcast_repo.set_recording_key(broadcast.id, &recording_key).await?                   |
| // Store egress_id in Redis for webhook correlation                                     |
| state.redis.set_ex(&format!('egress:{}', broadcast.id), egress_id, 86400).await?        |
|                                                                                         |
| // ── Step 3: LiveKit Egress webhook → POST /webhooks/livekit ─────                     |
| ASYNC FUNCTION handle_livekit_webhook(event, state)                                     |
| match event.event.as_str() {                                                            |
| 'egress_ended' => {                                                                     |
| // LiveKit sends the S3 download URL in the event                                       |
| let broadcast_id = parse_uuid(&event.room_name)?                                        |
| let s3_url = event.file_results.first()?.download_url.clone()?                          |
| // Cache in Redis so publish endpoint can fetch it                                      |
| state.redis.set_ex(                                                                     |
| &format!('recording_ready:{}', broadcast_id),                                           |
| &s3_url,                                                                                |
| 86400 * 7, // 7 days                                                                    |
| ).await?                                                                                |
| // Notify host that recording is ready to publish                                       |
| let b = broadcast_repo.find_by_id(broadcast_id).await?                                  |
| if let Some(b) = b {                                                                    |
| state.ws_hub.send_to_user(b.creator_id, WsPayload {                                     |
| event: 'recordingReady'.into(),                                                         |
| data: json!({ 'broadcastId': broadcast_id }),                                           |
| }).await                                                                                |
| }                                                                                       |
| }                                                                                       |
| _ => {}                                                                                 |
| }                                                                                       |
|                                                                                         |
| // ── Step 4: POST /broadcasts/:id/publish ────────────────────────                     |
| ASYNC FUNCTION publish(broadcast_id, user_id, state) -> Result<BroadcastDto>            |
| let b = broadcast_repo.find_by_id(broadcast_id).await?.ok_or(NotFound)?                 |
| if b.creator_id != user_id { return Err(NotCreator) }                                   |
| if b.status == Active { return Err(BroadcastStillLive) }                                |
| if !b.recording_enabled { return Err(RecordingNotAvailable) }                           |
| if b.published_at.is_some() { return Ok(BroadcastDto::from(b)) } // idempotent          |
|                                                                                         |
| let recording_url = state.redis                                                         |
| .get::<String>(&format!('recording_ready:{}', broadcast_id)).await                      |
| .map_err(│_│ BroadcastError::RecordingNotAvailable)?                                    |
|                                                                                         |
| let b = broadcast_repo.set_published(broadcast_id, &recording_url).await?               |
| Ok(broadcast_to_dto(b, Some(user_id), Default::default(), state).await?)                |
|                                                                                         |
| // ── GET /broadcasts/:id/recording-url ───────────────────────────                     |
| // Returns a presigned S3 URL (valid 24h) so the URL in recording_url                   |
| // is not permanently public.                                                           |
| ASYNC FUNCTION get_recording_url(broadcast_id, user_id, state) -> Json<RecordingUrlDto> |
| let b = broadcast_repo.find_by_id(broadcast_id).await?.ok_or(NotFound)?                 |
| let key = b.recording_key.ok_or(RecordingNotAvailable)?                                 |
| let url = s3_service.presign_get_url(&key, Duration::from_secs(86400)).await?           |
| Json(RecordingUrlDto {                                                                  |
| url,                                                                                    |
| expires_at: Utc::now() + chrono::Duration::hours(24),                                   |
| })                                                                                      |

## **8.3 Listen Later (Bookmarks)**

Figma context menu: Go to profile · Unsubscribe · Listen later · Share · Copy link. 'Listen later' is a bookmark feature.

| // DB Schema:                                                           |
| ----------------------------------------------------------------------- |
| CREATE TABLE broadcast_bookmarks (                                      |
| user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,           |
| broadcast_id UUID NOT NULL REFERENCES broadcasts(id) ON DELETE CASCADE, |
| saved_at TIMESTAMPTZ DEFAULT now(),                                     |
| PRIMARY KEY (user_id, broadcast_id)                                     |
| )                                                                       |
|                                                                         |
| // Endpoints:                                                           |
| POST /broadcasts/:id/bookmark AUTH — save to listen later               |
| DELETE /broadcasts/:id/bookmark AUTH — remove bookmark                  |
| GET /broadcasts/listen-later AUTH — paginated list of bookmarks         |
|                                                                         |
| // The BroadcastDto gets one new field:                                 |
| pub is_bookmarked: bool, // viewer has bookmarked this broadcast        |
|                                                                         |
| // Response for GET /broadcasts/listen-later                            |
| // Returns PaginatedResponse<BroadcastDto> with the same full DTO.      |
| // sorted by saved_at DESC                                              |

## **8.4 Continue Listening — Time Remaining**

Figma: '45mins left', '5mins left', '10mins left' on Continue listening cards. Needs per-user listen position tracking.

| // DB column addition:                                                                                |
| ----------------------------------------------------------------------------------------------------- |
| ALTER TABLE broadcast_listeners                                                                       |
| ADD COLUMN IF NOT EXISTS last_listen_position_seconds INT NOT NULL DEFAULT 0,                         |
| ADD COLUMN IF NOT EXISTS last_listened_at TIMESTAMPTZ                                                 |
|                                                                                                       |
| // Update listen position — called on leaveBroadcast WS event:                                        |
| ASYNC FUNCTION update_listen_position(broadcast_id, user_id, position_secs, state)                    |
| broadcast_repo.update_listener_position(                                                              |
| broadcast_id, user_id, position_secs                                                                  |
| ).await?                                                                                              |
|                                                                                                       |
| // The leaveBroadcast WS message payload now includes position:                                       |
| // FE sends: { 'event': 'leaveBroadcast', 'data': { 'broadcastId': '...', 'positionSeconds': 1234 } } |
| // BE handles_client_message() → extracts positionSeconds → updates DB                                |
|                                                                                                       |
| // time_remaining_seconds in BroadcastDto (continue-listening context):                               |
| // = broadcast.duration_seconds - listener.last_listen_position_seconds                               |
|                                                                                                       |
| // GET /broadcasts/continue-listening response:                                                       |
| // Returns broadcasts where:                                                                          |
| // - viewer has a broadcast_listeners row (joined before), AND                                        |
| // - broadcast ended < 7 days ago OR has a recording                                                  |
| // Sorted by last_listened_at DESC                                                                    |

## **8.5 Now Playing Mini-Player**

Figma: home screen shows 'Now streaming' banner at the top ('Rabbit: the need for a Bible Teacher | Join'). This requires an active session endpoint.

| // GET /broadcasts/active-session                                             |
| ----------------------------------------------------------------------------- |
| // AUTH REQUIRED                                                              |
| // Returns the broadcast the viewer is currently in (if any)                  |
| // Used by the home screen to show the mini-player 'Now streaming' bar        |
|                                                                               |
| ASYNC FUNCTION get_active_session(user_id, state) -> Option<ActiveSessionDto> |
| // Find any active broadcast_listeners row for this user                      |
| let session = broadcast_repo                                                  |
| .find_active_session_for_user(user_id).await?                                 |
|                                                                               |
| session.map(│s│ ActiveSessionDto {                                            |
| broadcast_id: s.broadcast_id,                                                 |
| broadcast_title: s.title,                                                     |
| creator_name: s.creator_name,                                                 |
| image_url: s.image_url,                                                       |
| role: s.role, // HOST │ COHOST │ LISTENER                                     |
| live_listener_count: redis_get_count(s.broadcast_id).await?,                  |
| joined_at: s.joined_at,                                                       |
| })                                                                            |
|                                                                               |
| #[derive(Serialize)]                                                          |
| #[serde(rename_all = "camelCase")]                                            |
| pub struct ActiveSessionDto {                                                 |
| pub broadcast_id: Uuid,                                                       |
| pub broadcast_title: String,                                                  |
| pub creator_name: String,                                                     |
| pub image_url: Option<String>,                                                |
| pub role: BroadcastRole,                                                      |
| pub live_listener_count: i64,                                                 |
| pub joined_at: DateTime<Utc>,                                                 |
| }                                                                             |
|                                                                               |
| // FE calls this on app foreground to restore the mini-player.                |
| // Returns 200 with null data if not in any broadcast.                        |

## **8.6 Cohost Invitation — Two-Step Flow**

Figma: '[Creator] wants to add you as a co-host. Accept request / Decline'. The v1 docs immediately inserted the cohost — this adds the proper invite → accept/decline flow.

| // DB Schema:                                                               |
| --------------------------------------------------------------------------- |
| CREATE TABLE cohost_invitations (                                           |
| id UUID PRIMARY KEY DEFAULT gen_random_uuid(),                              |
| broadcast_id UUID NOT NULL REFERENCES broadcasts(id) ON DELETE CASCADE,     |
| inviter_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,            |
| invitee_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,            |
| status TEXT NOT NULL DEFAULT 'pending', -- pending │ accepted │ declined    |
| created_at TIMESTAMPTZ DEFAULT now(),                                       |
| responded_at TIMESTAMPTZ,                                                   |
| UNIQUE (broadcast_id, invitee_id)                                           |
| )                                                                           |
|                                                                             |
| // POST /broadcasts/:id/co-hosts                                            |
| // Now creates an INVITATION (not a direct cohost insert)                   |
| // Emits WS 'cohostInvitation' to invitee:                                  |
| // { broadcastId, invitationId, inviterName, broadcastTitle }               |
| // Creates AddedAsCoHost notification for invitee                           |
|                                                                             |
| // POST /broadcasts/:id/co-hosts/:invId/accept                              |
| // AUTH = invitee only                                                      |
| // → updates invitation status = 'accepted'                                 |
| // → inserts into broadcast_cohosts                                         |
| // → if broadcast is live: mint COHOST token, upsert broadcast_listeners    |
| // → emits WS 'newCohost' to invitee with { broadcastId, livekitToken }     |
| // → emits WS 'cohostAccepted' to creator with { broadcastId, userId }      |
| // Returns: CohostSessionDto { user, livekitToken? }                        |
|                                                                             |
| // POST /broadcasts/:id/co-hosts/:invId/decline                             |
| // AUTH = invitee only                                                      |
| // → updates invitation status = 'declined'                                 |
| // → emits WS 'cohostDeclined' to creator with { broadcastId, userId }      |
|                                                                             |
| // New WS events:                                                           |
| // cohostInvitation → sent to invitee (shows 'Accept request' bottom sheet) |
| // cohostAccepted → sent to creator                                         |
| // cohostDeclined → sent to creator                                         |

## **8.7 Broadcast Context Menu**

Figma shows a '...' menu on broadcast cards and inside broadcasts: Go to profile, Unsubscribe, Listen later, Share, Copy link, Minimize stream. All of these are FE actions except the bookmark. No new BE endpoints needed beyond the bookmark endpoints.

| // Context menu action mapping:                                     |
| ------------------------------------------------------------------- |
| // 'Go to profile' → FE navigates to GET /users/:creatorId          |
| // 'Subscribe' → POST /subscribers/:creatorId                       |
| // 'Unsubscribe' → DELETE /subscribers/:creatorId                   |
| // 'Listen later' → POST /broadcasts/:id/bookmark                   |
| // 'Remove bookmark' → DELETE /broadcasts/:id/bookmark              |
| // 'Share' → FE generates deeplink (no BE needed)                   |
| // 'Copy link' → FE copies deeplink to clipboard                    |
| // 'Minimize stream' → FE: enters picture-in-picture mode (pure FE) |
|                                                                     |
| // The BroadcastDto v2 already returns is_subscribed_to_creator     |
| // and is_bookmarked, so the FE knows which label to show           |
| // (Subscribe vs Unsubscribe, Listen later vs Remove bookmark)      |
| // without any extra API call.                                      |

# **9. Complete DB Migrations (v2)**

| -- packages/db/migrations/0002_broadcast_v2.sql                               |
| ----------------------------------------------------------------------------- |
|                                                                               |
| -- Recording support                                                          |
| ALTER TABLE broadcasts                                                        |
| ADD COLUMN IF NOT EXISTS recording_enabled BOOLEAN NOT NULL DEFAULT false,    |
| ADD COLUMN IF NOT EXISTS recording_key TEXT,                                  |
| ADD COLUMN IF NOT EXISTS recording_url TEXT,                                  |
| ADD COLUMN IF NOT EXISTS published_at TIMESTAMPTZ,                            |
| ADD COLUMN IF NOT EXISTS end_reason TEXT;                                     |
|                                                                               |
| -- Listen position tracking (for 'X mins left')                               |
| ALTER TABLE broadcast_listeners                                               |
| ADD COLUMN IF NOT EXISTS last_listen_position_seconds INT NOT NULL DEFAULT 0, |
| ADD COLUMN IF NOT EXISTS last_listened_at TIMESTAMPTZ;                        |
|                                                                               |
| -- Listen later / bookmarks                                                   |
| CREATE TABLE IF NOT EXISTS broadcast_bookmarks (                              |
| user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,                 |
| broadcast_id UUID NOT NULL REFERENCES broadcasts(id) ON DELETE CASCADE,       |
| saved_at TIMESTAMPTZ NOT NULL DEFAULT now(),                                  |
| PRIMARY KEY (user_id, broadcast_id)                                           |
| )                                                                             |
|                                                                               |
| -- Cohost invitations                                                         |
| CREATE TABLE IF NOT EXISTS cohost_invitations (                               |
| id UUID PRIMARY KEY DEFAULT gen_random_uuid(),                                |
| broadcast_id UUID NOT NULL REFERENCES broadcasts(id) ON DELETE CASCADE,       |
| inviter_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,              |
| invitee_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,              |
| status TEXT NOT NULL DEFAULT 'pending',                                       |
| created_at TIMESTAMPTZ NOT NULL DEFAULT now(),                                |
| responded_at TIMESTAMPTZ,                                                     |
| UNIQUE (broadcast_id, invitee_id)                                             |
| )                                                                             |
|                                                                               |
| -- Performance indexes                                                        |
| CREATE INDEX IF NOT EXISTS idx_broadcasts_fts                                 |
| ON broadcasts USING GIN(to_tsvector('english', title ││ ' ' ││ description))  |
| WHERE deleted_at IS NULL;                                                     |
|                                                                               |
| CREATE INDEX IF NOT EXISTS idx_broadcasts_creator_status                      |
| ON broadcasts (creator_id, status) WHERE deleted_at IS NULL;                  |
|                                                                               |
| CREATE INDEX IF NOT EXISTS idx_broadcasts_end_time                            |
| ON broadcasts (end_time DESC NULLS LAST)                                      |
| WHERE status = 'inactive' AND deleted_at IS NULL;                             |
|                                                                               |
| CREATE INDEX IF NOT EXISTS idx_broadcast_listeners_user                       |
| ON broadcast_listeners (listener_id);                                         |
|                                                                               |
| CREATE INDEX IF NOT EXISTS idx_bookmarks_user                                 |
| ON broadcast_bookmarks (user_id, saved_at DESC);                              |
|                                                                               |
| -- NotificationType additions                                                 |
| ALTER TYPE "NotificationType"                                                 |
| ADD VALUE IF NOT EXISTS 'cohostInvitation',                                   |
| ADD VALUE IF NOT EXISTS 'recordingReady',                                     |
| ADD VALUE IF NOT EXISTS 'quotaWarning';                                       |

# **10. Complete Endpoint ****&**** WS Event Reference**

## **10.1 HTTP Endpoints**

| **Method** | **Path**                                | **Auth** | **Description**                                                             |
| ---------- | --------------------------------------- | -------- | --------------------------------------------------------------------------- |
| GET        | /broadcasts                             | —        | Full search (id, status, keywords, creator_id, sort_by, order, page, limit) |
| GET        | /broadcasts/live-for-you                | ✓        | Active broadcasts from subscriptions                                        |
| GET        | /broadcasts/now-live                    | —        | All active broadcasts, sorted by listeners                                  |
| GET        | /broadcasts/recently-live               | —        | Ended <24h, sorted by end_time                                              |
| GET        | /broadcasts/continue-listening          | ✓        | Viewer's past joins with time_remaining_seconds                             |
| GET        | /broadcasts/listen-later                | ✓        | Viewer's bookmarked broadcasts                                              |
| GET        | /broadcasts/active-session              | ✓        | Viewer's current active broadcast (mini-player)                             |
| GET        | /broadcasts/quota                       | ✓        | Creator's remaining broadcast time today                                    |
| GET        | /broadcasts/:id                         | —        | Single broadcast with full state signals                                    |
| POST       | /broadcasts                             | ✓        | Create broadcast (draft or scheduled)                                       |
| PUT        | /broadcasts/:id                         | ✓        | Partial update (title, description, etc.)                                   |
| DELETE     | /broadcasts/:id                         | ✓        | Soft delete                                                                 |
| PUT        | /broadcasts/:id/go-live                 | ✓        | Start broadcast → returns livekit_token                                     |
| POST       | /broadcasts/:id/join                    | ✓        | Join → returns livekit_token                                                |
| POST       | /broadcasts/:id/token                   | ✓        | Refresh LiveKit token                                                       |
| POST       | /broadcasts/:id/keepalive               | ✓        | HTTP keepalive for background apps                                          |
| POST       | /broadcasts/:id/publish                 | ✓        | Publish recording after end                                                 |
| GET        | /broadcasts/:id/recording-url           | ✓        | Presigned S3 URL (24h)                                                      |
| GET        | /broadcasts/:id/listeners               | —        | All-time listeners (paginated)                                              |
| GET        | /broadcasts/:id/live-listeners          | —        | Current LiveKit participants                                                |
| POST       | /broadcasts/:id/bookmark                | ✓        | Save to Listen Later                                                        |
| DELETE     | /broadcasts/:id/bookmark                | ✓        | Remove bookmark                                                             |
| POST       | /broadcasts/:id/co-hosts                | ✓        | Invite a co-host (creates invitation)                                       |
| DELETE     | /broadcasts/:id/co-hosts/:userId        | ✓        | Remove co-host                                                              |
| POST       | /broadcasts/:id/co-hosts/:invId/accept  | ✓        | Accept cohost invitation                                                    |
| POST       | /broadcasts/:id/co-hosts/:invId/decline | ✓        | Decline cohost invitation                                                   |
| POST       | /webhooks/livekit                       | SIG      | LiveKit event webhook (signature verified)                                  |

## **10.2 WebSocket Events — Client → Server**

| **Event**      | **Payload**                      | **Description**                                          |
| -------------- | -------------------------------- | -------------------------------------------------------- |
| endBroadcast   | { broadcastId }                  | Host ends the broadcast                                  |
| leaveBroadcast | { broadcastId, positionSeconds } | Listener leaves (positionSeconds for continue-listening) |
| heartbeat      | { broadcastId }                  | Participant keepalive every 60s                          |

## **10.3 WebSocket Events — Server → Client**

| **Event**              | **Sent To**            | **Payload**                                                        |
| ---------------------- | ---------------------- | ------------------------------------------------------------------ |
| newBroadcast           | Subscribers of creator | BroadcastDto                                                       |
| endedBroadcast         | Room participants      | { broadcastId, reason: EndReason }                                 |
| hostDisconnected       | Room participants      | { broadcastId, gracePeriodSecs, disconnectCount }                  |
| hostReconnected        | Room participants      | { broadcastId }                                                    |
| newBroadcastListener   | Host + cohosts         | UserSummaryDto + broadcastId                                       |
| broadcastListenerLeft  | Host + cohosts         | { userId, broadcastId }                                            |
| numberOfLiveListeners  | Room participants      | { broadcastId, count }                                             |
| numberOfLiveBroadcasts | All connected clients  | { count }                                                          |
| cohostInvitation       | Invitee                | { broadcastId, invitationId, inviterName, broadcastTitle }         |
| cohostAccepted         | Creator                | { broadcastId, userId, userName }                                  |
| cohostDeclined         | Creator                | { broadcastId, userId }                                            |
| newCohost              | Accepted invitee       | { broadcastId, livekitToken }                                      |
| removedCohost          | Removed cohost         | { broadcastId }                                                    |
| recordingReady         | Creator                | { broadcastId }                                                    |
| broadcastError         | Targeted user          | BroadcastWsError { broadcastId, code, message, recoverable, data } |
| notification           | Targeted user          | NotificationDto                                                    |

Page · Meno Engineering — Confidential
