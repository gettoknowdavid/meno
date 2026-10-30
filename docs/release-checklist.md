# Release checklist

Manual verification that cannot be done by a test suite: device behaviour, a real
backend, and the states only a person can put a phone into. Each row records the
scenario, what passing looks like, and the log excerpt that proves it. A row with no
excerpt is not done.

Run against a real server (`apps/api`) and a real device build — a simulator passing a
mock is not evidence, which is the whole point of this file.

## Realtime protocol (Step 78)

Findings: audit C04 (mobile and backend used incompatible realtime protocols) and H07
(reconnection had stale credentials, conflicting state, and ignored terminal reasons).

The framing contract (envelope shape, handshake refusal, the kick frame) is asserted by
`apps/api/tests/ws_protocol.rs`. What is left is what only a socket can show.

| #   | Scenario                                                          | Pass condition                                                                | Server log excerpt | Client log excerpt |
| --- | ----------------------------------------------------------------- | ----------------------------------------------------------------------------- | ------------------ | ------------------ |
| 1   | Sign in with the backend running; enter a broadcast room           | Handshake succeeds; one room event arrives within 5 seconds                   |                    |                    |
| 2   | Airplane mode for 60 seconds, then restore                         | Client shows reconnecting, then live; no duplicate session; roster reconciles |                    |                    |
| 3   | Host ends the broadcast while a listener is connected              | Listener moves to `ended` once; no reconnect loop                             |                    |                    |
| 4   | Kick a participant from the room                                   | Kicked client ends immediately; no retry                                      |                    |                    |
| 5   | Rotate the access token (force a refresh)                          | Connection continues or reconnects once; no 401 storm                         |                    |                    |
| 6   | Background the app for 5 minutes and resume                        | Exactly one connection; no duplicate delivery                                 |                    |                    |

### What to look for

- **Frame shape.** Every frame is `{event, data}` with a camelCase event name. A third
  top-level key, or a snake_case event, is a protocol change and belongs in
  `ws_protocol.rs` first. Client decoder:
  `apps/mobile/lib/features/broadcast/services/broadcast_events.dart`.
- **One connection.** `WebSocket connected` / `WebSocket disconnected` in the server log
  should pair up one-to-one per session. A background/foreground cycle that logs two
  connects without a disconnect in between is scenario 6 failing.
- **Terminal reasons.** `KICKED_FROM_ROOM` and `BROADCAST_FORCIBLY_ENDED` are fatal
  (`recoverable: false`); the client must close and stay closed. A reconnect after one of
  these is a failure, not resilience.
- **Refresh.** Scenario 5 must not produce a burst of 401s in the server log. The socket
  authenticates once, at the handshake, so a refreshed token means at most one new
  connection.

### Gate

Production realtime stays enabled only after all six rows carry their excerpts. Until
then the app ships with the visible "realtime unavailable" state (Step 44) rather than a
silently dead room.
