# Broadcast gap analysis

The highest-value gaps between the broadcast module as it exists on `refactor/monorepo` and the v2 contract in `docs/meno-broadcast-v2-final.md`, plus the places where the doc itself cannot be implemented as written.

**Scope:** backend (`apps/api`) and mobile Flutter (`apps/mobile`).
**Reference:** `docs/meno-broadcast-v2-final.md` only. Code comments that cite §4.2 / §4.4 / §4.8 / §9.1 are citing `docs/refactor-plan-document.md` (error taxonomy, rate limiting, observability, logging), not this spec.
**Method:** every claim below was read in the tree on 2026-10-23 and is evidenced with `path:line`. Nothing here is inferred from filenames.

Companion documents:

- [broadcast-contract-snapshot.md](broadcast-contract-snapshot.md) — the field-by-field index this analysis measures against.
- [broadcast-implementation-order.md](broadcast-implementation-order.md) — the order to close these gaps in.

---

## 1. What the doc requires, in its own section numbers

| Doc section | Requirement | In scope for Tier 1? |
| --- | --- | --- |
| §1.2 | Session response `{ broadcast, livekit_token }`, no URL, ENV-only LiveKit | yes |
| §2.1 | Failure taxonomy: double join, deleted-between, simultaneous end, quota mid-broadcast, token expiry | yes |
| §2.2 | Heartbeat 25s / host 60s / participant 20s / 2 misses | yes |
| §2.3 | Tiered host grace 120/90/60/30 with Redis keys and a watcher | yes |
| §2.4 | Offline message buffer, 50 events, 5 min, atomic drain | yes |
| §2.5 | Reconnect rate limit, 10/60s, 429 before upgrade | yes |
| §3.1 | `BroadcastError` taxonomy with machine-readable codes + `data` | yes |
| §3.2 | `BroadcastWsError` runtime codes and `recoverable` | yes |
| §4.1 | HTTP keepalive for backgrounded apps | yes |
| §5.1–§5.3 | Search/filter/sort/pagination | no |
| §6.1–§6.3 | `BroadcastDto` v2 and its derivations | yes |
| §7.1 | Three-tier participant counts (DB total, Redis live, WS delta) | yes |
| §8.1 | Daily quota | no (Tier 3) |
| §8.2 | Recording + publish | no (Tier 4) |
| §8.3–§8.5 | Listen Later, Continue Listening, mini-player | no (Tier 4) |
| §8.6–§8.7 | Cohost invitations, context menu | no (Tier 4) |
| §9 | DB migration set | no (per-tier) |
| §10.1–§10.3 | Endpoint and event catalogue | yes |

---

## 2. Confirmed gaps

Ordered by what blocks a correct client, not by effort.

### 2.1 Response casing (blocking)

`BroadcastResponse` serialises camelCase (`apps/api/src/modules/broadcast/dto.rs:299-300`). The mobile DTO reads snake_case keys (`apps/mobile/lib/features/broadcast/model/dtos.dart:19-41`), and the generated parser confirms it (`apps/mobile/lib/features/broadcast/model/dtos.g.dart:14` — `json['time_zone']`). `BroadcastHttpService._map` only checks that the value is a map; there is no key normalisation anywhere in the client (`services/broadcast_http_service.dart:226-234`, and no transform in `_shared/services/http_client.dart`).

Consequence: every multi-word field arrives as `null`/default. `timeZone` null, `imageUrl` null, `publishedAt` null, `participantRole` `'none'`, `isJoined` false, `liveParticipantsCount` 0, `totalParticipants` 0, `recordingEnabled` false, `endReason` `'none'`, `timeRemainingSeconds` null, and nested `fullName` null (`dtos.dart:152`). Single-word fields (`id`, `title`, `description`, `status`, `state`) survive, which is exactly what makes this look like a working screen in a demo.

This is the first thing to fix because every other contract test is meaningless until the client parses what the server sends. The earlier snapshot asserted the mobile "carries the fields the session path needs" — that was reading the Dart field list, not the JSON keys.

Request bodies are a different story and currently correct: `CreateBroadcastRequest` / `UpdateBroadcastRequest` have no `rename_all`, so they expect snake_case (`dto.rs:70-71`, `:116`), which is what mobile sends (`broadcast_http_service.dart:83-91`, `:110-121`). `BroadcastQuery` is camelCase and mobile sends camelCase (`broadcast_query.dart:280-286`). The one request-side miss is `DELETE /cohosts/{id}`, where mobile sends `remove_from_room` and the backend expects `removeFromRoom` (`dto.rs:185-192`); it is masked by the default being `true`.

### 2.2 DTO field names and nullability diverge from §6.2

Full table in [the snapshot §2.2](broadcast-contract-snapshot.md#22-field-by-field-mapping-62). The short list of missing fields on the repo DTO: `creator_id`, `is_subscribed_to_creator`, `last_listened_at`, `creator_quota`; of differently-named fields: `state`/`broadcast_state`, `participant_role`/`viewer_role`, `is_joined`/`viewer_is_in_room`; of different nullability: `description` and `time_zone` are optional in the repo and non-optional in the doc, `creator` is non-optional in the repo and optional in the doc, `end_reason` is non-optional in the repo with an extra `none` variant and optional in the doc.

The `none` end reason is worth calling out separately: `EndReason::None` serialises as `"none"` (`model.rs:246-268`), and the mobile's label map renders it as "Broadcast ended" (`entities.dart:485`). A live broadcast therefore has a field that says it ended.

### 2.3 Enum casing: SCREAMING_SNAKE in the doc, lowercase in both implementations

`BroadcastState` (`model.rs:313`), `ParticipantRole` (`:183`) and `EndReason` (`:257`) are all `rename_all = "snake_case"` / `"lowercase"`, and mobile matches them (`entities.dart:315-320`, `:638-642`, `:469-474`). §6.2 specifies `SCREAMING_SNAKE_CASE` for all three. The two implementations agree with each other and disagree with the doc, so this is a doc change or a two-sided rename — but not a silent third spelling.

After the participant-vocabulary decision (see snapshot §8) the doc and both implementations also agree on `PARTICIPANT` vs `participant`; only the casing differs: doc SCREAMING_SNAKE, both implementations lowercase.

### 2.4 `RECONNECTING` is defined and never produced

`Broadcast::state()` returns `Live`, `Ended`, `Scheduled`, or `Draft` from the row (`model.rs:94-104`), and its own comment says `Reconnecting` needs a fact from Redis and belongs to the service. No service code constructs it: `grep Reconnecting apps/api/src` returns only the definition and the comment. So `GET /broadcasts/{id}`, the list endpoints, and every session response say `live` while the doc says the client must be able to render "host reconnecting" (§6.1, §6.2). The mobile has the state in its enum (`entities.dart:317`) and a UI branch for it, which is currently dead code.

The underlying grace machinery is also not wired: `GracePeriodConfig` (120/90/60/30) and the Redis key names exist (`infrastructure/ws/model.rs:311-345`, `infrastructure/redis/keys.rs:85-99`) but nothing increments or reads them, and `routes/ws.rs:40-43` states that room join/leave and the host grace period are not attached yet. `host_disconnected(…)` is only called from tests.

What does exist is a different, working design: the reaper (`modules/broadcast/reaper.rs`) sweeps live rows, reads **LiveKit room presence**, stores `abandoned_since` in Postgres, and ends after a flat 10 minutes (`modules/broadcast/reaper.rs:59`); a connected cohost counts as healthy. That design solves two problems the doc's design has (process restart loses the timer; a host connected-but-not-publishing is invisible) and lacks two things the doc has (per-host tiers; a `RECONNECTING` signal to clients). Reconciling them is Tier 1/2 work, not a rewrite.

### 2.5 Live count is a DB query, not the Redis counter §7 requires

`live_participants_count` comes from `COUNT(*) … left_at IS NULL` (`repository/pg.rs:400-409`), computed per row in `viewer_for` (`service.rs:858`). §7 specifies a Redis `live_count:{bid}` with INCR/DECR on join/leave, a batch MGET for list responses, and an in-room WS delta.

Consequences: (a) a client that dies without sending leave stays counted until the reaper closes something (it does not close participant rows); (b) list responses pay one query per broadcast; (c) the in-room delta event does not exist, so the room cannot update its own count without polling. The Redis keys are already named (`keys.rs:85`) but unused.

§7 also has a leak of its own: `live_count` is created with **no TTL** and only deleted on `end_broadcast`. If the process dies before end, the key survives forever. See D-21.

### 2.6 Heartbeat numbers differ, and the doc's numbers are impossible

Repo: 25s ping, **90s** host, **60s** for everyone else (`listener_pong_timeout_secs` internally), 2 misses (`infrastructure/ws/model.rs:264-292`), with an `is_sane()` invariant (`:295-307`) that requires `max_missed × ping ≤ participant_timeout`.
Doc: 25s ping, **60s** host, **20s** participant (`participant_pong_timeout`), 2 misses.

The doc's participant numbers cannot work: the read loop treats a 20s wait for *any* inbound frame as a disconnect, which is shorter than the 25s ping interval and far shorter than the 50s that "2 missed pings" implies. A quiet participant is dropped between pings. Repo's values satisfy the invariant; doc's do not (D-06). Pick the repo's numbers and fix the doc, or fix the doc to another sane triple.

### 2.7 The token-refresh endpoint is missing and the socket error points at it

§2.1 and §3.2 require `TOKEN_EXPIRED` → call the refresh endpoint. §10.1 names `POST /broadcasts/:id/token`. `WsErrorCode::TokenExpired`'s doc comment even names that path (`infrastructure/ws/errors.rs:92-95`). No such route exists (`apps/api/src/routes/broadcasts.rs:77-136`). Mobile's only recovery is `POST /resume` (`broadcast_http_service.dart:166-183`), which also re-opens the participant row — a different operation from refreshing a token.

Token TTL is 15 minutes (`apps/api/src/config.rs:195`), so this is not a hypothetical: a long broadcast will hit expiry every quarter hour, and today the client has no defined action when the socket says so.

### 2.8 Keepalive endpoint missing

§4.1 defines `POST /broadcasts/:id/keepalive` for the backgrounded case, including the rule that a host keepalive during grace is a reconnect. Absent. The background-app story therefore rests entirely on `/resume` at launch, which is a recovery, not a keepalive: it does not extend grace, and it does not tell the client its token's remaining life.

### 2.9 WebSocket catalogue drift

Server (`infrastructure/ws/model.rs:30-116`) versus doc §10.3 versus mobile (`_shared/model/socket.dart:14-37`):

| Concept | Doc | Server | Mobile |
| --- | --- | --- | --- |
| participant joined | `participantJoined` | `participantJoined` | — |
| participant left | `participantLeft` | `participantLeft` | `broadcastListenerLeft` |
| live count | `numberOfLiveParticipants` | `numberOfLiveParticipants` | — |
| host grace | `hostDisconnected` | `hostDisconnected` | `hostDisconnected` |
| grace key spelling | `gracePeriodSecs` | `gracePeriodInSecs` (`dto.rs:110-117`) | reader accepts both |
| runtime error | `broadcastError` | `broadcastError` | **none** |
| host ends | client sends `endBroadcast` | not accepted | `endBroadcast` |
| participant leaves | client sends `leaveBroadcast` + position | not accepted | `leaveBroadcast` |
| room join | — | answers "not available yet" | `joinBroadcast`, `joinRoom` |

The remaining gaps are handlers, payloads and direction, not the noun:

- **Mobile has no `broadcastError` handler**, so `TOKEN_EXPIRED`, `BROADCAST_FORCIBLY_ENDED` and `KICKED_FROM_ROOM` are dropped on the floor. This is the mechanism by which token expiry is supposed to be solved, and the client is not listening.
- **`leaveBroadcast.positionSeconds` never reaches the server**, because the server does not accept the event; it only accepts `POST /leave`, which carries no position (`broadcast_http_service.dart:185-193`). Continue-listening cannot work through the current wire, independent of the missing `/continue-listening` endpoint.

The client's defensive two-spelling reading of `endedBroadcast` (`apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart:174-183`, `broadcast_id` **or** `broadcastId`) exists because the backend used to send both. The backend now sends camelCase only. The reader can be narrowed once the casing fix in 2.1 lands.

### 2.10 Endpoint surface drift

Present: `GET /broadcasts`, `GET /broadcasts/{id}`, `GET /broadcasts/{id}/participants`, `POST /broadcasts`, `PATCH /broadcasts/{id}`, `DELETE /broadcasts/{id}`, `POST /broadcasts/{id}/live`, `POST /{id}/end`, `POST /{id}/join`, `POST /{id}/resume`, `POST /{id}/leave`, `POST /{id}/cohosts`, `DELETE /{id}/cohosts/{cohostId}`, `POST /{id}/image` (`routes/broadcasts.rs:77-136`).

Absent from the repo but in §10.1: `/live-for-you`, `/now-live`, `/recently-live`, `/continue-listening`, `/listen-later`, `/active-session`, `/quota`, `/token`, `/keepalive`, `/publish`, `/recording-url`, `/live-participants`, `/bookmark` (POST/DELETE), `/co-hosts/{invId}/accept`, `/co-hosts/{invId}/decline`, `/webhooks/livekit`.

Spelling mismatches even where both exist: `POST /live` vs `PUT /go-live`; `PATCH` vs `PUT`; `/cohosts` vs `/co-hosts`; the doc splits `/participants` (all-time) and `/live-participants` (current) where the repo has one `/participants`.

The doc's §10.1 also omits `POST /end` and `POST /leave`, which the repo has and the mobile uses. §10.2 instead makes both WebSocket client events. Three surfaces now exist for ending a broadcast (HTTP, WS, grace) and only one is described fully.

### 2.11 Error mapping diverges

- Per-field validation codes (`MISSING_TITLE`, `DESCRIPTION_TOO_LONG`) do not exist. The repo returns one `VALIDATION_FAILED` carrying per-field detail (`crates/core/src/error.rs:173`, `:267`). A client switching on the doc's codes matches nothing; a client switching on `VALIDATION_FAILED` has to read `data.fields`. Pick one model and put it in §3.1.
- `NOT_LIVE` is 400 in §3.1, 409 in the repo (`error.rs:78-84`).
- `ALREADY_LIVE` is 400 in §3.1 and 409 in §2.1 — the doc contradicts itself.
- `JOIN_IN_PROGRESS` is 400 in §3.1, 409 in §2.1 and 409 in the repo (`ALREADY_JOINED`).
- `COHOST_LIMIT_REACHED` is 403 with a limit of 1 in §3.1; the repo returns 409 with `MAX_COHOSTS = 3` (`dto.rs:44`, `error.rs:178`).
- The repo adds codes the doc does not list (`CREATOR_CANNOT_JOIN`, `HOST_CANNOT_LEAVE`, `ALREADY_LIVE_ELSEWHERE`, `CANNOT_ADD_SELF_AS_COHOST`, `IMAGE_TOO_LARGE`) and the doc adds codes the repo does not have (`RECORDING_NOT_AVAILABLE`, `BROADCAST_STILL_LIVE`, `DAILY_QUOTA_EXCEEDED`, `CANNOT_END`, `INVITATION_NOT_YOURS`, `INVITATION_NOT_FOUND`, `DUPLICATE_INVITATION`).
- Socket side: repo has no `DAILY_QUOTA_EXCEEDED`; it adds `UNSUPPORTED` and `SERVER_SHUTDOWN`. Both additions are correct engineering and should be documented, because `UNSUPPORTED` is deliberately **not** fatal (`errors.rs:120-130`) and a client that treats it as fatal will drop working sockets.
- `data` payloads are part of the contract and undocumented per variant. The only example the doc gives is `resetsInMins`, which the Flutter snippet indexes by name.

### 2.12 Mobile still carries the pre-v2 control vocabulary

`startedBroadcast` and `joinBroadcast` remain in the client enum and are sendable (`socket.dart:18-19`); the socket service decodes `startedBroadcast` into `onStartedBroadcast` (`broadcast_socket_service.dart:37,76`); and `LiveSessionManager.reconnectToSocket` re-enters over `joinRoom(...)` (`live_session_manager.dart:149-157`). The server does not accept any of those frames — it answers "not available yet" (`routes/ws.rs:40-43`). So reconnect currently works only because the live scope is recovered over HTTP elsewhere; the socket path is dead code pretending to be a contract.

### 2.13 Teardown on every end path: two of four

Confirmed: explicit host end tears down the room and swallows a teardown failure after writing the row (`service.rs:459-497`); the reaper ends through the same path with a row guard, so concurrent sweeps produce one ending (`reaper.rs:31-38`, `:237-260`). Absent: admin force-end (no admin surface) and quota exhaustion (no quota). So the requirement is satisfied for the paths that exist, and the doc's §4.8-style rule (which does not actually appear in the doc) needs to be stated where it can be tested.

### 2.14 Feature surface absent

| Doc section | Feature | Repo state |
| --- | --- | --- |
| §8.1 | Daily quota | nothing (Redis `QUOTA` TTL constant exists, unused) |
| §8.2 | Recording / egress / publish | columns only; `recording_url` always null, no webhook |
| §8.3 | Listen Later | `is_bookmarked` hardcoded `false` (`service.rs:857`, `mod.rs:64`) |
| §8.4 | Continue listening | `time_remaining_seconds` always `None` (`service.rs:859`); no position write path |
| §8.5 | Mini-player | no `/active-session` |
| §8.6 | Cohost invitations | direct add/remove only, no invitation table or accept/decline |
| §8.7 | Context menu | needs bookmarks + subscriptions only |

These are Tier 3/4 by the existing plan. The point of listing them is that the client's DTO already contains placeholders for all of them, so each one is a silent false statement rather than a missing screen.

---

## 3. Doc-side defect register

The doc is the reference, but it cannot be implemented as written in the following places. Each item is a proposed correction; the memo documents the rule to follow until the doc is revised.

| # | Doc location | Defect | Proposed correction |
| --- | --- | --- | --- |
| D-01 | §6.1 vs §6.2 vs §2.3 | §6.1 says to read reconnecting from `connection_state`, which §6.2's DTO does not have; §2.3 defines a separate 3-value `BroadcastConnectionState`. | `broadcast_state` is the only state on the DTO. `connection_state` exists only in the keepalive response and must be a projection of it. |
| D-02 | §6.3 | `viewer_is_in_room = viewer_role != None` is false for anyone who has left, because the participant row persists (it is what `total_participants` and continue-listening are built on). | `viewer_is_in_room = participant row exists AND left_at IS NULL`. Role stays historical. |
| D-03 | §6.2 vs §8.2 | `published_at` means "recording published" in §8.2 but the repo writes it at go-live, and §6.2 uses it to mean "has a recording". | Split into `went_live_at` and `recording_published_at`, or keep `published_at` for the recording and add `started_at`. |
| D-04 | §6.2 | `duration_seconds = end - start`, but `start_time` is null for instant broadcasts, so the field is null for most broadcasts. | `end - COALESCE(start_time, published_at/went_live_at)`, floored at 0. |
| D-05 | §6.3 | A broadcast whose `start_time` has passed and never went live falls through to `DRAFT`, so an overdue broadcast renders as a draft. | Add `MISSED` (or keep `SCHEDULED` and expose an `overdue` flag). |
| D-06 | §2.2 | `participant_pong_timeout 20 < ping_interval 25`, so the read timeout fires before the first ping; `max_missed_pings 2` implies ≥50s. | Require `max_missed × ping ≤ participant_timeout` and `participant_timeout > ping`; e.g. 25/60/50/2 or the repo's 25/90/60/2. |
| D-07 | §6.2 vs §8.1 | Two `QuotaDto` definitions; §8.1 adds `remaining_display`, §6.2 omits it. | One `QuotaDto` with `remaining_display`. |
| D-08 | §2.1, §8.4 | Rejoin is not described: does a rejoin reset `last_listen_position_seconds`, re-open the row, re-count the total, re-emit `participantJoined`? | Rejoin re-opens the row, preserves the position, bumps the all-time counter once per join, and emits the join event. |
| D-09 | §8.6 | Removing a cohost does not say what happens to their LiveKit token or their room membership. | Removal evicts from the room and the next privileged call fails; tokens are not revocable, so eviction is the enforcement. |
| D-10 | §3.1 vs §2.1 | `ALREADY_LIVE` and `JOIN_IN_PROGRESS` are listed under 400 in §3.1 and specified as 409 in §2.1. | One status per condition: both 409. |
| D-11 | §10.1 | `PUT /broadcasts/:id` described as a "partial update" — PUT is replace semantics. | `PATCH`. |
| D-12 | §10.1 | Creator joining their own broadcast is undefined; repo refuses with 403. | Document the refusal (a host re-enters via go-live/resume). |
| D-13 | §10.2 | Connection cap per user is unspecified; repo enforces 5 (`constants.rs:75`). | Document the cap and its error. |
| D-14 | §5, §6 | No list-item DTO; clients render cards from the detail DTO or invent a second shape. | Define `BroadcastListItem` (the repo has one) or state that cards use the full DTO. |
| D-15 | §10.2 vs §2.2 | Two liveness mechanisms: protocol ping/pong and a 60s client `heartbeat` event. | Protocol ping is the liveness signal; the app heartbeat may touch presence and nothing else. |
| D-16 | §2.3 | The grace watcher is an in-process `tokio::spawn`; a deploy during grace loses the timer and leaves a broadcast live forever. | Persist the deadline; end with a conditional write; make the watcher a sweep. |
| D-17 | §2.5 | No rule for Redis being unavailable to the limiter. Repo fails closed with 503. | State fail-closed, and state that a 429 carries `Retry-After`. |
| D-18 | §4.1 | `token_expires_at` cannot be known by the server unless it stores the token `iat` or re-mints. | Return `token_ttl_seconds` from now, or echo the caller's token expiry as sent by the client. |
| D-19 | §2.1, §8.2 | The doc never states that the LiveKit room must be deleted on **every** end path; it is implied by egress and by the counter lifecycle. | Add the rule explicitly, with the idempotency requirement. |
| D-20 | §2.1, §7.1 | End paths do not close participant rows, so `live_count`/`live_participants_count` can stay non-zero after an end, and a crashed client is counted until then. | On end, set `left_at` for every open row; on start, reconcile from room presence. |
| D-21 | §7.1 | `live_count` is created with no TTL and deleted only on end; a process death leaks it forever. | TTL = maximum broadcast duration + slack; recreate on go-live. |
| D-22 | §8.1 | `deduct_quota` charges the whole elapsed time to the *end* day's UTC key; a broadcast crossing midnight is billed to one day. `broadcast_start` has a 24h TTL, so a longer session charges 0. | Split elapsed at UTC midnight; take the start from the row (`went_live_at`), not a TTL'd key. Set the quota key's expiry to the next midnight, not `now + 86400`. |
| D-23 | §8.1 | Quota is checked only at go-live (`remaining <= 0`), so a creator with one second left can run a full session until the 60s watcher fires. | Cap the session at the remaining seconds or refuse to start below a minimum. |
| D-24 | §8.2 | `publish` reads a 7-day Redis key for the recording URL, so after 7 days a real recording cannot be published. `recording_url` is stored from a 24h presigned URL, so the stored value expires. | Presign on read from `recording_key`; use Redis only as an accelerator; never persist a presigned URL. Handle `EGRESS_FAILED`, not only `egress_ended`. |
| D-25 | §8.3 | Bookmarking a live broadcast that ends without a recording leaves an unlistenable entry; soft-deleting a broadcast leaves its bookmarks behind (`ON DELETE CASCADE` never fires). | Define "recording unavailable" presentation; filter `deleted_at IS NULL` in listen-later. |
| D-26 | §8.4 | `positionSeconds` is client-supplied and unvalidated; it is only written on the explicit leave event, so a crash (the most common failure) never updates it. | Clamp to `[0, duration]`, write it on the 60s heartbeat as well, and derive it server-side from `joined_at` as a floor. |
| D-27 | §8.5 | `active-session` returns one option while a user can be in two rooms (hosting one, listening to another); a crashed client's open row resurrects the mini-player. | Tie-break (host wins, then most recent join); reconcile rows against room presence. |
| D-28 | §8.6 | `UNIQUE (broadcast_id, invitee_id)` means a declined invitation can never be re-sent. Accepting an invitation to an ended broadcast is undefined. | Partial unique index on `status = 'pending'`; accept on a non-live broadcast inserts the cohost without a token, or 400 `NOT_LIVE`. |
| D-29 | §3.1 vs repo | Cohost limit is 1 in the doc's error message and 3 in the repo constant. | One number in both. |
| D-30 | §5.3 | `only_subscriptions=true` with no viewer silently falls through to "all broadcasts"; `count_with_same_filters` is referenced but never defined, so page and count can diverge. | Anonymous + subscriptions → empty or 401; define the count query as the same predicate set. |
| D-31 | §10.3 | `numberOfLiveBroadcasts` is broadcast to every connected socket on every go-live/end; no throttle or coalescing is specified. | Coalesce per interval; the repo already has a Redis coalescing helper. |
| D-32 | §10.1 vs §10.2 | The doc lists no HTTP end/leave, yet §10.2 makes them client socket events and the repo ships both as HTTP. | Choose the surface per action and list it once. |

---

## 4. Aligned today (do not churn)

- Session envelope `{ broadcast, token }`, RFC 3339 timestamps, no media URL in the response.
- Go-live is atomic and refuses a second live broadcast (409), including a cross-replica race (conditional UPDATE + unique partial index).
- Join is one HTTP round-trip; the server writes the roster.
- Explicit end writes the row first, tears down the room second, and does not fail the response for a teardown failure.
- The reaper's abandonment rule is sound: LiveKit presence, persisted clock, cohost-aware, single ending under concurrent sweeps.
- Heartbeat defaults satisfy the invariant a 2-miss tolerance needs, and `is_sane()` refuses values that do not.
- Offline buffer: 50 events / 300s / atomic drain / chronological replay / TTL set once rather than refreshed.
- Reconnect rate limit: 10 per 60s, fail-closed, 429 with `retry_after` before upgrade, counted after authentication so an anonymous flood cannot spend a user's budget.
- Error envelope and machine-readable `code`; socket `recoverable` is derived from the code rather than stored beside it.
- Enum values agree between backend and mobile (even though they disagree with the doc).
- `is_joined` uses `left_at IS NULL`, which is the semantics D-02 asks for.

---

## 5. Priority order

1. **Parse correctness (blocking).** Fix response casing (2.1), then align the §6.2 field names and enum casing (2.2, 2.3), or amend the doc. Nothing else is verifiable until the client reads the wire.
2. **Lifecycle signals.** Produce `RECONNECTING` (2.4), wire host grace or explicitly adopt the reaper as the grace implementation and publish `hostDisconnected`/`hostReconnected` from it, handle `broadcastError` on mobile (2.9), and add `/token` refresh (2.7).
3. **Surface reconciliation.** One endpoint per action, one event per concept, one error status per condition (2.9–2.11). Retire `startedBroadcast`/`joinBroadcast` or implement WS room join/leave (2.12).
4. **Resilience.** Keepalive (§4.1), Redis live count (§7.1) or amend the doc to the DB count, reconnect/buffer edge cases (D-15, D-16), every end path (2.13).
5. **Features.** Quota (D-22, D-23), then recording, bookmarks, continue-listening, mini-player, cohost invitations (D-24–D-28).

---

## 6. Files to hand-check before each tier moves

Backend: `modules/broadcast/{dto,model,service,handlers,error,reaper,media}.rs`, `modules/broadcast/repository/{pg,memory}.rs`, `routes/broadcasts.rs`, `routes/ws.rs`, `infrastructure/ws/{model,dto,errors,mod}.rs`, `infrastructure/redis/keys.rs`, `infrastructure/constants.rs`, `config.rs`.

Mobile: `features/broadcast/model/{dtos,dtos.g,entities,broadcast_query}.dart`, `features/broadcast/services/{broadcast_http_service,broadcast_socket_service}.dart`, `features/broadcast/manager/{live_session_manager,stream_manager,broadcast_editor_manager,live_scope_manager}.dart`, `_shared/model/socket.dart`, `_core/config/{config,env}.dart`, `_shared/services/{http_client,livekit_client,room_transport}.dart`, `locator.dart`.

Yardsticks: `memo/broadcast-contract-snapshot.md`, `memo/broadcast-contract-workspace/broadcast_contract.ts`.

---

## 7. One-line summary

The two sides agree on the session envelope and on the mechanics that have been built carefully (atomic go-live, single-round-trip join, presence-driven reaper, heartbeat invariant, buffer, reconnect limiter), but the client cannot read a single multi-word field of the response, `RECONNECTING` is never emitted, token refresh and keepalive do not exist, and the doc has thirty-two places where it is silent, ambiguous, or self-contradictory — so the contract is not locked, and the first fix is casing, not features.
