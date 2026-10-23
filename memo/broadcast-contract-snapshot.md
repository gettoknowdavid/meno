# Broadcast wire-contract and session-lifecycle snapshot

> Working index into `docs/meno-broadcast-v2-final.md` for the parts backend (`apps/api`) and mobile (`apps/mobile`) exchange during a live session.
>
> **The doc is the source of truth.** This memo restates what the doc requires, names the repo field/event that corresponds to it, and marks every place the two disagree. Where the doc contradicts itself, the contradiction is recorded as a **doc gap** (`D-n`) and argued in [broadcast-gap-analysis.md §3](broadcast-gap-analysis.md#3-doc-side-defect-register). The doc is not edited by this memo; a `D-n` item is a request for a doc revision plus a rule the repo must follow until then.
>
> All `§` references are sections of `docs/meno-broadcast-v2-final.md`, whose real sections are 1–10 with subsections 1.1–1.2, 2.1–2.5, 3.1–3.2, 4.1, 5.1–5.3, 6.1–6.3, 7.1, 8.1–8.7, 10.1–10.3. There is no §4.2, §4.8 or §7.6 in that document; older notes in this memo and in code comments that cite them are citing `docs/refactor-plan-document.md` (§4.2 error taxonomy, §4.4 rate limiting, §4.8 observability, §9.1 logging), not the broadcast spec.
>
> **Source map for the rest of the doc set.** Only `docs/meno-broadcast-v2-final.md` is the contract. The others are implementation inputs and are superseded by it wherever they differ:
> - `docs/refactor-plan-document.md` — the numbering code comments cite: §4.2 error taxonomy, §4.4 rate limiting, §4.8 observability, §9.1 error rules, §7.x defect register. It says nothing about the broadcast wire shape.
> - `docs/meno-broadcast-module-guide.md` — an earlier implementation sketch: §6.7 host grace (a fixed 60s, in-process timer), §8.4 `/token` refresh (the only design for it, and it assumes a 6-hour token TTL), §4.x DTO sketches.
> - `docs/meno-rust-migration-guide.md` — Epic/Ticket migration plan; no broadcast contract.
>
> Verified against `refactor/monorepo` on 2026-10-23. Evidence is `path:line`.

**Status vocabulary** (used in every table below):

| Status | Meaning |
| --- | --- |
| **match** | Doc shape and repo shape agree; no action. |
| **drift** | Both exist but differ in name, casing, or value set; one side must change. |
| **missing** | Doc requires it; repo has no implementation. |
| **extra** | Repo has it; doc does not describe it. |
| **doc gap** | Doc is silent, ambiguous, or self-contradictory; a rule is proposed and carried to the gap analysis. |

---

## 1. The session response (§1.2, §10.1)

### 1.1 Shape

Doc (§1.2, launch path): one LiveKit Cloud project, one URL from server ENV, no per-region logic.

- `BroadcastSessionDto { broadcast: BroadcastDto, livekit_token: String }`
- **No `livekit_url`** in the ENV-only contract.
- Future self-hosted path is additive: `livekit_url: Option<String>`, with the client rule
  `final url = session.livekitUrl ?? AppConfig.livekitHost`.
- ENV: `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`, `LIVEKIT_HOST`.

Repo today:

- `BroadcastSessionResponse { broadcast: BroadcastResponse, token: String }` — `apps/api/src/modules/broadcast/dto.rs:502`.
- Mobile `BroadcastSessionDto { broadcast, token }` — `apps/mobile/lib/features/broadcast/model/dtos.dart:78`.
- No media URL in the response; the client builds the room URL from config (`apps/mobile/lib/locator.dart`).
- Backend accepts `LIVEKIT_URL` and the legacy `LIVEKIT_HOST`; mobile reads `MENO_LIVEKIT_URL` → `Config.liveKitUrl`.
- Token TTL: 15 minutes (`apps/api/src/config.rs:195` — `TOKEN_TTL_SECS`).

**Verdict:** shape **match**; field name `token` vs doc `livekit_token` **drift**; env name **drift** (`LIVEKIT_HOST` / `LIVEKIT_URL` / `MENO_LIVEKIT_URL` — pick one; see §8.1).

### 1.2 Endpoints that mint a session

| Doc (§10.1) | Repo (`apps/api/src/routes/broadcasts.rs`) | Status |
| --- | --- | --- |
| `PUT /broadcasts/:id/go-live` | `POST /broadcasts/{id}/live` (`handlers.rs:197`, route at `routes/broadcasts.rs:107`) | **drift** (method + path) |
| `POST /broadcasts/:id/join` | `POST /broadcasts/{id}/join` (`handlers.rs:250`) | **match** |
| `POST /broadcasts/:id/token` (refresh) | — | **missing** |
| `POST /broadcasts/:id/keepalive` | — | **missing** |
| — (doc: host ends over WS `endBroadcast`, §10.2) | `POST /broadcasts/{id}/end` | **extra** (HTTP end) |
| — (doc: participant leaves over WS `leaveBroadcast`, §10.2) | `POST /broadcasts/{id}/leave` | **extra** (HTTP leave) |
| — | `POST /broadcasts/{id}/resume` (`handlers.rs:214`) | **extra** — not in the doc at all; the doc's re-entry path is `/token` |
| `PUT /broadcasts/:id` ("partial update", §10.1) | `PATCH` (`routes/broadcasts.rs:105`) | **drift** — the doc's verb is wrong for partial semantics; see D-11 |

`/resume` is a real repo need: `go_live` refuses a live broadcast and `join` refuses the creator, so neither can be the "come back" path (`apps/api/src/modules/broadcast/service.rs:498-514`). If it stays, the doc must gain it; otherwise `/token` plus a re-join rule must replace it.

### 1.3 Session edge cases

| # | Scenario | Doc rule | Repo today | Action |
| --- | --- | --- | --- | --- |
| S1 | Host taps Go live twice / two replicas race | §2.1 `AlreadyLive`; §3.1 409 | Conditional `UPDATE … WHERE status='inactive'` plus a one-live-per-creator unique index → `already_live` 409 (`repository/pg.rs:280-303`, `error.rs:87`) | **match**; keep. Doc's code is 409, so map `AlreadyLive` → 409 (§3.1 groups it under 400 — D-10). |
| S2 | Same user taps Join twice | §2.1 says "Redis per-user lock → 409 Conflict"; §3.1 lists `JoinInProgress` under 400 | `JoinOutcome::AlreadyPresent` → 409 `already_joined` (`service.rs:596`, `error.rs:112`) | **match** on status; align the doc's status for `JoinInProgress`. |
| S3 | User rejoins after leaving (`left_at` set) | Not stated | Row is reopened and a fresh session returned (`JoinOutcome::Rejoined`) — no 409 | **doc gap** D-08: rejoin must be idempotent and must not reset `last_listen_position_seconds`. |
| S4 | Broadcast ended between list and Join | §2.1: deleted → 404 `NotFound`; ended → `NotLive` | `not_live` is 409 `CONFLICT` (`error.rs:78-84`) | **drift**: doc says `NotLive` is 400; repo returns 409. Pick one mapping and put it in §3.1. |
| S5 | Creator calls Join on their own broadcast | Not stated | 403 `creator_cannot_join` (`error.rs:133`) | **doc gap** D-12; keep repo rule. |
| S6 | Resume by someone not on the roster | Not stated (`/resume` absent) | `not_joined` → `NOT_PARTICIPANT` (400) | **doc gap** if `/resume` is kept. |
| S7 | Token expires mid-session | §2.1: FE polls expiry, calls the refresh endpoint; §3.2 `TOKEN_EXPIRED` recoverable | No refresh endpoint. `WsErrorCode::TokenExpired`'s own doc points at `/broadcasts/:id/token` (`apps/api/src/infrastructure/ws/errors.rs:92-95`) — a dangling reference. Mobile recovers with `/resume` only | **missing**; Tier 1 blocker (§6.2). |
| S8 | Host's app is backgrounded for longer than the token TTL | §4 expects continuation after the app returns | Session stored before backgrounding carries an expired token; `resume` re-mints | Works via `/resume`, but that is an **extra** path, not the doc's `/token`. |
| S9 | Broadcast is in host grace when the session is fetched | §6.2 `broadcast_state = RECONNECTING` | Repo never returns `reconnecting` (§3.1 below) | **missing**. |
| S10 | Soft-deleted broadcast fetched or joined | §5.3 filters `deleted_at IS NULL` | Reads filter `deleted_at IS NULL`; join on a deleted row is 404 | **match**. |
| S11 | Six devices open one account | Not stated | Connection cap 5 per user → 429 (`constants.rs:75`, `routes/ws.rs:206`) | **doc gap** D-13 (document the cap and its error). |
| S12 | Client is on the future self-hosted path | `livekit_url` optional; `?? config` fallback | Field absent; config-only | **match** today; when the field is added the client must keep the `??` fallback rather than assume presence. |

---

## 2. The broadcast DTO the session carries (§6.1–§6.3)

### 2.1 Questions the DTO must answer (§6.1)

| Client question | Doc field | Repo field | Status |
| --- | --- | --- | --- |
| Is it live? | `broadcast_state == LIVE` | `state == live` | **drift** (name + casing) |
| Is it live with the host reconnecting? | §6.1 says `connection_state == RECONNECTING`; §6.2 has no `connection_state` | `state` never leaves `live` | **doc gap** D-01 + **missing** |
| Am I the host? | `viewer_role == HOST` | `participant_role == host` | **drift** |
| Am I a cohost? | `viewer_role == COHOST` | `participant_role == cohost` | **drift** |
| Am I in the room now? | `viewer_is_in_room == true` | `is_joined` (from `left_at IS NULL`) | **drift** name; repo semantics are the correct ones (D-02) |
| Am I subscribed to the creator? | `is_subscribed_to_creator` | — | **missing** |
| How long was it? | `duration_seconds` | `duration_seconds` | **match** type; computation differs (D-04) |
| Live count now? | `live_participants_count` (Redis) | `liveParticipantsCount` (DB `COUNT(*)` of `left_at IS NULL`) | **drift** — source only; name matches |
| Total ever? | `total_participants` | `totalParticipants` | **match** |
| Recorded and published? | `published_at != null && recording_url != null` | `published_at` means "went live" in the repo; `recording_url` always null | **drift** semantics + **missing** feature |
| Time left if I joined halfway? | `time_remaining_seconds` | field exists, always `None` (`service.rs:859`) | **missing** feature |
| Creator quota? | `creator_quota` (host-only, go-live only) | — | **missing** |

### 2.2 Field-by-field mapping (§6.2)

Doc JSON is camelCase because every doc struct carries `#[serde(rename_all = "camelCase")]`. Repo serialization follows the same rule (`dto.rs:299`), **but the mobile DTO reads snake_case keys** (`apps/mobile/lib/features/broadcast/model/dtos.dart:19-41`, generated `dtos.g.dart:14`). Every multi-word field below therefore arrives as absent on mobile. This is the single most consequential finding in this memo — see [gap analysis §2.1](broadcast-gap-analysis.md#21-response-casing-blocking).

| Doc §6.2 field | Doc type | Repo wire field | Status |
| --- | --- | --- | --- |
| `id` | `Uuid` | `id` | match |
| `title` | `String` | `title` | match |
| `description` | `String` | `Option<String>` | **drift** (doc non-null) |
| `time_zone` | `String` | `Option<String>` | **drift** |
| `image_url` | `Option` | `imageUrl` | match |
| `image_id` | `Option` | `imageId` | match |
| `creator_id` | `Uuid` | — (only nested `creator.id`) | **missing** |
| `created_at` | `Option<DateTime>` | `createdAt` non-null RFC 3339 | **drift** (doc allows null) |
| `start_time` | `Option<DateTime>` (null for instant) | `startTime` | match |
| `end_time` | `Option<DateTime>` | `endTime` | match |
| `published_at` | recording publish time (§8.2) | set at **go-live** (`repository/pg.rs:286`) | **drift** — one column, two meanings (D-03) |
| `duration_seconds` | `end - start` | `end - published_at` (`service.rs:487-489`) | **drift** + doc gap D-04 |
| `broadcast_state` | `BroadcastState`, 5 variants, SCREAMING_SNAKE | `state`, 5 variants, lowercase | **drift** (name + casing) |
| `viewer_role` | `ViewerRole { HOST, COHOST, PARTICIPANT, NONE }` | `participantRole { host, cohost, participant, none }` | **drift** (name + casing only) |
| `viewer_is_in_room` | `bool` | `isJoined` | **drift** (name) |
| `is_subscribed_to_creator` | `bool` | — | **missing** |
| `live_participants_count` | `i64`, Redis | `liveParticipantsCount`, DB | **drift** (source only) |
| `total_participants` | `i64`, DB | `totalParticipants` | **match** |
| `recording_enabled` | `bool` | `recordingEnabled` | match |
| `recording_url` | `Option<String>` presigned when published | `recordingUrl` (always null today) | **missing** feature |
| `end_reason` | `Option<EndReason>`, 4 variants | `endReason` non-optional, **5** variants incl. `none`, lowercase | **drift** (nullability, extra variant, casing) |
| `time_remaining_seconds` | `Option<i64>`, continue-listening only | present, always `None` | **missing** feature |
| `last_listened_at` | `Option<DateTime>` | — | **missing** |
| `creator_quota` | `Option<QuotaDto>` | — | **missing** |
| `creator` | `Option<UserSummaryDto>` | `creator` non-optional | **drift** (optionality) |
| `cohosts` | `Option<Vec<CohostDto>>` with `is_cohost` | `cohosts: Vec<UserSummary>` | **drift** (no `isCohost`, no badge) |
| `is_bookmarked` (§8.3, absent from §6.2) | `bool` | `isBookmarked`, hardcoded `false` (`service.rs:857`, `mod.rs:64`) | **doc gap** (missing from §6.2) + **missing** feature |
| — | — | `status` (`inactive`/`active`/`ended`) | **extra** (repo exposes the DB column alongside `state`) |

Repo extras outside §6.2 that a client may legitimately want: `status` and the list-item shape (`BroadcastListItem`: `creatorId`, `creatorName`, `creatorAvatarId`, `creatorAvatarUrl`, `totalParticipants`, `state`). §5 does not define a list-item DTO at all (D-14).

### 2.3 Enums

| Enum | Doc §6.2 | Repo (`apps/api/src/modules/broadcast/model.rs`) | Mobile (`entities.dart`) |
| --- | --- | --- | --- |
| `BroadcastState` | `LIVE`, `RECONNECTING`, `ENDED`, `SCHEDULED`, `DRAFT` | `live`, `reconnecting`, `ended`, `scheduled`, `draft` (`:313-334`) | same lowercase (`:315-320`) |
| `ViewerRole` / `ParticipantRole` | `HOST`, `COHOST`, `PARTICIPANT`, `NONE` | `host`, `cohost`, `participant`, `none` (`:183-192`) | same (`:638-642`) |
| `EndReason` | `NORMAL`, `HOST_DISCONNECTED`, `ADMIN_FORCED`, `QUOTA_EXCEEDED`; the field is `Option` | `normal`, `host_disconnected`, `admin_forced`, `quota_exceeded`, **`none`** (`:257-268`) | same including `none` (`:469-474`) |
| `BroadcastConnectionState` (§2.3) | `LIVE`, `RECONNECTING`, `ENDED` — used by the keepalive response | — | — |

- The doc has **two** state enums for overlapping facts. Only `broadcast_state` is authoritative on the DTO (D-01).
- Repo lowercase matches mobile but not the doc. Either the doc's `SCREAMING_SNAKE_CASE` wins (rename both sides) or the doc adopts lowercase (rename the doc). Recommend the doc's casing wins: the error `code`s in the same document are already SCREAMING_SNAKE, so one casing for machine-read values stays coherent.
- `EndReason::None` is a repo-only fifth variant. The doc models "not ended" as `null`. Sending `"none"` is not the same statement as `null`; a client that renders `endReason` unconditionally will say "Broadcast ended" for a live broadcast (`entities.dart:485`). Keep `null`.

### 2.4 Derived fields (§6.3)

| Field | Doc rule | Repo rule | Verdict |
| --- | --- | --- | --- |
| `broadcast_state` | Active + grace key → RECONNECTING; Active → LIVE; Inactive + end_time → ENDED; Inactive + future start → SCHEDULED; else DRAFT | Pure row function (`model.rs:94-104`); `Reconnecting` is defined but **never constructed** anywhere in `apps/api/src` | **missing** overlay. The repo's own doc comment says the service owns it; no service code does. |
| `viewer_role` | creator → HOST; cohost row → COHOST; participant row → PARTICIPANT; else NONE | creator → host; cohost list → cohost; participant row → participant; else none (`service.rs:846-850`) | match in intent; casing only |
| `viewer_is_in_room` | doc derives it as `viewer_role != NONE` | `left_at IS NULL` (`service.rs:852`) | **doc gap** D-02: the doc's derivation is wrong, because a participant row outlives the visit (it is needed for `total_participants` and continue-listening). |
| `live_participants_count` | Redis `live_count:{bid}`, batch-fetched for lists | `COUNT(*) … left_at IS NULL` per row (`repository/pg.rs:400-409`), i.e. N+1 in list contexts | **drift**; the DB count also cannot see a client that died without sending leave. |
| `total_participants` | DB all-time joins | `total_participants` column, bumped on rejoin | **match** |
| `duration_seconds` | `end - start` | `end - published_at` | **doc gap** D-04: `start_time` is null for instant broadcasts, so the doc's formula yields nothing for exactly the broadcasts that ran. |
| `time_remaining_seconds` | `duration - last_listen_position`, only in continue-listening | always `None` | **missing** |
| `creator_quota` | host-only, go-live response only | — | **missing**; §8.1's `QuotaDto` also has `remaining_display`, which §6.2's copy lacks (D-07). |

State-derivation hole (both sides): a broadcast with `start_time` in the past that never went live falls to `DRAFT`, because the SCHEDULED arm requires `start_time > now()`. A user sees "draft" for something that was supposed to start yesterday. Proposed rule in D-05: `SCHEDULED` for a future start, `MISSED`/`ENDED` when the start passed without going live, or keep `SCHEDULED` with a client-side "overdue" affordance. Pick one.

### 2.5 DTO edge cases

| # | Scenario | Rule to enforce |
| --- | --- | --- |
| E1 | Unauthenticated viewer | `viewer_role = NONE`, `is_joined = false`, `is_bookmarked = false`, `live_participants_count` still real. Doc's §6.3 matches; repo matches. |
| E2 | Creator is also a cohost row | HOST wins (creator check first). Repo does this. |
| E3 | Viewer left the room an hour ago | `viewer_is_in_room = false` **and** `viewer_role` may still be `PARTICIPANT` (historical). A client must not derive "in room" from the role (D-02). |
| E4 | Cohost was removed mid-session | Role falls back to PARTICIPANT/NONE on the next read; the LiveKit token already minted stays valid until TTL; the room must evict them (D-09). |
| E5 | Broadcast ended, viewer never joined | `end_reason` set, `duration_seconds` set, `viewer_role = NONE`, `time_remaining_seconds = null`. |
| E6 | `time_remaining_seconds` when position > duration | clamp to 0 (doc's `.max(0)`), and clamp the stored position at write time. |
| E7 | Quota on a non-host response | must be `null`; never serialise `creator_quota` for non-hosts (leaks the creator's usage). |
| E8 | `published_at` for a broadcast that is live and unrecorded | must be `null` under the doc's meaning. Repo currently puts go-live time here, which is why `duration_seconds` has a value at all. Renaming/splitting the column is D-03. |
| E9 | `live_participants_count` on an ended broadcast | `0` (doc: "0 when not live"). The DB count of `left_at IS NULL` is 0 only if every client left cleanly — the reaper does not close participant rows. Define the end path to close them. |

---

## 3. The live session lifecycle (§2.1–§2.5, §4.1, §10.2)

### 3.1 Heartbeat (§2.2)

| Parameter | Doc | Repo (`apps/api/src/infrastructure/ws/model.rs:264-292`) |
| --- | --- | --- |
| `ping_interval_secs` | 25 | 25 |
| `host_pong_timeout` | 60 | **90** |
| `participant_pong_timeout` | 20 | **60** (`listener_pong_timeout_secs` internally) |
| `max_missed_pings` | 2 | 2 |

- **Doc gap** D-06: the doc's own numbers cannot work together. The read loop waits `pong_timeout` for *any* inbound frame and treats a timeout as a disconnect; with `participant_pong_timeout = 20 < ping_interval = 25`, a quiet participant is dropped before the first ping can be answered. Even `timeout = 25` would drop it between pings, and `max_missed_pings = 2` implies the deadline must cover at least two ping intervals (`2 × 25 = 50`). The repo's `is_sane()` encodes that invariant (`model.rs:295-307`) and its defaults satisfy it (60 ≥ 50).
- The repo's **host 90s** is a deliberate extension of the doc's 60s for backgrounded iOS hosts. If the doc is authoritative, this is **drift**; if the repo's value is the intended one, the doc needs the same rule.
- Liveness source of truth: the **protocol ping/pong**. §10.2 also has a client `heartbeat { broadcastId }` event every 60s; it must be defined as advisory (touch presence at most) or removed, because two liveness mechanisms that disagree produce a host that is "alive" to one and dead to the other (D-15).
- A pong or **any** inbound frame resets the missed counter (repo and doc agree).

### 3.2 Host disconnect and tiered grace (§2.3)

Doc: per-disconnect counter in Redis, tiers 120/90/60/30, `host_grace:{bid}` with TTL, `host_grace_started:{bid}` for the client countdown, `hostDisconnected` with `gracePeriodSecs` + `disconnectCount`, a spawned watcher that ends the broadcast if the key survives the sleep.

Repo:

- `GracePeriodConfig` implements the same four tiers (`apps/api/src/infrastructure/ws/model.rs:311-345`).
- Redis keys exist: `LIVE_COUNT`, `HOST_GRACE`, `GRACE_STARTED`, `DISCONNECT_COUNT`, `RECONNECT_RATE` (`apps/api/src/infrastructure/redis/keys.rs:85-99`).
- **Nothing increments or reads them** (except `RECONNECT_RATE`, which `routes/ws.rs` uses). `Reconnecting` is never constructed; `host_disconnected(…)` is only called from tests; `apps/api/src/routes/ws.rs:40-43` states plainly that "room join/leave and the host grace period" are not wired.
- What actually ends an abandoned broadcast is `BroadcastReaper`: a sweep over live rows, presence from the **LiveKit room** (not the roster), a DB column `abandoned_since`, and a flat **10-minute** grace (`apps/api/src/modules/broadcast/reaper.rs:59`). A connected **cohost counts as healthy**; a room with nobody who can publish does not.
- Teardown: the reaper calls the same end path and, being row-guarded, ends at most once even under concurrent sweeps (`reaper.rs:31-38`).

Doc gaps / decisions this forces:

| # | Issue | Proposed rule |
| --- | --- | --- |
| G1 | **Process restart loses the grace timer.** The doc's watcher is a `tokio::spawn`; a deploy or crash during the grace window leaves a live broadcast with no timer. | Persist the deadline (DB column or Redis + a sweep). The repo's `abandoned_since` is the pattern; the doc should adopt it (D-16). |
| G2 | **Check-then-end race.** `exists()` followed by `end_broadcast` is not atomic with `DEL` on reconnect: a host who returns between the two is still ended. | End with a conditional write (`… WHERE status='active' AND deadline < now()`), and have reconnect clear the deadline *before* announcing `hostReconnected`. |
| G3 | **Tier reset mid-session.** `host_disconnect_count` expires after 1h, so a long broadcast returns to the 120s tier; the doc's `expire` is re-applied on each disconnect, which slides the window. | Store the count on the broadcast row or use a key whose TTL is the broadcast's maximum life. |
| G4 | **Grace applies to the host only; a cohost in the room is ignored** by the doc, but a cohost *is* producing audio. | State the rule explicitly. Repo rule (cohost keeps it alive) is the safer one. |
| G5 | **Host connected but not publishing.** §2.1 says a LiveKit media drop with the WS alive is "log only", so a room with no publishing host can stay "live" indefinitely. | Health = "host or cohost is *publishing*", not merely present. |
| G6 | **`hostReconnected` after end.** `find_active_hosted_by` returns nothing once the broadcast ended, so a host returning after grace expiry gets no event and no error — the client shows a room that no longer exists. | Emit `broadcastError` with `BROADCAST_FORCIBLY_ENDED` or `ROOM_NOT_FOUND` on the resume attempt, or let `/resume` 409/404. |
| G7 | `host_grace_started` TTL is `grace + 10s`; a client that asks later cannot compute the countdown. | Make the countdown derivable from `hostDisconnected.gracePeriodSecs` received on the wire (repo already ships `disconnectCount`), not from a second key. |
| G8 | Field spelling: doc `gracePeriodSecs`; repo `gracePeriodInSecs` (`apps/api/src/infrastructure/ws/dto.rs:110-117`). | One spelling. Recommend the doc's `gracePeriodSecs`. |

### 3.3 Message buffer (§2.4)

| Property | Doc | Repo | Verdict |
| --- | --- | --- | --- |
| Size | last 50 events per user | 50 (`constants.rs:90`) | match |
| TTL | 300s | 300s (`constants.rs:97`) | match |
| Write | LPUSH + LTRIM + EXPIRE when offline | LPUSH + LTRIM; TTL applied on key creation only (`ws/mod.rs:471-500`) | **better than doc** — re-applying EXPIRE per push would keep a stale buffer alive forever |
| Drain | atomic LRANGE+DEL | same Lua script (`ws/mod.rs:508-520`) | match |
| Replay order | chronological (reverse of LPUSH) | same | match |

Edge cases:

- **Register/buffer race.** `is_online` is a local-registry answer; a user registering between the check and the LPUSH gets the event buffered *and* never replayed (their drain already ran) or replayed twice. Either take a per-user lock around register + drain, or accept at-most-once and have the client refetch on `ready`.
- **Replay is stale by construction.** A 4-minute-old `endedBroadcast` may describe a broadcast the user re-joined. Clients must treat replayed events as informational and refetch authoritative state; counts are absolute values, so a stale count is corrected by the next one.
- **Buffer is per user, not per room.** Fifty events is a global budget across every broadcast the user is in. Documented limit; a chat gap is expected and reconciled by refetch.
- **Ordering across replicas.** LPUSH order is the order the server saw the events, which is not necessarily causal order across rooms. No client rule should depend on cross-room ordering.

### 3.4 Reconnect rate limiting (§2.5)

| Property | Doc | Repo (`apps/api/src/routes/ws.rs:75-170`) |
| --- | --- | --- |
| Window | 60s | `ttl::RECONNECT_RATE = 60s` |
| Budget | > 10 → refuse | `MAX_RECONNECTS_PER_WINDOW = 10`, `> 10` refused |
| Response | 429 before upgrade | 429 `RateLimited { retry_after_secs }` before `on_upgrade` |
| Redis down | not stated | **fail closed** → 503 (`Upstream`) |

Edge cases:

- The counter counts *every* authenticated connect, not only reconnects. Three devices plus app restarts can exhaust it without a network problem; the doc's wording ("reconnecting too fast") does not match the implementation. Say "connections per user per minute".
- A browser WebSocket cannot reliably read a 429 from a failed upgrade. Either expose a WS-level `broadcastError` after upgrade, or document that the client must treat a failed upgrade as retry-after-`retry_after_secs`. Include `Retry-After` in the HTTP response.
- Fail-closed is a deliberate availability trade-off; the doc should name it (D-17), because it turns a Redis outage into "no one can connect" rather than "everyone can storm".

### 3.5 Keepalive and token refresh (§4.1)

Doc `POST /broadcasts/:id/keepalive`:

- Requires a live broadcast and a participant row; otherwise `NotLive` / `NotParticipant`.
- Writes `participant_last_seen:{id}:{uid}` with a 5-minute TTL.
- For the host: if the grace key exists, treat the call as **host reconnected** (clear grace, announce).
- Returns `{ broadcast_id, connection_state, live_participants_count, token_expires_at }`.

Repo: **missing** (`apps/api/src/routes/broadcasts.rs` has no keepalive route).

Edge cases this endpoint must define:

- A keepalive during grace is a *reconnect without a socket*. It must run the same grace-clear + `hostReconnected` announcement as the socket path, or the room is told two different things.
- A keepalive after grace expired must not resurrect the broadcast: answer `NotLive`, which the client renders as "this broadcast ended".
- `token_expires_at` must be the expiry of the token the caller currently holds. The server does not store tokens; it can only answer "minted at + TTL" if the client sends its token `iat`, or answer the TTL and let the client do arithmetic. The doc does not say which (D-18).
- If the keepalive itself is the client's only liveness signal (WS dead), it must also touch presence, or the reaper/reconnect logic will consider the participant gone.
- `connection_state` here has only three values while `broadcast_state` has five. Since keepalive only answers for a live broadcast, `ENDED` is unreachable and `SCHEDULED`/`DRAFT` are refused — the enum is total only because the endpoint narrows first (D-01).

Token refresh (`POST /broadcasts/:id/token`):

- Must re-infer the role from the DB, not from the request (a participant must not be able to ask for a host token).
- Must refuse when the broadcast is not live (`NotLive`) and when the caller has no row (`NotParticipant`).
- Must refuse when the caller was removed as a cohost after their old token was minted — role inference from the current row handles this.
- Must be rate limited; otherwise it is a free token-minting endpoint.
- `TOKEN_EXPIRED` on the socket must be followed by this call, then `room.refreshToken(...)` — not by a full re-join, which would re-announce the participant.

The only design for this endpoint is the module guide's §8.4: look the broadcast up, require it live, find the participant row, re-mint with that row's role, and return the session. Take its *shape*, not its numbers — it assumes a 6-hour token TTL, while the repo mints 15 minutes and §2.1 treats expiry as a real mid-session event.

### 3.6 End paths and teardown

Doc (§2.1, §2.3, §8.1, §10.2) names these ways a broadcast ends: host `endBroadcast` over WS, host grace expiry, admin force-end, quota exhaustion. It never says the LiveKit room must be deleted on all of them; that requirement is implied by §8.2's egress and by §7's counter lifecycle. This is a **doc gap** (D-19) — the requirement that matters most is the least stated.

| End path | Repo | Room deleted | Counters/rows closed |
| --- | --- | --- | --- |
| Host ends (`POST /end`) | `service.end` → `mark_ended` → `media.end_room`, failure logged not propagated (`service.rs:459-497`) | yes, best effort | `live_participants_count` is a live query, so nothing to reset; participant rows stay open (D-20) |
| Host grace/reaper | `BroadcastReaper::end` → same end + announcement | yes | same |
| Admin force-end | no admin surface | — | — |
| Quota exhaustion | no quota | — | — |
| Process shutdown | `close_all_connections` sends a recoverable `SERVER_SHUTDOWN`-style error (`ws/mod.rs:548`, `dto.rs:268-281`) | rooms survive a process restart | — |

Edge cases: two end paths firing at once must produce exactly one ending and one announcement (repo: row-guarded `mark_ended`, `reaper.rs:31-38`); ending an already-ended broadcast must be idempotent 200 or 409, not 500; a failed `end_room` must not leave the row live (repo already does this correctly — row first, media second); and the client's own disconnect must not be the only cleanup path.

### 3.7 Lifecycle summary

**Wired today:** heartbeat with sane values, presence, offline buffer + atomic replay, reconnect rate limit, connection cap, chat frames, explicit HTTP end/go-live/join/leave/resume, reaper-based abandonment, room teardown on the two end paths that exist.

**Not wired today:** WS room join/leave (so `startedBroadcast`/`joinBroadcast` are dead vocabulary), host grace + `Reconnecting`, keepalive, token refresh, quota, admin end, recording, bookmarks, continue-listening, mini-player, cohost invitations.

---

## 4. WebSocket catalogue (§10.2, §10.3)

### 4.1 Client → server (§10.2)

| Doc event | Payload | Repo `WsEvent` | Mobile `SocketEvent` | Status |
| --- | --- | --- | --- | --- |
| `endBroadcast` | `{ broadcastId }` | — (HTTP `POST /end`) | `endBroadcast` | **drift**; mobile sends an event the server does not accept |
| `leaveBroadcast` | `{ broadcastId, positionSeconds }` | — (HTTP `POST /leave`, no position) | `leaveBroadcast` | **drift**; position is lost, so continue-listening cannot work |
| `heartbeat` | `{ broadcastId }` | `heartbeat` (app-level) | — | **drift** (mobile never sends it; protocol ping is the real liveness signal) |
| — | — | `joinBroadcast`, `startedBroadcast` | both present | **extra**; server answers "not available yet" (`routes/ws.rs:40-43`) |

### 4.2 Server → client (§10.3)

| Doc event | Repo `WsEvent` | Mobile `SocketEvent` | Status |
| --- | --- | --- | --- |
| `newBroadcast` | `newBroadcast` | `newBroadcast` | match |
| `endedBroadcast` | `endedBroadcast` | `endedBroadcast` | match name; payload spelling differs (`broadcast_id`/`broadcastId` decoded defensively) |
| `hostDisconnected` | `hostDisconnected` | `hostDisconnected` | match name; `gracePeriodSecs` vs `gracePeriodInSecs` |
| `hostReconnected` | `hostReconnected` | `hostReconnected` | match |
| `participantJoined` | `participantJoined` | — | **drift** (payload only): doc `broadcastId` + `UserSummaryDto`, repo adds `count` |
| `participantLeft` | `participantLeft` | `broadcastListenerLeft` | **drift** (client only) |
| — | `participantKicked` | — | **extra** |
| `numberOfLiveParticipants` | `numberOfLiveParticipants` | — | **match** (doc ↔ server); no client handler |
| `cohostInvitation` | `cohostInvitation` | — | match name; doc payload `{ invitationId, inviterName, broadcastTitle }` vs repo `{ broadcastId, token }` |
| `cohostAccepted` / `cohostDeclined` | same | — | match names |
| `newCohost` | `newCohost` | — | match |
| `removedCohost` | `removedCohost` | — | match name; doc payload `{ broadcastId }`, repo `UserSummary` |
| — | `cohostDemotion` | — | **extra** |
| `recordingReady` | `recordingReady` | — | match |
| — | `recordingPublished` | — | **extra** |
| `broadcastError` | `broadcastError` | — | **missing on mobile** |
| `notification` | `notification` | `notification` | match |
| `numberOfLiveBroadcasts` | — | — | **missing** |
| — | `scheduledBroadcast`, `broadcastDeleted`, `homeInvalidated`, chat events, `newReaction` | chat events | **extra** (undocumented in §10.3) |

Mobile's enum (`apps/mobile/lib/_shared/model/socket.dart:14-37`) is a strict subset plus two dead sent-events. It has no `broadcastError` handler at all, so a `TOKEN_EXPIRED` or `KICKED_FROM_ROOM` frame is silently dropped today. That is a Tier 1 gap (§6.2).

---

## 5. Errors (§3.1, §3.2)

### 5.1 HTTP

Doc shape: `{ statusCode, code, message, data? }`, `code` = SCREAMING_SNAKE variant name, clients switch on `code`, never on `message`.

| Doc code | Doc status | Repo | Status |
| --- | --- | --- | --- |
| `MISSING_TITLE` | 400 | one `VALIDATION_FAILED` with per-field `data` (`crates/core/src/error.rs:173`, `dto.rs:56`) | **drift** — the doc's per-field codes do not exist |
| `DESCRIPTION_TOO_LONG` | 400 | same `VALIDATION_FAILED`; the 244 limit is enforced (`DESCRIPTION_MAX`) | **drift** — the limit matches, the code does not |
| `NOT_LIVE` | 400 | 409 `CONFLICT` (`error.rs:78`) | **drift** |
| `ALREADY_LIVE` | 400 (§3.1) / 409 (§2.1) | 409 | **doc self-contradiction** (D-10) |
| `NOT_PARTICIPANT` | 400 | `NOT_PARTICIPANT` (shared taxonomy) | code match; confirm 400 vs 403 |
| `JOIN_IN_PROGRESS` | 400 (§3.1) / 409 (§2.1) | 409 `ALREADY_JOINED` | **drift** in code + doc self-contradiction |
| `INVALID_TIME_ZONE` | 400 | `INVALID_TIME_ZONE` (`error.rs:169`) | match |
| `START_TIME_IN_PAST` | 400 | `START_TIME_IN_PAST` (`error.rs:154`) | match |
| `RECORDING_NOT_AVAILABLE` | 400 | — | missing |
| `BROADCAST_STILL_LIVE` | 400 | — (no publish) | missing |
| `DAILY_QUOTA_EXCEEDED` | 400 + `data.resetsInMins` | — | missing |
| `NOT_CREATOR` | 403 | `NOT_CREATOR` (`error.rs:69`) | match |
| `CANNOT_END` | 403 | — (creator check only) | missing |
| `COHOST_LIMIT_REACHED` | 403 | 409, limit **3** not 1 (`dto.rs:44`, `error.rs:178`) | **drift** (status + product limit) |
| `INVITATION_NOT_YOURS` | 403 | — (no invitations) | missing |
| `NOT_FOUND` | 404 | `NOT_FOUND` | match |
| `INVITATION_NOT_FOUND` | 404 | — | missing |
| `ALREADY_COHOST` | 409 | 409 (`error.rs:187`) | match |
| `DUPLICATE_INVITATION` | 409 | — | missing |
| `TOO_MANY_RECONNECTS` | 429 | `RateLimited` + `retry_after_secs` (`routes/ws.rs:150-165`) | match |
| `LIVEKIT_UNAVAILABLE` | 503 | `media_unavailable` (`error.rs:220`) | match |
| `DATABASE` / `INTERNAL` | 500 | shared taxonomy | match |
| — | — | `CREATOR_CANNOT_JOIN`, `HOST_CANNOT_LEAVE`, `ALREADY_LIVE_ELSEWHERE`, `CANNOT_ADD_SELF_AS_COHOST`, `IMAGE_TOO_LARGE` | **extra** — document or drop |

`data` is part of the contract — `resetsInMins` is the only example the doc gives. Every variant that carries extra data needs its key documented, because the Flutter pattern is `error.data['resetsInMins']` and a rename is silent.

### 5.2 WebSocket

| Doc `WsErrorCode` | Recoverable per doc | Repo (`apps/api/src/infrastructure/ws/errors.rs:88-130`) | Action |
| --- | --- | --- | --- |
| `TOKEN_EXPIRED` | true → call `/token` | `TokenExpired`, recoverable, doc comment points at `/broadcasts/:id/token` | endpoint missing |
| `BROADCAST_FORCIBLY_ENDED` | false | present | match |
| `KICKED_FROM_ROOM` | false | present | match |
| `DAILY_QUOTA_EXCEEDED` | false | — | missing (no quota) |
| `ROOM_NOT_FOUND` | false | present | match |
| `MEDIA_SERVER_ERROR` | true | present | match |
| — | — | `UNSUPPORTED` (non-fatal, frame dropped) | **extra**; the doc should name it because clients must not treat it as fatal |
| — | — | `SERVER_SHUTDOWN` (recoverable) | **extra**; §3.2 has no shutdown code, but the server sends one on process exit (`dto.rs:268-281`) |

`WsError` shape: `{ broadcastId, code, message, recoverable, data? }`. Repo's `recoverable` is derived from the code, not stored independently (`errors.rs:132-140`), which is the right invariant.

---

## 6. What "Tier 1 is locked" means

Tier 1 is a wire contract, so every clause is mechanically checkable. It is locked when:

1. **One casing.** Every response field the client reads is spelled the same on both sides. Today: backend camelCase, mobile snake_case → not locked (§2.2).
2. **One session shape.** `{ broadcast, token }`, no media URL, session minted by go-live/join and refreshed by `/token`. Today: `/token` missing.
3. **One DTO.** §6.2 fields exist under the doc's names, or the doc is amended to the repo's names; no field exists in one shape and not the other. Today: 4 fields missing (`creator_id`, `is_subscribed_to_creator`, `last_listened_at`, `creator_quota`), 4 renamed (`state`, `participant_role`, `is_joined`, `token`), and one extra `end_reason = none`.
4. **One state enum.** `broadcast_state` is authoritative, `RECONNECTING` is actually produced when the host is in grace, and the client renders it. Today: never produced.
5. **One event catalogue.** Server and client agree on names, direction, and payload keys, including `broadcastError` on the client. Today: names agree between doc and server, but four payload shapes differ, three events have no client handler, and two client events are dead.
6. **One error mapping.** Status + code per condition, with `data` keys documented. Today: `NOT_LIVE` and `COHOST_LIMIT_REACHED` differ; `ALREADY_LIVE` is contradictory in the doc itself.
7. **Refresh, not re-join.** `TOKEN_EXPIRED` → `/token` → `refreshToken`; the client does not re-announce itself as a new participant.
8. **Every end path ends.** Teardown, announcement, and idempotency on host end, grace/reaper, admin, quota. Today: two of four exist.

Until 1–8 hold, Tiers 2–4 are being built against a shape the client does not actually parse.

---

## 7. Status ledger

**Locked**

- Session envelope is `{ broadcast, token }` on both sides, RFC 3339 timestamps, no embedded media URL.
- Join is one HTTP round-trip; the server self-reports the join.
- Explicit go-live is atomic and refuses a second one (409).
- Explicit end writes the row before tearing down the room, and does not fail the response when teardown fails.
- Reaper ends abandoned broadcasts from **room presence**, with a persisted clock, cohost-aware health, and a single ending under concurrent sweeps.
- Heartbeat parameters satisfy the invariant a 2-miss tolerance requires.
- Offline buffer: 50 events, 300s, atomic drain, chronological replay.
- Reconnect rate limit: 10/60s, fail-closed, 429 with `retry_after` before upgrade.
- Error envelope and machine-readable `code` are the shared taxonomy, not a broadcast-local enum.
- Enum values match between backend and mobile (lowercase).

**Drift (both sides exist, contract differs)**

- Response field casing (camelCase vs snake_case) — **blocking**.
- `state`/`broadcast_state`, `participantRole`/`viewer_role`, `isJoined`/`viewer_is_in_room`, `gracePeriodInSecs`/`gracePeriodSecs`.
- Live count source: Redis counter (doc §7.1) vs DB `COUNT(*)` (repo).
- Client event names: mobile still uses `broadcastListenerLeft`; the doc and server now both say `participantLeft`.
- Endpoint spellings: `POST /live` vs `PUT /go-live`; `PATCH` vs `PUT`; `/cohosts` vs `/co-hosts`; the doc splits `/participants` (all-time) and `/live-participants` (current) where the repo has one `/participants`.
- Enum casing: SCREAMING_SNAKE (doc) vs lowercase (both implementations).
- `NOT_LIVE` 400 (doc) vs 409 (repo); `COHOST_LIMIT_REACHED` 403/1 (doc) vs 409/3 (repo).
- `published_at` = "went live" (repo) vs "recording published" (doc).

**Missing**

- `/token` refresh, `/keepalive`, `broadcastError` handling on mobile, host grace + `RECONNECTING`, `is_subscribed_to_creator`, `creator_quota`, `/quota`, bookmarks, continue-listening, mini-player, recording/publish, cohost invitations, quota watcher, admin end, `numberOfLiveBroadcasts`, WS room join/leave (so the doc's `endBroadcast`/`leaveBroadcast` client events are unimplemented).

**Extra (repo-only, needs a doc line or removal)**

- `POST /resume`, `POST /end`, `POST /leave`, `status` on the DTO, `BroadcastListItem`, `UNSUPPORTED`/`SERVER_SHUTDOWN` socket codes, `participantKicked`, `cohostDemotion`, `recordingPublished`, `scheduledBroadcast`, `broadcastDeleted`, `homeInvalidated`, chat events, `CREATOR_CANNOT_JOIN`/`HOST_CANNOT_LEAVE`, connection cap 5.

---

## 8. Naming reconciliation

**Decision (2026-10-23): `participant` is the canonical API noun.** A person in a broadcast is a participant; `listener` is retired from identifiers, event names, field names, enum values and routes. The product names that contain the word — *Listen Later*, *Continue Listening* — are unaffected, and "listening" remains the verb. The doc was amended to this vocabulary, so the remaining drifts below are only where the repo and the amended doc still differ.

### 8.1 Room-server config — pick one name

| Side | Name |
| --- | --- |
| Doc §1.2 | `LIVEKIT_HOST` (and `AppConfig.livekitHost` in the mobile example) |
| Backend | `LIVEKIT_URL`, legacy `LIVEKIT_HOST` still accepted (`config.rs:948` test) |
| Mobile | `MENO_LIVEKIT_URL` → `Config.liveKitUrl` |

Recommendation: keep the deploy-time variable `LIVEKIT_HOST` (doc) and make the mobile's build-time key resolve to it; if `MENO_LIVEKIT_URL` must stay for build flavours, document the alias in §1.2 rather than leaving three names for one value.

### 8.2 DTO fields

Doc name → repo name (all are drifts above): `broadcast_state`→`state`, `viewer_role`→`participant_role`, `viewer_is_in_room`→`is_joined`, `livekit_token`→`token`. `live_participants_count` and `total_participants` now agree in name; they differ only in *source* for the live one.

### 8.3 Events

After the participant decision, the doc and the server agree on `participantJoined`, `participantLeft` and `numberOfLiveParticipants`. What remains is `gracePeriodSecs`→`gracePeriodInSecs` (server) and the mobile's `broadcastListenerLeft` (client).

Each row is a one-line change on one side and a one-line change on the other; the cost of leaving them is a permanent translation layer.

---

## 9. Quick mismatch checklist

Each line is a test that should exist, not a reading exercise.

- [ ] Parse a real backend session response with the mobile DTO and assert `timeZone`, `imageUrl`, `publishedAt`, `participantRole`, `isJoined`, `liveParticipantsCount`, `totalParticipants`, `recordingEnabled`, `endReason`, `timeRemainingSeconds` are all non-default. *(Fails today: snake_case keys.)*
- [ ] `state` is one of the five lowercase values on both sides; if the doc wins, all five SCREAMING values round-trip.
- [ ] Host drops its socket → an active broadcast reads `RECONNECTING` on `GET /broadcasts/{id}` and on the list endpoints. *(Fails today: always `live`.)*
- [ ] Grace expiry ends the broadcast exactly once when two sweeps run concurrently; the room is deleted.
- [ ] Host returns at grace+1s → `resume` fails and the client is told why, not shown a dead room.
- [ ] `TOKEN_EXPIRED` on the socket → client calls the refresh endpoint, gets a token, `refreshToken` on the room, and does **not** re-announce.
- [ ] `broadcastError` frames are handled on mobile for every code in §5.2. *(Fails today: no handler.)*
- [ ] `endedBroadcast` arrives with one spelling only, and the client no longer needs its two-key reader.
- [ ] Client sends `leaveBroadcast` with `positionSeconds` and continue-listening reflects it.
- [ ] `is_joined` is false after leaving while `participant_role` may still be `participant`.
- [ ] `time_remaining_seconds` is null outside continue-listening and non-negative inside it.
- [ ] `creator_quota` is null for every non-host response.
- [ ] `published_at` is null for a live unrecorded broadcast (after D-03) and `duration_seconds` is still correct for an instant broadcast (D-04).
- [ ] A broadcast scheduled for yesterday is not `draft` (D-05).
- [ ] Ending twice returns the same outcome, never a 500; the room is deleted once.
- [ ] Two sockets reconnecting at once cannot deliver the same buffered events twice (the drain is atomic).
- [ ] A user who is offline for five minutes reconnects to a gap, not an ever-growing buffer.

---

## 10. One-line alignment rule

If a client field, event, code, or config key is not spelled exactly as it is in `docs/meno-broadcast-v2-final.md`, it is a translation layer that must be either renamed or added to the doc in the same change — and nothing new should be built on top of it until one of those has happened.
