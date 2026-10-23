# Broadcast v2 contract snapshots

> Workspace for comparing `docs/meno-broadcast-v2-final.md` against what backend and mobile actually do, before more code is built on top of either.
> These are inspection artifacts, not rules. The doc is the rule.

## What lives here

1. `broadcast_contract.ts` (this directory)
   A documentation artifact, kept out of `apps/mobile` on purpose: nothing in the app imports it, `flutter build` never sees it, and it is not a declared asset. It exists to be read and diffed, not compiled.

   One file, three parts:
   - **Part 1** — the repo's current wire shape, read from `apps/api/src/modules/broadcast/dto.rs` and `infrastructure/ws/model.rs`.
   - **Part 2** — the doc's target shape (§1.2, §3.2, §4.1, §6.2, §10.2, §10.3), under the doc's names and casing.
   - **Part 3** — the request DTOs, which are snake_case for bodies and camelCase for query parameters.

   Diff Part 1 against Part 2. Do not diff backend against mobile: both are wrong in the same direction today (see below).

2. `memo/broadcast-contract-snapshot.md`
   The section-by-section index: what the doc requires, the repo field/event that corresponds to it, the status of each pair, and the edge cases that must be defined before either side can call the contract locked.

3. `memo/broadcast-gap-analysis.md`
   The confirmed gaps with `path:line` evidence, plus a register of places where the doc contradicts itself and cannot be implemented as written (`D-01`–`D-32`).

4. `memo/broadcast-implementation-order.md`
   The tiers, each with edge cases and a checkable acceptance criterion.

## Naming

The canonical API noun is **participant**: a person in a broadcast is a participant, and `listener` is retired from identifiers, events, fields, enum values and routes (decision recorded in `broadcast-contract-snapshot.md` §8). *Listen Later* and *Continue Listening* are product names and keep their wording.

## How to use this directory

When reading a backend DTO, a mobile DTO, or a manager that reacts to a session event, compare it to Part 2 of `broadcast_contract.ts` in this directory and to the snapshot, not to the other implementation. If the two implementations agree with each other but not with the doc, the doc wins and one of them has to change.

Two things to keep in mind while reading:

- **Response casing is the first blocker.** The backend serialises camelCase; the mobile model reads snake_case (`apps/mobile/lib/features/broadcast/model/dtos.dart`). Almost every multi-word field is dropped before a screen sees it. Nothing about the session path can be called verified until that is fixed.
- **`reconnecting` is a defined-but-unproduced state.** The backend's `BroadcastState` includes it, and no code path constructs it, so every read says `live` even while the host is gone.

## Current meaning of "Tier 1 is locked"

See `memo/broadcast-implementation-order.md` §"Definition of done for Tier 1" and `memo/broadcast-contract-snapshot.md` §6. In short: one casing, one name per field and enum value, `RECONNECTING` actually produced, a session envelope of `{ broadcast, token }` with a refresh endpoint, server and client event catalogues that match name-for-name including `broadcastError`, one status and one code per error condition, and one ending per broadcast on every end path.

As of 2026-10-23 it is **not** locked. Locked: the session envelope, atomic go-live, single-round-trip join, explicit end ordering, the presence-based reaper, the heartbeat invariant, the offline buffer, the reconnect limiter, and the shared error taxonomy. Not locked: casing, field names, enum casing, `RECONNECTING`, `/token`, the event catalogue, the error mapping, end-path coverage, and the dead client vocabulary.

## Out of scope for this workspace

Web app work, full search (§5), recording/egress (§8.2), quota (§8.1), and the Listen Later / Continue Listening / Now Playing / context-menu / cohost-invitation features (§8.3–§8.7) are analysed in the gap register but are not part of the Tier 1 contract lock.
