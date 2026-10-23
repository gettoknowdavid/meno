# Broadcast implementation order

Tier 1 locks the wire contract. Tiers 2–4 build resilience, quota, and Figma completeness on top of it.

**Scope:** backend (`apps/api`) and mobile Flutter (`apps/mobile`).
**Reference:** `docs/meno-broadcast-v2-final.md` (real sections: 1–10, subsections 1.1–1.2, 2.1–2.5, 3.1–3.2, 4.1, 5.1–5.3, 6.1–6.3, 7.1, 8.1–8.7, 10.1–10.3).
**Companions:** [contract snapshot](broadcast-contract-snapshot.md) · [gap analysis](broadcast-gap-analysis.md) · [yardstick](broadcast-contract-workspace/README.md).

Each item has the same shape: what the doc requires, the edge cases the item must close (not just the happy path), a checkable acceptance criterion, and where the code stands today.

---

## Tier 1 — make the contract readable and unambiguous

Do these first. Every later tier is unverifiable while the client cannot parse the response.

### 1.1 Make the client parse the response (blocking)

**Doc source:** §6.2 (field names), §1.2 (session envelope).
**Requirement:** the JSON the server sends is the JSON the client model reads, with one casing convention. §6.2 is camelCase on the wire because every doc struct carries `#[serde(rename_all = "camelCase")]`.

**Edge cases to close:**
- Nested objects too: `creator.fullName`, `creator.avatarId`, `creator.avatarUrl` (the mobile reads `full_name`).
- Optional vs required must not be papered over by `@Default` — a defaulted `isJoined: false` is indistinguishable from a real `false`.
- Request side is already mixed (create/update bodies snake_case, query params camelCase); keep each as the backend expects it and do not "unify" it into a new bug.
- Unknown fields must be ignored, not fatal, so a backend addition does not crash an older build.
- Dates: RFC 3339 only; a bare Dart `DateTime` must never serialise as a nine-number tuple.

**Acceptance:** a test parses a fixture captured from a real backend response and asserts `timeZone`, `imageUrl`, `imageId`, `publishedAt`, `participantRole`, `isJoined`, `isBookmarked`, `liveParticipantsCount`, `totalParticipants`, `recordingEnabled`, `endReason`, `timeRemainingSeconds`, and `creator.fullName` are all non-default. It fails on today's tree.

**Current:** backend camelCase (`modules/broadcast/dto.rs:299`), mobile snake_case (`mobile/.../dtos.dart:19-41`, `dtos.g.dart:14`), no key normalisation in the HTTP client. **Not locked.**

### 1.2 One name and one casing per field

**Doc source:** §6.2, §6.3.
**Requirement:** the §6.2 DTO exists under the doc's names, or the doc is amended to the repo's — but not both. Recommended: doc names win, because mobile is pre-release and the rename is mechanical.

**Edge cases to close:**
- `viewer_is_in_room` must keep the repo's semantics (`left_at IS NULL`), not §6.3's `role != NONE`; the doc's derivation is wrong (D-02).
- `end_reason` must be `null` for a live broadcast. The repo's extra `"none"` variant makes a live broadcast say it ended in the mobile label map.
- `creator` optionality: §6.2 says optional, repo is non-optional. Pick the repo's (always present) and amend the doc, or make the client tolerate absence.
- Enum casing: §6.2 says SCREAMING_SNAKE; both implementations use lowercase. One decision, applied everywhere. The noun itself is settled: `participant` (see snapshot §8).
- `creator_id` is missing from the repo DTO even though §6.2 has it and the list item has it.
- `is_bookmarked` is in §8.3 but absent from §6.2; `is_subscribed_to_creator` is in §6.2 and absent from the repo.

**Acceptance:** a single table-driven test per enum asserts every wire value round-trips both sides; a field-presence test asserts every §6.2 field exists in a real response.

**Current:** 8 fields missing, 12 renamed, 3 enums cased differently, one extra `none` variant. **Not locked.**

### 1.3 Emit `RECONNECTING` (or delete it)

**Doc source:** §6.1, §6.2, §2.3.
**Requirement:** when the host's connection has dropped, a client that fetches the broadcast must be told `RECONNECTING`, not `LIVE`.

**Edge cases to close:**
- The overlay must be applied on **every** read: detail, list, session, keepalive. A list that says live while the detail says reconnecting is worse than both saying live.
- Grace in progress and the host returns: the next read must return `LIVE` again, and the room must get `hostReconnected` exactly once.
- Grace expires: the next read must return `ENDED` with `end_reason = HOST_DISCONNECTED`, not `RECONNECTING` forever.
- A process restart during grace must not lose the window (the doc's in-process watcher does; the repo's `abandoned_since` does not).
- A cohost still in the room: decide whether that is healthy. Repo says yes; the doc implies no. One rule, written down, applied to both the signal and the ending.
- A host who is present but not publishing: presence alone says "alive" while the room has no audio. Health should require publishing.

**Acceptance:** with the host's socket dropped, `GET /broadcasts/{id}` returns `RECONNECTING` within one read; after reconnect it returns `LIVE`; after the window it returns `ENDED` with the right reason; a concurrent end cannot produce two `endedBroadcast` frames.

**Current:** `Reconnecting` is defined and never constructed (`modules/broadcast/model.rs:314`, no producer); grace config and Redis keys exist unused; the reaper is the real end path. **Not locked.**

### 1.4 One session surface

**Doc source:** §1.2, §10.1.
**Requirement:** go-live, join, and refresh all answer `{ broadcast, token }`; the media URL never appears in the response.

**Edge cases to close:**
- Go-live on an already-live broadcast: 409, including two replicas racing (already true).
- Join twice: 409 (already true), but rejoin after leaving must succeed and must not reset the listen position (D-08).
- Join after end: one status, documented (`NOT_LIVE`).
- Join own broadcast: 403, documented (D-12).
- Deleted between list and join: 404.
- `/token` must infer the role from the current roster, never from the request. A demoted cohost must receive a participant token; a removed participant must be refused.
- `/token` must refuse a non-live broadcast and be rate limited.
- Six devices: the connection cap (5) must be documented, with the same error shape (D-13).
- The future `livekit_url` must be optional, with the client keeping `?? config` rather than assuming presence.

**Acceptance:** contract tests for go-live / join / rejoin / resume / refresh: status codes, body shape, and role inferred from the roster in each case.

**Current:** envelope matches; `/token` and `/keepalive` missing; `/resume` is a repo-only path the doc does not describe; `POST /live` vs doc `PUT /go-live`; `/cohosts` vs `/co-hosts`; the doc splits `/participants` (all-time) and `/live-participants` (current) where the repo has one `/participants`. **Partially locked.**

### 1.5 One event catalogue, including error frames

**Doc source:** §10.2, §10.3, §3.2.
**Requirement:** every event the server sends has a client handler, one spelling, and one payload shape; every event the client sends is accepted or removed.

**Edge cases to close:**
- `broadcastError` must be handled on mobile for every code, and `recoverable` must drive retry vs navigate-away. Today there is no handler at all.
- `TOKEN_EXPIRED` must lead to `/token` + room token refresh, not a re-join (a re-join re-announces the participant and duplicates the roster event).
- `UNSUPPORTED` is **not** fatal — the frame is dropped, the socket stays open. A client that closes on it breaks working sockets.
- `SERVER_SHUTDOWN` is recoverable: reconnect elsewhere.
- `participantJoined` payload: the doc sends `broadcastId` + `UserSummaryDto`; the server adds `count`. One shape.
- `participantLeft` payload: the doc sends `broadcastId` + `userId`; the server adds `count` and a participant projection. One shape.
- `hostDisconnected.gracePeriodSecs` (doc) vs `gracePeriodInSecs` (server): one key; the client's two-key reader can then be narrowed.
- `endedBroadcast` payload: one spelling; the client's defensive reader is retired with it.
- Removed cohost receives `removedCohost`; evicted participant receives `KICKED_FROM_ROOM`. Decide which event wins when both happen in the same action, so the client does not show two dialogs.
- `cohostInvitation` payload: doc `{ invitationId, inviterName, broadcastTitle }` vs server `{ broadcastId, token }`.
- `numberOfLiveBroadcasts` fanned out to every socket needs coalescing (D-31).

**Acceptance:** a generated catalogue test compares the server's `WsEvent` list and the client's `SocketEvent` list name-for-name in both directions, with an explicit allowlist of documented repo-only events.

**Current:** the server and the amended doc agree on every event name; what remains is payload shape (`participantJoined`/`participantLeft`/`cohostInvitation`/`removedCohost`), `gracePeriodInSecs`, no mobile `broadcastError` handler, no mobile handlers for the participant events, and two dead client-to-server names. **Not locked.**

### 1.6 One error mapping

**Doc source:** §3.1, §3.2.
**Requirement:** each condition has one status and one code, with `data` keys documented where the code carries data.

**Edge cases to close:**
- 409 for `ALREADY_LIVE`, `JOIN_IN_PROGRESS`/`ALREADY_JOINED`, `NOT_LIVE` — the doc's §3.1 lists the first two under 400 while §2.1 says 409 (D-10).
- `COHOST_LIMIT_REACHED`: status 403 vs 409 and limit 1 vs 3 (D-29).
- Codes the repo adds (`CREATOR_CANNOT_JOIN`, `HOST_CANNOT_LEAVE`, `ALREADY_LIVE_ELSEWHERE`, `CANNOT_ADD_SELF_AS_COHOST`, `IMAGE_TOO_LARGE`) must be in the doc's enum, or the client is switching on values it cannot find.
- `data.resetsInMins` is indexed by name in the Flutter pattern; any rename is silent.
- Per-field validation codes vs one `VALIDATION_FAILED` with `data.fields`: the doc lists `MISSING_TITLE`/`DESCRIPTION_TOO_LONG`; the repo returns one code plus field detail. Choose, and make the client's switch match the choice.
- Validation must accumulate, not short-circuit: two bad fields should not need two round-trips.
- The envelope must be identical between HTTP and a refused WS upgrade.

**Acceptance:** a table-driven test asserts `(condition → status, code, data keys)` for every variant, and the client has a matching exhaustive switch with no default swallowing unknown codes silently.

**Current:** `NOT_LIVE` 400 vs 409, cohost limit 403/1 vs 409/3, extra codes on both sides, `data` undocumented. **Not locked.**

### 1.7 Every end path ends once

**Doc source:** §2.1, §2.3, §8.1, §10.2.
**Requirement:** host end, grace/reaper end, admin end, quota end all write the row, delete the room, close open participant rows, and announce exactly once.

**Edge cases to close:**
- Two end paths racing: exactly one ending and one announcement (the reaper's row guard is the pattern).
- Room teardown failure: the response must still tell the truth (ended), with the failure logged, not a 500 that leaves the client showing live.
- Ending an already-ended broadcast: idempotent 200 or 409, never 500.
- Participant rows must be closed on end, or the live count and the next `total_participants` are wrong (D-20).
- Admin end and quota end do not exist yet, so their paths cannot be "confirmed"; they are missing, not untested.
- The client's own disconnect is never the only cleanup path.

**Acceptance:** a concurrency test runs two end paths for one broadcast and asserts one `endedBroadcast` frame and one `end_room` call.

**Current:** explicit end and reaper end are correct; admin and quota paths do not exist. **Partially locked.**

### 1.8 Retire the pre-v2 vocabulary

**Doc source:** §10.2, §1.2.
**Requirement:** the client does not send events the server does not accept, and does not expect acknowledgements the server does not send.

**Edge cases to close:**
- `startedBroadcast` and `joinBroadcast` are sendable today and answered with "not available yet". Either implement WS room join/leave (the doc's model) or delete the client names and the handlers that listen for them.
- Reconnect must not depend on a socket join that never succeeds; the live scope recovery must be the single path.
- Removing an enum value must remove every `switch` branch that references it, or the compiler stops being the contract.

**Acceptance:** the client's socket enum contains no value the server rejects; a reconnect test shows one re-entry path and no duplicated announcements.

**Current:** both names present, no server support. **Not locked.**

---

## Tier 1 status, plainly

**Locked:** session envelope; atomic go-live; single-round-trip join; explicit end ordering; reaper's presence-based, persisted, cohost-aware abandonment with a single ending; heartbeat invariant; offline buffer; reconnect limiter; shared error taxonomy; backend/mobile enum agreement.

**Not locked:** response casing (blocking), §6.2 field names and enum casing, `RECONNECTING`, `/token`, event catalogue, error mapping, end-path coverage, dead client vocabulary.

---

## Tier 2 — resilience on top of the locked contract

**Doc source:** §2.1–§2.5, §4.1.

### 2.1 Grace: one mechanism, not two

- The doc's tiered Redis grace and the repo's reaper are two implementations of one rule. Choose: keep the reaper and add tiers + a `RECONNECTING` projection, or implement the Redis tiers with a persisted deadline. Do not ship both.
- Close D-16 (restart loses the timer), the check-then-end race (G2), the tier reset after an hour (G3), and the "connected but not publishing" hole (G5).
- Decide cohost-keeps-alive once and apply it to both the signal and the ending.

### 2.2 Heartbeat and presence

- Adopt one parameter set that satisfies `max_missed × ping ≤ participant_timeout`; the doc's 25/60/20/2 does not (D-06).
- Define the relationship between the protocol ping and the client `heartbeat` event (D-15): one liveness signal, one advisory.
- Presence TTL must outlive the longest legal gap between pings, or a healthy host is dropped by the reaper.

### 2.3 Buffer and reconnect

- Close the register/buffer race (lock register+drain, or accept at-most-once with a refetch on ready).
- Keep the TTL set once, not refreshed (already correct).
- Document that replay is stale and that counts are absolute.
- Give the 429 a `Retry-After` and decide how a browser client learns it (D-17).

### 2.4 Keepalive

- Implement `POST /broadcasts/:id/keepalive` with the doc's four payload fields.
- A host keepalive during grace must run the same grace-clear and announcement as the socket path, or the room is told two different things.
- Answer `NOT_LIVE` after grace expiry; never resurrect.
- Answer `token_ttl_seconds` rather than inventing an absolute expiry the server cannot know (D-18).

### 2.5 Resume vs refresh

- `/resume` (repo) and `/token` (doc) must not diverge: resume re-opens the roster row, refresh does not. Document which one a client calls on launch, on socket `TOKEN_EXPIRED`, and on foreground.
- The launch recovery path must not show a room before the recovered session is usable.

---

## Tier 3 — quota

**Doc source:** §8.1, §3.1 `DAILY_QUOTA_EXCEEDED`, §3.2.

- Close D-22 (midnight split, start time from the row not a TTL'd key, expiry at the next midnight) and D-23 (cap the session at the remaining time).
- The watcher's 60s granularity means up to 60s of overshoot: decide whether that is charged or forgiven.
- The quota day is UTC while the product is Nigeria-facing (UTC+1): the reset lands at 01:00 local. State it.
- Define the mid-broadcast path: WS `DAILY_QUOTA_EXCEEDED` (doc) then auto-end, with the same single-ending guarantee as Tier 1.7.
- `GET /broadcasts/quota` and `creator_quota` in the go-live response must agree; §6.2's `QuotaDto` and §8.1's differ by `remaining_display` (D-07).

---

## Tier 4 — the rest of the catalogue

**Doc source:** §8.2–§8.7, §5.1–§5.3.

- **Recording/publish (D-24):** presign on read from the key; never persist a presigned URL; handle `EGRESS_FAILED` and duplicate/out-of-order webhooks; publish is idempotent; a broadcast that is still live cannot be published.
- **Bookmarks (D-25):** idempotent add/remove; soft-deleted broadcasts excluded; define the "no recording" presentation.
- **Continue listening (D-26):** clamp positions; write on heartbeat as well as leave; derive a server-side floor from `joined_at`; `time_remaining_seconds` null outside this context.
- **Mini-player (D-27):** one session, tie-broken; reconcile open rows against presence.
- **Cohost invitations (D-28):** partial unique index so a declined invite can be re-sent; accept on a non-live broadcast defined; accept/decline is idempotent; removing a cohost evicts them (D-09).
- **Search (§5):** D-30 (anonymous subscriptions filter, count query parity), and the GIN expression must match the query expression exactly or it is unused.

---

## Definition of done for Tier 1

- [ ] A real response parsed by the real client model yields no defaults.
- [ ] One casing, one name per field, one value per enum, on both sides and in the doc.
- [ ] `RECONNECTING` is observable on every read and returns to `LIVE`.
- [ ] `/token` exists, infers role from the roster, is rate limited, and `TOKEN_EXPIRED` points at it.
- [ ] The client handles `broadcastError` and every code in §3.2.
- [ ] Server and client event catalogues match name-for-name, with documented extras.
- [ ] One status and one code per error condition, with `data` keys listed.
- [ ] Every end path ends once, deletes the room, closes rows, announces once.
- [ ] No client event exists that the server refuses.
