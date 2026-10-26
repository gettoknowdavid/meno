# Audit Report — Implementation Guide

**Purpose.** This guide turns [docs/audit-report.md](audit-report.md) into an ordered set of small, buildable steps, using [docs/account-di-migration.md](account-di-migration.md) as the reference design for the account/auth/DI/session/routing slice.

**How to use it.** Work top to bottom. Each step names its audit source, the exact files, the shape of the code, and how to prove it worked. Steps inside a phase are ordered; a step may only start when its `Depends on` steps are merged.

**Scope.** All 64 findings of the audit (C01–I02), the audit's §1 scope redesign (`ActiveBroadcastManager`), §4 over-engineering ledger, §5 keep-list, §6 feature readiness, §7 reference architecture, §8 priorities — mobile Dart (`apps/mobile`) and backend Rust (`apps/api`).

**Non-negotiable.** The audit is the source of truth for _what_ changes. Where the design document is heavier than one developer needs, the step says so in one line and keeps the safety requirement intact. Nothing in the audit listed as "done well" is rewritten.

**Status of this document.** It is instructions, not a claim of work done. No code here has been compiled, and no device test has been run.

**Revision note (alignment and simplification pass).** This version applies the review "Implementation Guide Review: Alignment, Drift and Over-Engineering". Three things changed. (1) Eleven places where the guide drifted from the audit were closed (review A-1 to A-11). (2) Twenty-six internal contradictions were fixed (D-1 to D-26). (3) The mechanisms the audit never asks for were replaced with plain ones, keeping the safety property each was protecting (OE-1 to OE-25). Where this guide and `account-di-migration.md` disagree, **the audit wins**, and where the audit offers "A or B" the simpler option is taken. Steps that were simplified say so in a `Simplified from ...` line. Step numbers are kept stable so every cross-reference still works; steps that were removed stay in the numbering as short stubs, and steps that must ship together are grouped in §5.5 (**Delivery groups**, §5.5).

---

## 1. Assumptions and open questions

The brief says to decide and move on, so each item below has a default already baked into the steps. Change the default before starting the phase that depends on it if you disagree.

| #   | Assumption                                                                                                                                                                                                                                                                                                                                                                                                   | Default taken                                                                                                                                                                                                                                                                                                                                                                                            | Where it bites                  |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| A1  | Audit paths are written as `meno/apps/mobile/...`; this repo is a monorepo whose app lives at `apps/mobile`. Every path in this guide is repo-relative, e.g. `apps/mobile/lib/locator.dart`.                                                                                                                                                                                                                 | Repo-relative paths used throughout.                                                                                                                                                                                                                                                                                                                                                                     | Every step                      |
| A2  | Repo already has `freezed`, `freezed_annotation`, `build_runner` and committed `*.freezed.dart` files (believed `freezed 4.0.2`, `freezed_annotation 3.1.0`; **unverified**). Freezed is used only for the three value classes in section 5.2; everything else uses enums and native sealed classes.                                                                                                         | Read `apps/mobile/pubspec.yaml` first and correct this row if it is wrong; use what is installed; do not upgrade.                                                                                                                                                                                                                                                                                        | Step 0                          |
| A3  | Dart SDK is believed to be `^3.13.5`, Flutter 3.47.x (**unverified**; read `pubspec.yaml`). The code uses the `new(...)` declaring-constructor shorthand; freezed does not generate that shorthand, so new freezed classes use plain `const factory(...)` and hand-written classes keep `new(...)`. Both compile side by side.                                                                               | Mixed style, documented in Step 0.                                                                                                                                                                                                                                                                                                                                                                       | Step 0                          |
| A4  | "One developer" means no interfaces for mocking, no event bus, no generic lifecycle framework. Fakes are concrete classes passed through constructors.                                                                                                                                                                                                                                                       | Concrete collaborators everywhere.                                                                                                                                                                                                                                                                                                                                                                       | All steps                       |
| A5  | Sessions are single-use refresh tokens (audit §3 H09, backend `apps/api/src/modules/auth/token.rs`).                                                                                                                                                                                                                                                                                                         | Any uncertain refresh outcome requires reauthentication; never auto-retry.                                                                                                                                                                                                                                                                                                                               | Step 10, Step 66                |
| A6  | The app may ship with realtime visibly unavailable (design §5.6). HTTP/account safety must not wait for the WebSocket work.                                                                                                                                                                                                                                                                                  | Realtime enablement is gated behind Step 47 + Step 78.                                                                                                                                                                                                                                                                                                                                                   | Phase R                         |
| A7  | Notifications durability is deferred to the backend inbox (audit H24, audit §6 "build first: backend inbox contract"). The remediation keeps only the root payload fix and platform adapter (Step 45). The inbox, token policy and client inbox (Steps 70, 71, 73) are a separate **Feature track** after the remediation. Until the inbox exists the app shows an honest "notifications unavailable" state. | Documented in Steps 70/71.                                                                                                                                                                                                                                                                                                                                                                               | Phase U                         |
| A8  | The project commits generated files (`.freezed.dart`, `objectbox.g.dart`, `*.g.dart` are in `lib/` and not in `.gitignore`) (**unverified**; read `.gitignore`).                                                                                                                                                                                                                                             | Generated files are committed; regenerate and commit them in the same change. Correct this row if `.gitignore` says otherwise.                                                                                                                                                                                                                                                                           | Step 0                          |
| A9  | `ObjectBox 6.0.0-preview.4` is accepted as a deliberate risk (audit I02). The risk is recorded (Step 46); schema changes are gated; the upgrade check is one line in `docs/release-checklist.md` (Step 80).                                                                                                                                                                                                  | Keep preview, add the gate.                                                                                                                                                                                                                                                                                                                                                                              | Step 46, Step 80                |
| A10 | Localization is deferred, as a **conscious deviation from audit L06** ("introduce localization before expanding UI copy"). It is deferred because one developer cannot add an l10n framework inside a remediation. Mitigation: each screen's strings live in one block at the top of its file, and the unsupported language setting is labelled as unsupported.                                              | Documented limitation, no l10n framework added.                                                                                                                                                                                                                                                                                                                                                          | Step 87                         |
| A11 | Two mocking libraries exist (Mockito, Mocktail). New and repaired tests use **Mocktail** only; untouched Mockito tests are not converted in this work.                                                                                                                                                                                                                                                       | Mocktail for new and repaired files.                                                                                                                                                                                                                                                                                                                                                                     | Step 83                         |
| A12 | The `live:<id>` DI tier is deleted, not refactored (audit §1.4). Its state machine is rebuilt inside one account-owned manager.                                                                                                                                                                                                                                                                              | See Phase P3.                                                                                                                                                                                                                                                                                                                                                                                            | Steps 28–35                     |
| A13 | **Installed builds.** The audit says the app is not release-ready (M25) and the guide never states that a production install base exists. The legacy-migration machinery (review OE-3) is only needed if real users have an installed build.                                                                                                                                                                 | **Default taken: assume SOME installed builds exist**, so Step 5 is a ~15-line startup cleanup (not a migration framework). **Ask the developer:** "Is there any build with real users, or only your own devices?" If the answer is _only own devices_: delete Step 5, Step 75 and Step 80's upgrade part, the legacy halves of Steps 16, 37 and 38, and the Step 46 UID test; reinstall on dev devices. | Steps 5, 16, 37, 38, 46, 75, 80 |
| A14 | **CI.** Nothing in the audit shows that a CI pipeline exists, and the audit does not require one (M22, M25 need a one-way import graph and production signing only).                                                                                                                                                                                                                                         | Signing values come from a local, git-ignored `key.properties`; the import-direction script runs by hand or from a git pre-push hook. **If a CI system already exists (verify), keep CI wording.**                                                                                                                                                                                                       | Steps 17, 77, 82, 83            |
| A15 | Step 76 offers background audio as Option A (supported) or Option B (blocked).                                                                                                                                                                                                                                                                                                                               | **Default: Option A**, because the product is live audio. Confirm with the developer.                                                                                                                                                                                                                                                                                                                    | Step 76                         |
| A16 | Items that cannot be known without the repo are marked **(verify)** in the steps: installed builds (A13), CI (A14), pinned Flutter/ObjectBox APIs, backend idempotency replay, backend ended-broadcast filter, shared-fixtures folder.                                                                                                                                                                       | Read the code first; do not guess.                                                                                                                                                                                                                                                                                                                                                                       | Steps 12, 37, 52, 58, 61, 77    |

**Open questions that are _not_ blocking** (defaults are in the steps, escalate only if a product decision changes them): exact approved API origin list (Step 8 assumes one origin from `Config`), whether biometric gating of the vault is required (assumed no, Step 3), whether notification taps survive process death (assumed no, Step 22), and whether the backend will supply a refresh-recovery endpoint (assumed no, Step 10).

---

## 2. Big picture

Two application DI tiers, one owner per concern, ordinary objects everywhere else.

```text
┌──────────────────────────── root scope (app lifetime) ────────────────────────────┐
│ Config · MenoLogger · FlutterSecureStorage/SecureStorage                          │
│ Database (one ObjectBox handle) · Preferences/LocalStorage                        │
│ HttpClient (anonymous transport only, no implicit account)                        │
│ SessionStore (the only writer of credentials)                                     │
│ AccountTransitionOwner (the only thing that pushes/pops the account scope)        │
│ AuthManager (UI commands) · UserManager (signed-out greeting only)                │
│ MediaService · PermissionsService · Push/LocalNotifications (platform adapters)   │
│ OnboardingManager · ThemeManager · GoRouter + ONE pending destination             │
└───────────────────────────────────────────────────────────────────────────────────┘
                    │ push exactly one `authenticated` scope when ready
                    ▼
┌──────────────── authenticated scope (one account, one generation) ───────────────┐
│ AccountContext (immutable id + generation)                                        │
│ Account-bound HttpClient facade + SessionInterceptor (identity-pinned)            │
│ ProfileCacheService · SettingsLocalService · BroadcastLocalService (each          │
│    takes ownerId in its constructor; no generic facade above them)                │
│ BroadcastHttpService · ProfileHttpService · SettingsHttpService                   │
│ UserProfileRepository (stable subject identity, refcounted proxies)               │
│ MyProfileManager · SettingsManager (initialized once, no global selectors)        │
│ SocketClient + BroadcastSocketService + AccountConnectionOwner                    │
│ LiveSessionStore (non-secret recovery intent only)                                │
│ ActiveBroadcastManager ── owns at most one ActiveBroadcastSession                 │
│      ActiveBroadcastSession = RoomTransport + roster + media subscriptions        │
│ FavouritesManager                                                                 │
└───────────────────────────────────────────────────────────────────────────────────┘
                    │ widget lifetime (never in DI)
                    ▼
┌──────────────── widget-owned objects ─────────────────────────────────────────────┐
│ BroadcastEditorManager · ProfileEditorManager · UserProfileManager (details)      │
│ EmailVerificationManager · feed data sources via createOnce · form controllers    │
└───────────────────────────────────────────────────────────────────────────────────┘
```

Reading the diagram: an arrow means "is created by". The `authenticated` scope is a **GetIt scope** — a named layer of registrations that is pushed when an account becomes usable and popped (disposed) when it leaves. Nothing below root is ever pushed by a page.

---

## 3. Glossary

Plain-language definitions for every term this guide leans on. Read once; refer back as needed.

| Term                                    | One-line meaning                                                                                                                                                                                                                                  |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **GetIt scope**                         | A named layer of registrations stacked on top of the root registrations. Lookup searches top-down; popping the layer disposes everything registered in it.                                                                                        |
| **`registerLazySingleton`**             | Registers a factory that runs on first lookup and is reused afterwards. Nothing is constructed until someone asks.                                                                                                                                |
| **`registerSingleton`**                 | Registers an already-constructed object immediately; disposal goes through the `dispose:` callback.                                                                                                                                               |
| **freezed**                             | Code generator producing `copyWith`, `==`, `hashCode`, `toString` for immutable classes. Used here only for `CredentialRecord`, `LiveSnapshot` and `AccountSnapshot` (section 5.2).                                                               |
| **sealed class**                        | A native Dart 3 class hierarchy where every subtype is known, so `switch` can be exhaustive and the compiler complains when a case is missing. No code generation needed. Used for outcomes that carry data. Payload-free sets are plain `enum`s. |
| **`ValueNotifier` / `ValueListenable`** | A box holding a value that tells listeners when the value changes. `ValueListenable` is the read-only view.                                                                                                                                       |
| **FIFO queue**                          | First-in-first-out work queue: jobs run one at a time in the order they arrived, and one job's failure does not drop the queue.                                                                                                                   |
| **compare-and-set (CAS)**               | "Write only if the thing I expect is still there." Used so a late refresh cannot overwrite a newer session.                                                                                                                                       |
| **idempotent**                          | Calling it twice is the same as calling it once. Every close/dispose path in this guide is idempotent.                                                                                                                                            |
| **epoch**                               | A random identifier for one login lineage, stored in the credential record. A fresh login produces a new epoch, so a late refresh for an old login can be recognised and discarded.                                                               |
| **generation**                          | An in-memory counter, incremented each time an account is activated. Used to key the account subtree and to fence late async work. It is not persisted: nothing survives a process kill, so persisting it would protect nothing.                  |
| **interceptor**                         | A hook that runs around every HTTP request (attach token, react to 401).                                                                                                                                                                          |
| **single-flight**                       | At most one in-flight operation for a key; concurrent callers wait for the same result.                                                                                                                                                           |
| **proxy**                               | A wrapper around a DTO that adds behaviour (commands, overrides) and notifies widgets on change.                                                                                                                                                  |
| **`createOnce`**                        | A `watch_it` helper that creates a widget-local object on first build and disposes it when the widget unmounts.                                                                                                                                   |
| **`watchValue`**                        | A `watch_it` function that reads a `ValueListenable` off a DI object and rebuilds the widget when it changes.                                                                                                                                     |
| **restriction (command_it)**            | A `ValueListenable<bool>` where `true` means "cannot run". Not a concurrency lock.                                                                                                                                                                |
| **owner-fenced**                        | After every `await`, the code re-checks that its account, generation and owner are still the live ones before touching anything.                                                                                                                  |
| **transport**                           | The layer that physically sends an HTTP/WebSocket request. It never decides which identity to use.                                                                                                                                                |

---

## 4. Traceability table

One row per audit finding. **Step** lists every step that touches it; the first listed step is where it is finished. **Status** says who does the work: Mobile (Dart), Backend-Rust (`apps/api`), Both, or Device-test only.

| ID  | Title                                                                               | Severity | Step(s)                                      | Status       |
| --- | ----------------------------------------------------------------------------------- | -------- | -------------------------------------------- | ------------ |
| C01 | Requests can cross account boundaries during replay                                 | CRITICAL | 1, 8, 9, 13                                  | Mobile       |
| C02 | Session-store mutations are neither atomic nor identity-conditional                 | CRITICAL | 2, 3, 4, 5, 10, 23, 74                       | Mobile       |
| C03 | Explicit logout can leave a usable session recoverable                              | CRITICAL | 2, 3, 5, 7, 23, 74, 75                       | Mobile       |
| C04 | Mobile and backend use incompatible realtime protocols                              | CRITICAL | 47, 78                                       | Mobile       |
| C05 | Backend access-token revocation is not enforced by the actual HTTP guard            | CRITICAL | 65                                           | Backend-Rust |
| H01 | Authenticated UI is not synchronized with account-scope readiness                   | HIGH     | 1, 18, 19, 20, 21                            | Mobile       |
| H02 | Live transitions can outlive account teardown or supersede newer sessions           | HIGH     | 15, 18, 30, 31                               | Mobile       |
| H03 | Session disposal can access an uninitialized timer                                  | HIGH     | 29                                           | Mobile       |
| H04 | Failed end/leave skips session closure; ended UI depends on disposed resources      | HIGH     | 15, 29, 32, 33                               | Mobile       |
| H05 | Broadcast end and cohost-removal contracts are wrong                                | HIGH     | 51, 52                                       | Both         |
| H06 | No production path starts realtime delivery                                         | HIGH     | 44, 48                                       | Mobile       |
| H07 | Reconnection has stale credentials, conflicting state, and ignored terminal reasons | HIGH     | 49, 78                                       | Mobile       |
| H08 | Publishing permission is mistaken for creator authority                             | HIGH     | 28                                           | Mobile       |
| H09 | Refresh failures destroy sessions, including ambiguous rotated-token outcomes       | HIGH     | 10, 66                                       | Both         |
| H10 | Stored access-token expiry moves forward on every read                              | HIGH     | 2, 3, 4, 5, 6                                | Mobile       |
| H11 | Refresh, pagination, query changes, and invalidations race                          | HIGH     | 60                                           | Mobile       |
| H12 | Client-filtered empty pages strand pagination                                       | HIGH     | 61                                           | Mobile       |
| H13 | Cold profile proxies share an empty identity and cannot be released correctly       | HIGH     | 39                                           | Mobile       |
| H14 | Viewer-specific profile cache state leaks across accounts                           | HIGH     | 16, 36, 38                                   | Mobile       |
| H15 | Async operations continue after their owner is disposed                             | HIGH     | 19, 26, 34, 43                               | Mobile       |
| H16 | Participant UI does not implement the backend roster contract                       | HIGH     | 53, 54                                       | Both         |
| H17 | Backend cohost removal does not reliably revoke role or presence                    | HIGH     | 55                                           | Backend-Rust |
| H18 | Backend broadcast transitions are not safely retryable                              | HIGH     | 56                                           | Backend-Rust |
| H19 | Editor progress and retries can report success without completing entry             | HIGH     | 57                                           | Mobile       |
| H20 | Broadcast mutations do not carry stable logical-operation idempotency keys          | HIGH     | 58                                           | Both         |
| H21 | Password-reset requests revoke sessions before proving ownership                    | HIGH     | 67                                           | Backend-Rust |
| H22 | Notes sync can lose folders created in the same batch                               | HIGH     | 68                                           | Backend-Rust |
| H23 | Chat deletion can mutate one room and publish to another                            | HIGH     | 69                                           | Backend-Rust |
| H24 | Push and durable notifications are not implemented end-to-end                       | HIGH     | 45, 79 (remediation); 70, 71 (Feature track) | Both         |
| H25 | Client validation rejects server-valid accounts and can break credential storage    | HIGH     | 6, 12                                        | Mobile       |
| M01 | Email resend availability drives an automatic resend loop                           | MEDIUM   | 25                                           | Mobile       |
| M02 | Add-account flow retains password-only login mode                                   | MEDIUM   | 7, 23                                        | Mobile       |
| M03 | Scoped credentials and socket tokens stay stale                                     | MEDIUM   | 44, 49                                       | Mobile       |
| M04 | Public profile page resolves an unregistered proxy                                  | MEDIUM   | 40                                           | Mobile       |
| M05 | Local storage isolation is optional rather than enforced                            | MEDIUM   | 16, 36, 75                                   | Mobile       |
| M06 | ObjectBox writes can duplicate logical rows; queries lack deterministic cleanup     | MEDIUM   | 36, 37, 75, 80                               | Mobile       |
| M07 | Favourites are disconnected snapshots                                               | MEDIUM   | 59                                           | Mobile       |
| M08 | Pagination errors are absent from the feed error surface                            | MEDIUM   | 62                                           | Mobile       |
| M09 | Socket rebinding cancels the wrong handles; feed events ignore filters              | MEDIUM   | 63                                           | Mobile       |
| M10 | Invalidation parsing and event enrichment are inconsistent                          | MEDIUM   | 50                                           | Both         |
| M11 | Profile editor compares different types and ignores bio validity                    | MEDIUM   | 41                                           | Mobile       |
| M12 | Current-user presentation has multiple unsynchronized sources                       | MEDIUM   | 24                                           | Mobile       |
| M13 | Settings writes race and overwrite unknown preference keys                          | MEDIUM   | 42, 72                                       | Both         |
| M14 | Account-scoped managers redundantly track global account selection                  | MEDIUM   | 18, 24, 26                                   | Mobile       |
| M15 | HTTP failure paths can hang, misclassify, or leak credentials to external origins   | MEDIUM   | 8, 9, 11                                     | Mobile       |
| M16 | Push payloads, channels, and tap lifecycle disagree                                 | MEDIUM   | 22, 45, 79                                   | Mobile       |
| M17 | Background-audio support is declared but not established                            | MEDIUM   | 76                                           | Mobile       |
| M18 | Production can select a successful silent transport; recovery stores a bearer token | MEDIUM   | 3, 14, 16, 75                                | Mobile       |
| M19 | Routing discards identity and intended destinations                                 | MEDIUM   | 21, 22, 32                                   | Mobile       |
| M20 | Named navigation is used for a route without that name                              | MEDIUM   | 81                                           | Mobile       |
| M21 | Scope/connection orchestration is over-commanded                                    | MEDIUM   | 30, 31, 32, 34, 35                           | Mobile       |
| M22 | Shared infrastructure imports features and broad barrels hide the dependency graph  | MEDIUM   | 17, 82                                       | Mobile       |
| M23 | Production logging suppresses every severity                                        | MEDIUM   | 13                                           | Mobile       |
| M24 | Critical lifecycle tests are commented out                                          | MEDIUM   | 0, 20, 74, 83                                | Mobile       |
| M25 | Build/platform configuration is not release-ready                                   | MEDIUM   | 77                                           | Mobile       |
| M26 | UI presents incomplete behavior as operational                                      | MEDIUM   | 84                                           | Mobile       |
| L01 | Rebuild granularity is too broad for a one-second clock                             | LOW      | 85                                           | Mobile       |
| L02 | Derived observables are recreated by getters                                        | LOW      | 23, 24, 25                                   | Mobile       |
| L03 | Timer semantics and retry timing are inaccurate                                     | LOW      | 11, 29, 85                                   | Mobile       |
| L04 | Small UI/validation mismatches remain                                               | LOW      | 84                                           | Mobile       |
| L05 | Dead paths, fixtures, and documentation obscure the reference implementation        | LOW      | 35, 86                                       | Mobile       |
| L06 | Language setting is not connected to localization                                   | LOW      | 87                                           | Mobile       |
| I01 | The feed base mixes unused page arithmetic with cursor pagination                   | INFO     | 64                                           | Mobile       |
| I02 | Dependency risk is explicit, not proven obsolescence                                | INFO     | 0, 46, 80, 83                                | Mobile       |

Count: 5 CRITICAL + 25 HIGH + 26 MEDIUM + 6 LOW + 2 INFO = **64 findings, all mapped.** Steps 5, 75 and 80's upgrade part apply only if assumption A13 says an installed build exists; Steps 70, 71 and 73 are the Feature track.

---

## 5. Rules of the road

### 5.1 Simplicity rules

Read these before writing a single class. They are how the guide stays followable.

1. **Concrete classes, constructor injection.** Every new manager/service takes its collaborators as constructor parameters. No `di<X>()` inside a method body outside `locator.dart`, the app composition files, and small view-composition boundaries (design §4.4).
2. **No interface per class.** An interface exists only where there is a real second implementation today: `RoomTransport` (LiveKit vs silent fake) and `TokenBlocklist` (already exists on the backend). Everything else gets a concrete fake in `test/_support/`.
3. **No event bus, no generic framework, no new DI tier.** Commands are for user actions; plain `Future` methods are for internal sequencing (audit §4).
4. **Prefer fewer files.** One file may hold two or three tightly related small types (`AccountContext` + `RequestIdentity`; `CredentialRecord` + `VaultEnvelope` + `RememberedIdentity`). Never a file per field.
5. **Immutable data, mutable owners.** Models are `final` + freezed. Managers own their mutable state (`ValueNotifier`); nothing else writes it.
6. **One writer per fact.** Credentials: `SessionStore`. Account selection: `AccountTransitionOwner`. Live phase: `ActiveBroadcastManager`. Current-profile presentation: `MyProfileManager`. A second writer is a bug.
7. **Fence after every await.** Any `await` in account-scoped code is followed by a check that the generation/epoch/owner is still live before the next side effect. This is the single most important habit in the guide.
8. **Dispose everything you create**, in reverse creation order, idempotently. `finally` closes resources; server outcomes never gate local cleanup.
9. **No `BuildContext` across an await.** Capture what you need before the gap.
10. **Enums for payload-free sets, native sealed classes for outcomes that carry data.** `LivePhase`, `AccountPhase`, `RenewalState` are enums. `RefreshOutcome`, `TransitionResult`, `MutationResult` are native Dart 3 sealed classes with exhaustive `switch`. No code generation for either. No booleans standing in for three-way states.
11. **Keep the existing stack**: get_it, watch_it, command_it, listen_it (via `flutter_it`), go_router, ObjectBox, LiveKit, fpdart. `freezed` is already present; no other dependency is added by this guide.
12. **Simplify openly.** Where a step is simpler than the design document, its first line says `Simplified from design doc §X because ...; safety preserved because ...`.

### 5.2 Freezed policy

| Use                                                   | What                                                                                             | Why (one line each)                                                                                                                                                      |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Freezed**                                           | `CredentialRecord`, `LiveSnapshot`, `AccountSnapshot`                                            | The only places where `copyWith` earns its place. Value equality is what makes `select` comparisons trustworthy.                                                         |
| **Hand-written `final class`** with `==` / `hashCode` | `AccountContext`, `RequestIdentity`, `VaultEnvelope`, `RememberedIdentity`, `PendingDestination` | Small immutable values. A few lines of equality code is cheaper than a generated file.                                                                                   |
| **`enum`**                                            | `AccountPhase`, `LivePhase`, `RenewalState`, `LoginFormMode`                                     | Payload-free closed sets. Exhaustive `switch` works without codegen.                                                                                                     |
| **Native `sealed class`**                             | `RefreshOutcome`, `TransitionResult`, `MutationResult`                                           | Outcomes that carry data: the compiler forces every caller to handle rejection, uncertainty and cancellation.                                                            |
| **No**                                                | Services, managers, notifiers, widgets, anything with a lifecycle                                | Value-class generators have no `dispose` and hide ownership.                                                                                                             |
| **No** (hand-written decode)                          | Vault/secrets storage shapes                                                                     | Storage must _fail closed_: a missing or malformed field returns `null`/typed failure. Blind generated decoding throws deep inside the class instead of at the boundary. |

How to combine freezed with fail-closed decoding (Step 2 does exactly this):

- The class declares the shape (freezed for `CredentialRecord`; hand-written `final class` for `VaultEnvelope` and `RememberedIdentity`).
- A hand-written `static CredentialRecord? tryDecode(Map<String, dynamic>? json)` performs **every** field check itself and returns `null` on the first violation, constructing the value only after all checks pass.
- Generated `fromJson` is not used for these types; serialization is a hand-written `Map<String, dynamic> toJson()` on the same class.

### 5.3 Naming and folder conventions

| Kind                                      | Where it goes                                                                                                                                           | Example                                                    |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Immutable model / union                   | `lib/features/<feature>/model/` (or `lib/_routing/` for routing values)                                                                                 | `features/auth/model/account_context.dart`                 |
| Service (one external boundary)           | `lib/features/<feature>/services/`; only genuinely cross-feature services stay in `lib/_shared/services/`                                               | `features/auth/services/session_store.dart`                |
| Manager (semantic business logic)         | `lib/features/<feature>/manager/`                                                                                                                       | `features/broadcast/manager/active_broadcast_manager.dart` |
| App composition and lifecycle owners      | `lib/app/`                                                                                                                                              | `app/account_transition_owner.dart`                        |
| Widget                                    | `features/<feature>/pages/` (full screen) or `widgets/` (part of a screen)                                                                              | `features/profile/pages/user_profile_page.dart`            |
| Reusable widget with no feature knowledge | `lib/_shared/widgets/`                                                                                                                                  | `_shared/widgets/feed_widget.dart`                         |
| Test                                      | mirror the `lib` path under `test/`                                                                                                                     | `test/features/auth/session_store_test.dart`               |
| Test fake / clock                         | `test/_support/`                                                                                                                                        | `test/_support/fake_secure_storage.dart`                   |
| Rust module                               | `apps/api/src/modules/<module>/`: wire types in `dto.rs`, SQL in `repository/pg.rs`, rules in `service.rs`, HTTP in `handlers.rs`, WS in `routes/ws.rs` | `apps/api/src/modules/broadcast/service.rs`                |
| Rust test                                 | `apps/api/tests/<area>_<layer>.rs`                                                                                                                      | `apps/api/tests/broadcast_service.rs`                      |

Naming rules: class names say what they own (`AccountTransitionOwner`, not `AccountHelper`); no `Impl` suffix unless a real interface exists; no `_v2` suffix on classes — versions live in storage keys and schema fields.

### 5.4 Import direction

`lib/app` → `features` → `_shared` → `_core`. Never the reverse. `_shared` must not import `features` or `app` (audit M22). Feature-to-feature calls go through a small public file in that feature, never a broad barrel.

---

### 5.5 Delivery groups and recommended order

Several steps cannot ship alone (a publisher with no gate, a registration change without the page migration). Each group below is **one change**, with a short checklist per member step. Do not merge groups the guide does not tie together, and do not merge across a backend/mobile boundary. Step numbers stay as they are; the group says what lands together.

| Group                      | Steps                                                                                 | Why together                                                                                                                        |
| -------------------------- | ------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| G1 Vault                   | 2, 3, 4 and the absolute-expiry part of 6 (Step 5 is separate, decision-gated)        | The vault only works when they land together. After OE-2, Step 3 is only key names and typed failures.                              |
| G2 Identity-bound HTTP     | 8, 9, 11 (Step 10 lands in the same PR)                                               | They share the interceptor.                                                                                                         |
| G3 Small P1 fixes          | 12, 13, 14                                                                            | Optional: three short checklists in one PR if you prefer fewer PRs.                                                                 |
| G4 Gate and publisher      | 18, 19                                                                                | A publisher with no gate changes nothing.                                                                                           |
| G5 Router and pending slot | 21, 22                                                                                | A guard without the slot loses the destination it promised to keep. Platform adapters stay in Step 45.                              |
| G6 Session internals       | 28, 29                                                                                | Includes the single timer-manager rewrite (formerly also Step 85).                                                                  |
| G7 Switch to the manager   | 31, 32, 33, 34, 35                                                                    | Registration, page migration and deletion move together or the build breaks. Step 29 does not delete the old manager; Step 35 does. |
| G8 Local storage floor     | 36, 37                                                                                | Must land before Steps 38–43.                                                                                                       |
| G9 Raw socket              | 47, 48, 50                                                                            | Pure client and adapter rewrite; depends on P1 only, so it ships **before P3**. Only Step 49 needs the live manager.                |
| G10 Editor operation       | 57 with the mobile half of 58                                                         | Resolves the old 56 → 58 → 57 → 56 cycle.                                                                                           |
| G11 Feed base              | 60, 62, 64 (Step 61 only if there is no backend ended filter; Step 63 stays separate) | They change the same base file and tests.                                                                                           |
| G12 Backend auth           | 65, 67                                                                                | Same module and test file; they ship **right after P1**.                                                                            |
| G13 Release config         | 76, 77                                                                                | Same manifests; Step 77 depends on Step 76's decision.                                                                              |
| G14 Release checklist      | 74, 75, 79, 80                                                                        | Rows of one `docs/release-checklist.md`. Step 78 stays separate.                                                                    |
| G15 Structure cleanup      | 82, 86 (Step 81 rides along)                                                          | Files are moved, then documented.                                                                                                   |

**Document order follows this order (audit §8).** Steps 65 and 67 appear directly after Phase P1, and Steps 47, 48 and 50 appear directly before Phase P3. Steps 70, 71 and 73 appear in a separate **Feature track** section after Phase C.

| Order | Work                                                                                                 | Steps                                                       | Audit §8 |
| ----- | ---------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- | -------- |
| 1     | Test fakes, vault, bound HTTP identity, logout, refresh outcomes, validation parity, logging, config | 0, G1, G2, 10, 12–14, 16, 17                                | 1        |
| 2     | Backend revocation and reset ordering                                                                | G12 (65, 67); 66, 68, 69, 72 any time after                 | 2        |
| 3     | Account transition, readiness and remount                                                            | 15 (shrunk; skip if P1–P3 ship together), G4, 20, G5, 23–26 | 3        |
| 4     | Raw WebSocket client                                                                                 | G9 (47, 48, 50)                                             | 4        |
| 5     | Live teardown, then the account-owned manager                                                        | G6 (28, 29), 30, G7; then 44 and 49                         | 5, 6     |
| 6     | Broadcast and backend contracts                                                                      | 51–56, G10, 59                                              | 7        |
| 7     | Shared feeds                                                                                         | G11, 61 only if needed, 63                                  | 8        |
| 8     | Shared identity and persistence                                                                      | G8, 38–43, 45, 46                                           | 9, 10    |
| 9     | Release and device validation                                                                        | G13, 78, G14                                                | 12       |
| 10    | Cleanup and reference quality                                                                        | G15, 83, 84, 87                                             | 13, 14   |
| Later | **Feature track:** inbox, token policy, client inbox, readiness gates                                | 70, 71, 73                                                  | 11       |

---

## 6. Phase P0 — Tooling and safety net

**What you will have when this phase is done:** a working `freezed`/`build_runner` loop you already have (used for three classes only), plus a fake secure store and a controllable clock, which every later phase uses to prove account behaviour without a device.

**How to know it's safe to ship:** `dart analyze` clean, generated files committed, and the two fakes tested. Nothing is shipped to users by this phase.

**What we are deliberately NOT doing here:** no dependency upgrades (audit I02 treats versions as a documented risk), and no change to the strict analyzer baseline (audit §5 item 10).

### Step 0: Tooling and safety net

**Phase:** P0
**Audit reference:** none (supporting step), needed by every freezed step and by §3 [M24] "Critical lifecycle tests are commented out"; dependency baseline per [I02] "Dependency risk is explicit, not proven obsolescence", evidence `apps/mobile/pubspec.yaml:38–39,55–62`; analyzer baseline per audit §5 item 10, evidence `apps/mobile/analysis_options.yaml:76–111`.
**First, check the environment (D-24).** Read `apps/mobile/pubspec.yaml` and `.gitignore` before anything else. Assumptions A2, A3 and A8 are unverified; if freezed is not 4.x, the Dart SDK differs, or generated files are git-ignored, correct those rows and adapt this step.
**Why (plain words):** The guide adds a few small immutable types and native sealed classes, and the account work is only provable with a fake secure store and a controllable clock. Neither the fakes nor the union conventions exist yet. This step makes everything after it testable without adding a dependency.

**Files**

| File (exact path from repo root)                     | Action        | Kind   | Required? | Notes                                                                                                                                          |
| ---------------------------------------------------- | ------------- | ------ | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/pubspec.yaml`                           | Modify        | config | Required  | Comment block only, recording the freezed/build_runner pins. No version bumps (I02 treats versions as a documented risk, not an upgrade task). |
| `apps/mobile/test/_support/fake_clock.dart`          | Create        | test   | Required  | `FakeClock` with `nowUtc()`, `monotonic()`, `advance(Duration)`.                                                                               |
| `apps/mobile/test/_support/fake_secure_storage.dart` | Create        | test   | Required  | In-memory `SecureStorage` with injectable failures (`failOnWrite`, `failOnRead`).                                                              |
| `apps/mobile/analysis_options.yaml`                  | Do not change | config | Required  | The strict baseline stays (audit §5 item 10).                                                                                                  |

**Pseudo-code**

```
// Conventions locked here, applied everywhere later.

Freezed usage
    USE installed freezed 4.x ONLY for CredentialRecord, LiveSnapshot, AccountSnapshot: @freezed on `abstract class X with _$X`
    USE `enum` for payload-free sets and native `sealed class` (no codegen) for outcomes with data   // section 5.2
    USE `const factory(...)` constructors, NEVER the project's `new(...)` shorthand
        // freezed does not generate the declaring-constructor form
    MIXED STYLE IS FINE: hand-written classes keep `new(this.x)`; freezed classes use `const factory`
    COMMIT generated files with the source change           // repo convention: *.freezed.dart is tracked
    RUN `dart run build_runner build --delete-conflicting-outputs` from apps/mobile after each model change

Clock seam
    abstract class Clock { DateTime nowUtc(); Duration monotonic(); }
    SystemClock      -> production; registered once in root composition
    FakeClock(start) -> frozen; advance(by) moves both readings       // tests assert expiry without sleeping

Secure store seam
    FakeSecureStorage(Clock) holding a key->value map
    write(key, value)  -> optional FAIL
    read(key)          -> optional FAIL
    delete(key)        -> optional FAIL
    NEVER print or assert on secret values in logs; keys only

Analyzer
    KEEP very_good_analysis as configured
    KEEP `dart analyze` clean after every step; never add a per-file `// ignore:` to pass
```

**Done when:**

- `cd apps/mobile && dart run build_runner build --delete-conflicting-outputs` exits 0 and changes nothing.
- `dart analyze` reports no new issues compared with the pre-step baseline you recorded.
- A test constructs `FakeSecureStorage`, round-trips one key, and can be told to fail that write.
- `FakeClock.advance` moves both readings by the requested duration.
- `pubspec.yaml` gained no dependency.

**Tests:** `apps/mobile/test/_support/fake_secure_storage_test.dart` (round-trip; forced write failure; forced read failure). `apps/mobile/test/_support/fake_clock_test.dart` (advance moves `nowUtc` and `monotonic` equally).

**Watch out for:** threading `Clock` through every existing constructor at once. Add it where a step needs it; only Step 8 registers it in root composition.

**Depends on:** nothing.

---

## 7. Phase P1 — Safety foundation

**Findings finished in this phase:** C01, C02, C03, H09, H10, H25, M15, M18 (mobile slice), M23, L03 (HTTP slice), plus the M05/H14 safety guards that must exist before accounts are switchable, and the H02 admission fence that keeps the old live tier from writing into DI during a transition.

**What you will have when this phase is done (3–5 lines):**

- One secure vault document (`auth.vault.v2`) written through a single serialized queue, with absolute expiries that never move on read.
- A logout that removes the account's usable credentials locally even when the server is unreachable, and an add-account mode that no longer hides the email field.
- Every HTTP request pinned to the account and generation that started it, with a token only ever attached to the approved API origin and every interceptor path completing exactly once.
- A refresh that is classified into success / rejection / unavailable / uncertainty, never a silent sign-out.
- Old build data deleted at startup (one small routine), and the legacy live tier fenced so it cannot overlap a transition.

**How to know it's safe to ship:** the P1 test list in §17 passes; `dart analyze` is clean; a manual run shows the app requiring a fresh login after upgrade and preserving drafts/settings; a request started as account A cannot be released into B in the interceptor test; killing the app during logout leaves either "logged out" or a single "could not sign out, retry" state — never a silently restorable session.

**Order inside the phase:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17. Delivery group **G1** (Steps 2, 3, 4 and the absolute-expiry part of 6) is one buildable change: the vault only works when they land together. Delivery group **G2** (Steps 8, 9, 11; Step 10 lands in the same PR) must land together because they share the interceptor. See §5.5.

**What we are deliberately NOT doing in P1:** no new DI tier, no `AccountTransitionOwner` yet (that is P2), no live-session redesign (P3), no feed fixes (Phase F), no backend changes (Phase B/U), no new packages. Audit §4 items left alone here: everything about live scopes and command pipelines.

---

### Step 1: Add the account identity value types

**Phase:** P1
**Audit reference:** §3 [C01] "Requests can cross account boundaries during replay", Recommended fix (capture account ID and session generation before dispatch); §3 [H01] "Authenticated UI is not synchronized with account-scope readiness", Recommended fix (publish transition/readiness and generation); audit §1.5 ("immutable account ID plus session generation"). Evidence `apps/mobile/lib/_shared/services/session_interceptor.dart:96–175`, `apps/mobile/lib/features/auth/manager/user_scope_manager.dart:56–78`.
**Why (plain words):** Today nothing in the app carries "which account, and which activation of that account, am I?" — so a late callback cannot tell that it is stale. These two types are the fence everything else uses. They are plain immutable values, so they cost nothing at runtime.

_Simplified from design doc §3 because `AccountPhase` is an enum with five values (not a freezed union with seven), `RequestIdentity` has four fields and no mutable flag, and `AccountSnapshot` moves to Step 18 where it is first used (D-14); safety preserved because the audit asks only for an account id plus generation and a readiness observable, and cancellation goes through the HTTP client's cancel token._

**Files**

| File (exact path from repo root)                           | Action | Kind   | Required? | Notes                                                                                                                                                      |
| ---------------------------------------------------------- | ------ | ------ | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/auth/model/account_context.dart` | Create | model  | Required  | Hand-written `final class AccountContext` (accountId + generation) and `final class RequestIdentity` in one file, each with `==` / `hashCode`. No freezed. |
| `apps/mobile/lib/features/auth/model/_model.dart`          | Modify | config | Required  | Export the new file from the feature's narrow barrel.                                                                                                      |

**Pseudo-code**

```
AccountContext (final class, value equality)
    accountId: Id                 // the validated server user id, never email/name/local row id
    generation: int               // in-memory counter, incremented per activation (owner keeps it, Step 18)
    // no tokens, no profile fields: this is an identity fence, not a session object

RequestIdentity (final class, value equality)
    accountId: Id
    generation: int
    epoch: String                 // one login lineage; a fresh login makes a new epoch
    revision: int                 // the token revision this request started with
    // requestId (correlation only, never identity) is passed beside it, not inside it
    // NO isCancelled flag: cancel through the HTTP client's cancel token (D-14)
```

`AccountSnapshot` and the `AccountPhase` enum are created in Step 18, the first step that uses them.

**Done when:**

- Two `AccountContext`s with the same id and generation compare equal; different generations do not.
- `RequestIdentity` has no mutable field.
- Nothing in the codebase writes to these types except constructors (`grep -n "accountId =" apps/mobile/lib` shows no new mutable copies).

**Tests:** no dedicated file. Equality is exercised through the transition-owner tests (Step 18) and the interceptor tests (Step 8), per the test-plan reduction (review OE-24).

**Watch out for:** putting a bearer token or a full user profile into `AccountContext`. That reintroduces the "credential as identity" problem the audit flags in C01 and duplicates `MyProfileManager` (M12).

**Depends on:** Step 0.

---

### Step 2: Define the vault storage shapes with fail-closed decoding

**Phase:** P1
**Delivery group:** G1 (Steps 2, 3, 4 and the absolute-expiry part of 6 land as one change).
**Audit reference:** §3 [C02] "Session-store mutations are neither atomic nor identity-conditional", Recommended fix (coherent, versioned credential record); §3 [C03] "Explicit logout can leave a usable session recoverable"; §3 [H10] "Stored access-token expiry moves forward on every read", Recommended fix (persist absolute `expires_at` in a versioned record). Evidence `apps/mobile/lib/_shared/services/session_store.dart:105–156,242–267`, `apps/mobile/lib/features/auth/model/dtos.dart:38–55`.
**Why (plain words):** The vault currently stores a whole `AuthResponseDto` per account, so the relative `expires_in` is re-interpreted on every read and one malformed field throws deep inside a getter. A single versioned document with absolute expiry can be validated once, at the boundary, and either accepted whole or rejected whole.

_Simplified from design doc §3 (persisted `nextGeneration`, `ActivePointer`, `transitionIntent`, `migrationState`, and a five-field renewal journal) because a generation only fences late async work inside one process and nothing survives a kill; safety preserved because after a kill mid-switch the vault holds exactly one valid active account id and restoring it is safe, and a crash mid-refresh is covered by a three-value `renewalState` that forces a re-login._

**Files**

| File (exact path from repo root)                                     | Action             | Kind   | Required? | Notes                                                                                                                                            |
| -------------------------------------------------------------------- | ------------------ | ------ | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `apps/mobile/lib/features/auth/model/credential_record.dart`         | Create             | model  | Required  | Freezed `CredentialRecord`; hand-written `final class VaultEnvelope` and `RememberedIdentity`; each with hand-written `tryDecode` / `toJson`.    |
| `apps/mobile/lib/features/auth/model/credential_record.freezed.dart` | Create (generated) | model  | Required  | Commit it. Only `CredentialRecord` generates.                                                                                                    |
| `apps/mobile/lib/features/auth/model/_model.dart`                    | Modify             | config | Required  | Export the new file.                                                                                                                             |
| `apps/mobile/lib/features/auth/model/dtos.dart`                      | Modify             | model  | Optional  | Only to delete the now-unused `UserCredentialDtoX.toDto` round trip; skip if other call sites still need it. Loses nothing except a dead method. |

**Pseudo-code**

```
RenewalState (enum): idle | inFlight | uncertain          // persisted BEFORE a refresh is dispatched

CredentialRecord (freezed)
    schemaVersion: int            // 1 for this record type
    accountId: Id
    epoch: String                 // new random value on every fresh login / verification
    revision: int                 // incremented on every committed token replacement
    accessToken: String
    refreshToken: String?         // optional: a session may be access-only
    accessExpiresAt: DateTime      // ABSOLUTE UTC. Never recomputed from a duration
    receivedAt: DateTime
    isVerified: bool              // server fact, not form validation
    renewalState: RenewalState     // a crash while inFlight is treated as uncertain on next start

VaultEnvelope (final class)
    schemaVersion: 2
    revision: int                 // monotonic committed mutation counter
    activeAccountId: Id?          // the ONLY persisted "active" fact; no generation, no pointer object
    orderedAccountIds: List<Id>   // last-used order, unique
    credentials: Map<String, CredentialRecord>      // key MUST equal record.accountId.toString()
    remembered: Map<String, RememberedIdentity>

RememberedIdentity (final class)
    accountId, displayName, avatarUri?, email?, lastUsedAt, requiresAuthentication
    // greeting metadata only; never a token, never proof of authentication

tryDecode(Map<String,dynamic>? json) -> CredentialRecord? / VaultEnvelope? / RememberedIdentity?
    IF json is null RETURN null
    CHECK schemaVersion is supported (reject unknown, do not "upgrade" silently)
    CHECK every required field present and of the right primitive type
    CHECK accountId parses AND map key equals accountId          // design §2 qualification: key/nested id equality
    CHECK accessExpiresAt is a UTC instant; missing/naive RETURN null   // H10: unknown expiry is never "fresh"
    CHECK epoch non-empty, revision >= 0
    IF any check fails RETURN null                               // fail closed: caller treats it as unusable
    RETURN constructed value

toJson()
    WRITE only already-validated primitives; no DTO objects, no relative durations
```

**Done when:**

- A round trip `envelope → toJson → tryDecode` returns an equal envelope.
- Removing `accessExpiresAt`, making it local-time, or making the map key differ from `accountId` each makes `tryDecode` return `null` (no throw).
- An envelope with `schemaVersion: 99` is rejected rather than partially read.
- No `json_serializable`-generated `fromJson` exists for these types.
- The envelope has no generation, pointer, transition-intent or migration field (`grep -n "nextGeneration\|transitionIntent\|migrationState\|ActivePointer" apps/mobile/lib` finds nothing).

**Tests:** `apps/mobile/test/features/auth/credential_record_test.dart` — (a) round-trip equality, (b) each of the four malformed variants returns `null` instead of throwing, (c) `RememberedIdentity` never serializes a token-shaped field (assert the map keys).

**Watch out for:** adding `@JsonSerializable` "for convenience". The generated decoder throws on bad input, which is exactly the failure mode C02/H10 want removed.

**Depends on:** Step 1.

---

### Step 3: Reduce `SecureStorage` to single-key operations and add the v2 key

**Phase:** P1
**Delivery group:** G1.
**Audit reference:** §3 [C02] "Session-store mutations are neither atomic nor identity-conditional", Recommended fix (serialize mutations; conditional replace/remove); §3 [C03] "Explicit logout can leave a usable session recoverable"; §3 [H10] "Stored access-token expiry moves forward on every read"; §3 [M18] "Production can select a successful silent transport; recovery stores a bearer token in preferences", Recommended fix (persist minimal recovery intent). Evidence `apps/mobile/lib/_shared/services/secure_storage.dart:40–47,62–67`, `apps/mobile/lib/_core/keys/storage_keys.dart`.
**Why (plain words):** `SecureStorage` decides _how_ to write several keys and reports nothing back, so a failed write is indistinguishable from a successful one. This step reduces it to single-key read/write/delete where a failed write **throws a typed `StorageFailure`**, and reserves the new canonical key. The wrapper keeps no knowledge of what an "account layout" is.

_Simplified from design doc §3 (read-back confirmation, a lying-keystore test double, a global storage gate) because read-back doubles keystore I/O on every token refresh and defends against a platform that falsely reports success, which is not an audit finding; safety preserved because the vault queue writes first and updates memory only after the write succeeds, so a failed write leaves memory unchanged and consistent (C02), and local removal happens regardless of server outcome (C03)._

**Files**

| File (exact path from repo root)                       | Action | Kind    | Required? | Notes                                                                                                                                 |
| ------------------------------------------------------ | ------ | ------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/_shared/services/secure_storage.dart` | Modify | service | Required  | Single-key `read`/`write`/`delete`; a platform exception becomes a typed `StorageFailure`; delete every multi-key/auth-layout method. |
| `apps/mobile/lib/_core/keys/storage_keys.dart`         | Modify | config  | Required  | Add `vaultV2`; mark every legacy auth key `@legacy`. (No migration marker key: Step 5.)                                               |

**Pseudo-code**

```
StorageKeys
    ADD vaultV2 = 'auth.vault.v2'
    KEEP legacy keys as constants marked @legacy, for the startup cleanup only (Step 5)
        // userId, accessToken, refreshToken, sessionExpiry, credential, accounts
    KEEP non-auth keys untouched (fcm token stays device-scoped, design §7)

SecureStorage
    write(key, value)  -> AWAIT platform.write(key, value); a thrown error becomes StorageFailure
    read(key) / delete(key)                         // unchanged, single key, typed failure on throw
REMOVE any auth-layout orchestration from this wrapper
    // the vault's shape belongs to SessionStore; the wrapper only moves strings
```

**Done when:**

- `grep -n "writeAll\|writeActive\|accounts" apps/mobile/lib/_shared/services/secure_storage.dart` finds no layout logic.
- A platform write that throws surfaces as `StorageFailure`, not a raw platform exception.
- Every legacy key constant is annotated `@legacy` and referenced only from the startup cleanup.
- `dart analyze` clean.

**Tests:** none beyond `fake_secure_storage_test.dart` from Step 0 (a failing write reaches the caller as a typed failure). Durable-write behaviour on a real device is one line in `docs/release-checklist.md` (Step 74).

**Watch out for:** catching the failure inside the wrapper and returning `false`/`null`. The caller must see a typed failure so the vault can leave memory unchanged.

**Depends on:** Step 0.

---

### Step 4: Rewrite `SessionStore` as one serialized, conditional vault

**Phase:** P1
**Delivery group:** G1.
**Audit reference:** §3 [C02] "Session-store mutations are neither atomic nor identity-conditional", Recommended fix (serialize mutations and check expected account/generation _inside_ the serialized operation; conditional replace/remove); §3 [H10] "Stored access-token expiry moves forward on every read". Evidence `apps/mobile/lib/_shared/services/session_store.dart:105–156,242–267`.
**Why (plain words):** Concurrent renewal, switch and logout interleave read–modify–write today, so one can lose another's rotated token or clear the wrong account. One FIFO queue around one in-memory envelope makes every mutation ordered and lets "only if this is still account A at epoch E revision R" be evaluated where nothing can change underneath it.

_Simplified from design doc §3 and §12 (reserved/persisted generation, transition-intent marker, read-back, `_storageOk` gate with `recover()`) because those protect nothing the audit asks for; safety preserved because the queue still checks the expected account/epoch/revision before every write and memory is updated only after the write succeeds. A thrown write becomes a typed `StorageFailure` and memory is unchanged._

**This step defines the complete final API.** Every later step calls only the methods listed here (D-8).

**Files**

| File (exact path from repo root)                            | Action                                                                  | Kind    | Required? | Notes                                                                                         |
| ----------------------------------------------------------- | ----------------------------------------------------------------------- | ------- | --------- | --------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/auth/services/session_store.dart` | Move (`git mv` from `_shared/services/session_store.dart`) then rewrite | service | Required  | Same class name; new body. Update every import in the same step. **No re-export shim** (D-7). |
| `apps/mobile/test/features/auth/session_store_test.dart`    | Create                                                                  | test    | Required  | The CAS/queue suite.                                                                          |

**Pseudo-code**

```
SessionStore (root-owned, single writer)
    _queue      : FIFO tail of Futures        // one mutation at a time
    _envelope   : VaultEnvelope?              // committed in-memory truth
    _isClosing  : bool

FUNCTION mutate(expected, apply) -> MutationResult      // MutationResult is a native sealed class
    ENQUEUE on _queue; a rejected previous tail must not poison the queue
    REQUIRE initialised AND NOT _isClosing
    COMPARE expected (accountId, epoch, revision) against _envelope INSIDE the queue
    IF stale RETURN stale                     // no write, no event
    next = apply(_envelope)                   // pure function: never touches storage
    VALIDATE next: key==record.accountId, active id resolves, versions supported
    TRY AWAIT storage.write(vaultV2, next.toJson())
    CATCH StorageFailure RETURN storageFailed // memory unchanged; caller shows one retry state
    _envelope = next                          // memory updated only AFTER a successful write
    EMIT one immutable projection change
    RETURN committed

COMPLETE API (nothing else exists)
    initialize()                              // read + tryDecode; unusable record => treated as signed out
    readSnapshot() -> VaultSnapshot           // synchronous after init: accounts, activeAccountId, remembered
    usableCredential(accountId) -> CredentialRecord?   // null if missing, expired beyond recovery, or renewalState uncertain
    adoptCandidate(CredentialRecord)          // MUTATE if accountId absent or epoch differs: store record
    activate(accountId)                       // MUTATE: set activeAccountId (owner calls it when scope is ready)
    deactivate()                              // switch / add-account: clear activeAccountId, RETAIN credentials + remembered
                                              // not logout; must not be logged as one
    conditionalReplace(accountId, epoch, fromRevision, nextTokens, nextExpiry)
                                              // MUTATE if current record matches all three: replace tokens+expiry, revision+1, renewalState=idle
    markRenewalInFlight(accountId, epoch, revision)   // persist renewalState=inFlight BEFORE a refresh is dispatched (Step 10)
    settleRenewal(accountId, epoch, revision, outcome)// apply a refresh outcome inside the queue (Step 10)
    removeCredentials(accountId, epoch)       // explicit logout: delete the record, clear a matching activeAccountId,
                                              // mark remembered.requiresAuthentication; greeting metadata survives, tokens do not
    updateRememberedIdentity(accountId, safeDisplayFields)   // never tokens
    close()                                   // _isClosing = true; AWAIT queue tail; cancel projections
```

**Done when:**

- Interleaving `adoptCandidate`, `conditionalReplace`, `deactivate` and `removeCredentials` in any order never leaves two different token pairs for one account.
- `conditionalReplace` with a stale revision returns `stale` and writes nothing.
- A failing write returns `storageFailed`, leaves the in-memory envelope exactly as it was, and the next mutation works normally (no global gate, no `recover()`).
- `readSnapshot()` is synchronous after initialisation (no `await` in the hot path).
- No method takes a "current account" default — every one names the account it mutates.
- No `writeAndConfirm`, `_storageOk`, `recover()`, `reserveGeneration`, `commitActive`, `recordTransitionIntent`, `clearActivePointer` or `credentialRevisions` exists.

**Tests:** `apps/mobile/test/features/auth/session_store_test.dart` — (a) two concurrent `conditionalReplace` calls: one commits, one returns `stale`; (b) `removeCredentials` during a queued `conditionalReplace` leaves no record; (c) a failed write returns `storageFailed`, memory is unchanged and the next mutation succeeds; (d) projection emits exactly once per committed mutation.

**Watch out for:** awaiting anything slow (network, a transition) inside the queue. The vault queue must never wait on the transition queue, per design invariant I12.

**Depends on:** Steps 2, 3.

---

### Step 5: Delete legacy auth data once at startup (decision-gated)

**Phase:** P1
**Audit reference:** §3 [C03] "Explicit logout can leave a usable session recoverable", Recommended fix (remove captured credentials regardless of server outcome); §3 [H10] "Stored access-token expiry moves forward on every read"; §3 [M18] "recovery stores a bearer token in preferences". Evidence `apps/mobile/lib/_shared/services/session_store.dart:132–186,205–267`.
**Decision needed (assumption A13).** Ask the developer: _"Is there any build with real users, or only your own devices?"_ This guide assumes **some installed builds exist** and implements the small routine below. If there are **none**, delete this step entirely (and the legacy halves of Steps 16, 37, 38 and Steps 75, 80's upgrade part) and reinstall on dev devices.
**Why (plain words):** Legacy builds wrote split active keys plus a JSON vault of whole auth responses, with no trustworthy expiry and no record of an explicit logout. Reinterpreting those as usable sessions is how a logged-out account comes back. So the app does not read them: it deletes them once at startup and starts signed out. Drafts are kept.

_Simplified from design doc §7 and §12 (migration marker key, `cleanupPending` list, read-back, journal stages, resume logic, a fail-closed migration matrix) because the audit asks only that old credentials never become usable again; safety preserved because the old keys are deleted without being decoded, the vault is never seeded from them, and a failed delete is simply retried on the next launch._

**Files**

| File (exact path from repo root)                             | Action | Kind    | Required? | Notes                                                                                                                                                                                            |
| ------------------------------------------------------------ | ------ | ------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `apps/mobile/lib/features/auth/services/legacy_cleanup.dart` | Create | service | Required  | One function `deleteLegacyAuthData(SecureStorage, Preferences)`, about 15 lines. Steps 16 and 38 add their deletions (live-session token records, `profile.public_*` keys) to this same routine. |
| `apps/mobile/lib/features/auth/services/session_store.dart`  | Modify | service | Required  | `initialize()` calls the cleanup before reading `vaultV2`.                                                                                                                                       |

**Pseudo-code**

```
FUNCTION deleteLegacyAuthData(storage, prefs)
    FOR each of the six legacy auth keys (@legacy constants, Step 3)
        TRY AWAIT storage.delete(key) CATCH StorageFailure -> log the key CATEGORY only; continue
    FOR each legacy recovery-token preference key
        TRY AWAIT prefs.remove(key) CATCH -> log category only; continue
    // never decode, never copy, never "keep the active account because the keys look consistent"
    // a failed delete is retried next launch; the vault never reads these keys
    // no marker key, no cleanupPending list, no read-back, no resume logic

store.initialize()
    AWAIT deleteLegacyAuthData(...)
    read vaultV2; absent or unusable -> start signed out
```

**Done when:**

- With all six legacy keys populated, startup deletes them, `credentials` is empty and the app starts signed out.
- Drafts and settings are untouched.
- A delete that throws does not stop startup and is retried on the next launch.
- No code path decodes a legacy auth key.

**Tests:** one small test in `apps/mobile/test/features/auth/session_store_test.dart`: legacy keys populated → deleted, vault empty, signed out. (`vault_migration_test.dart` is dropped, review OE-24.)

**Watch out for:** "helpfully" keeping the active account because its split keys look consistent. Audit C03 makes that unsafe: there is no logout tombstone, so the app cannot tell an intact session from a logged-out one.

**Depends on:** Step 4.

---

### Step 6: Convert relative lifetimes to absolute expiry once, at receipt

**Phase:** P1
**Delivery group:** G1 (the absolute-expiry part lands with Steps 2–4; the record shape is the same one H10 needs).
**Audit reference:** §3 [H10] "Stored access-token expiry moves forward on every read", Recommended fix (convert relative lifetime once at API receipt; test with an injected clock); §3 [H25] "Client validation rejects server-valid accounts and can break credential storage", part "separate wire/storage mapping". Evidence `apps/mobile/lib/features/auth/model/dtos.dart:38–55,79–82`.
**Why (plain words):** `UserCredentialDtoX.toDto` writes `expires_in` back out and `toDomain` re-adds it to the current time, so a three-day-old token looks fresh on every read. Receipt-time conversion makes the stored value a fixed instant and keeps proactive renewal honest.

**Files**

| File (exact path from repo root)                            | Action | Kind          | Required? | Notes                                                                   |
| ----------------------------------------------------------- | ------ | ------------- | --------- | ----------------------------------------------------------------------- |
| `apps/mobile/lib/features/auth/model/dtos.dart`             | Modify | freezed model | Required  | Delete the relative-expiry round trip; add `toCredentialRecord(Clock)`. |
| `apps/mobile/lib/features/auth/services/session_store.dart` | Modify | service       | Required  | Persist/read `accessExpiresAt` only.                                    |
| `apps/mobile/test/features/auth/auth_expiry_test.dart`      | Create | test          | Required  | Clock-driven expiry tests.                                              |

**Pseudo-code**

```
AuthHttpService success handler
    REQUIRE envelope parsed, access token non-empty, user id valid UUID
    REQUIRE expires_in is a positive, bounded number of seconds
    at = clock.nowUtc()                             // ONE reading, captured once
    record = CredentialRecord(
        accessExpiresAt: at + seconds(expires_in),  // the only place a duration becomes an instant
        receivedAt: at, ...)
    RETURN record                                   // never the DTO

Reading from storage
    RETURN stored accessExpiresAt verbatim
    NEVER add now() to anything
    MISSING or malformed expiry -> record unusable (tryDecode returned null)

Proactive refresh threshold
    needsRefresh = accessExpiresAt - now <= skewMargin      // skewMargin from Config, e.g. 60s
```

**Done when:**

- `grep -n "expiresIn" apps/mobile/lib` shows `expiresIn` only in the DTO and its receipt-time conversion.
- Advancing `FakeClock` past `accessExpiresAt` makes `needsRefresh` true without any storage round trip.
- Repeated reads of a stored record return an unchanged `accessExpiresAt`.

**Tests:** `apps/mobile/test/features/auth/auth_expiry_test.dart` — (a) receipt at T stores T+expires, (b) read at T+2 days returns the same instant, (c) crossing the threshold flips `needsRefresh`, (d) a record without expiry is unusable.

**Watch out for:** a "helpful" fallback that treats a missing expiry as "long-lived". Missing means unusable.

**Depends on:** Steps 2, 4.

---

### Step 7: Make logout durable and separate it from deactivate

**Phase:** P1
**Audit reference:** §3 [C03] "Explicit logout can leave a usable session recoverable", Recommended fix (distinguish deactivate/add-account from logout; remove the captured account's usable credentials locally regardless of server outcome); §3 [M02] "Add-account flow retains password-only login mode", Recommended fix (explicit "use another account" mode independent of greeting metadata). Evidence `apps/mobile/lib/features/auth/manager/auth_manager.dart:243–245,281–314`, `apps/mobile/lib/_shared/services/session_store.dart:132–156`.
**Why (plain words):** Logout currently clears only the active keys, so the vault copy stays selectable and an in-flight renewal can write it back. Add-account clears the active state but keeps the last-known user, which hides the email field so a second account cannot be entered. Both are decisions about _which_ state to keep, so they are fixed together.

**Files**

| File (exact path from repo root)                                  | Action | Kind    | Required? | Notes                                                                                                       |
| ----------------------------------------------------------------- | ------ | ------- | --------- | ----------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/auth/model/login_form_mode.dart`        | Create | enum    | Required  | `fullEmail`, `prefillSelectedIdentity`; replaces "is a user remembered?" inference.                         |
| `apps/mobile/lib/features/auth/manager/auth_manager.dart`         | Modify | manager | Required  | `signOut()` calls `removeCredentials` first; `addAccount()` sets `fullEmail` and clears the password field. |
| `apps/mobile/lib/features/auth/pages/login_page.dart`             | Modify | widget  | Required  | Renders the email field from the mode, not from `lastUser != null`.                                         |
| `apps/mobile/lib/features/auth/widgets/switch_account_sheet.dart` | Modify | widget  | Required  | Offers "Use another account" as an explicit row.                                                            |

**Pseudo-code**

```
FUNCTION logout(capturedContext)
    CAPTURE the access token from the record BEFORE it is removed (needed only for the optional server call)
    result = AWAIT store.removeCredentials(capturedContext.accountId, capturedContext.epoch)
    IF result == storageFailed
        SHOW one state: "could not sign out, retry"      // honest: restart-safe logout not achieved
        STILL close local resources below                 // never leave media/connection running
    ELSE
        localLogoutComplete = true
    AWAIT closeResources(capturedContext)                 // broadcast exit (Step 15), connection (P4), scope pop (P2)
    IF localLogoutComplete
        TRY plain server logout call with the captured token, once, no refresh   // failure changes the message only
        PUBLISH signedOut with "remote revocation unconfirmed" when it failed
    // a late refresh writing the removed epoch must be impossible: epoch is gone from the envelope

FUNCTION addAccount()
    AWAIT deactivate(capturedContext)                     // clears active pointer, KEEPS other sessions
    formMode = LoginFormMode.fullEmail                    // explicit, not inferred from remembered user
    clearSubmittedPassword(); clearSelectedIdentity()
    KEEP remembered greeting list for display only
    // cancelling add-account does NOT silently reactivate the previous account
```

**Done when:**

- After logout with the server unreachable, `store.readSnapshot().credentials` has no entry for that account.
- A pending `conditionalReplace` for the removed epoch returns `stale` after logout (no record recreated).
- Add-account shows an editable, empty email field even when a user was remembered seconds earlier.
- `LoginFormMode` is the only input to the login page's field visibility (`grep -n "lastUser" apps/mobile/lib/features/auth` finds no UI decision).

**Tests:** `apps/mobile/test/features/auth/auth_manager_test.dart` — (a) logout removes credentials with a failing server call, (b) late renewal after logout cannot recreate the record, (c) add-account switches to `fullEmail` and keeps other accounts selectable, (d) a failed removal write (`storageFailed`) shows the retry state instead of success and still closes local resources.

**Watch out for:** calling the server first "so it looks clean". Local durable removal must happen before any optional network call.

**Depends on:** Step 4.

---

### Step 8: Bind every request to the identity that started it

**Phase:** P1
**Delivery group:** G2 (Steps 8, 9 and 11 share the interceptor; Step 10 lands in the same PR).
**Audit reference:** §3 [C01] "Requests can cross account boundaries during replay", Recommended fix (capture account ID and session generation before dispatch, bind refresh/retry to that identity, cancel or reject obsolete operations); §3 [M15] "HTTP failure paths can hang, misclassify, or leak credentials to external origins", Recommended fix (attach tokens only to the approved API origin). Evidence `apps/mobile/lib/_shared/services/session_interceptor.dart:96–124,140–175`, `apps/mobile/lib/_shared/services/http_client.dart:387–406`.
**Why (plain words):** The interceptor looks up "the current credential" when a 401 arrives, so a request started as A is replayed as B after a switch. It also attaches a bearer token to any URL it is handed, so an external download URL would receive the account's token. Both are fixed by deciding identity _before_ the first await and by never substituting another identity later.

**Files**

| File (exact path from repo root)                                  | Action                                                                        | Kind    | Required? | Notes                                                                                                                                                  |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------------- | ------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `apps/mobile/lib/features/auth/services/session_interceptor.dart` | Move (`git mv` from `_shared/services/session_interceptor.dart`) then rewrite | service | Required  | Identity-pinned policy; constructor takes `SessionStore`, `Clock`, approved origin. Update every import in the same step; **no re-export shim** (D-7). |
| `apps/mobile/lib/_shared/services/http_client.dart`               | Modify                                                                        | service | Required  | Transport only: no `SessionStore` knowledge, injectable interceptor policy, explicit `authMode`, `followRedirects` off on the API client.              |
| `apps/mobile/lib/_core/config/config.dart`                        | Modify                                                                        | config  | Required  | Expose the normalised approved API origin (scheme + host + effective port).                                                                            |
| `apps/mobile/test/features/auth/session_interceptor_test.dart`    | Create                                                                        | test    | Required  | The cross-account and cross-origin matrix.                                                                                                             |

**Pseudo-code**

```
RequestIdentity capture(req)
    RETURN RequestIdentity(accountId, generation of the current ready AccountContext, epoch, current token revision) plus a new requestId for correlation
    // captured BEFORE the first await; never re-read later

FUNCTION approvedOrigin(uri)
    n = normalise(uri.scheme, uri.host, effectivePort(uri))
    REQUIRE n == config.apiOrigin      // HTTPS required in production; loopback allowed only in dev
    RETURN true/false

FUNCTION dispatch(req, authMode)
    REQUIRE authMode is explicit: anonymous | account | bearer(token)   // never inferred from a path string
    IF authMode == anonymous
        ATTACH nothing; if the caller supplied a bearer, strip it
        RETURN AWAIT transport.send(req)
    IF authMode == bearer(token)
        // used only by logout: send EXACTLY this captured token; approved origin only; no refresh, no replay
        REQUIRE approvedOrigin(req.uri); ATTACH token; RETURN AWAIT transport.send(req)
    identity = capture(req)
    IF NOT approvedOrigin(req.uri)
        REJECT typed WrongOrigin                         // M15: no credential leaves the API origin
    cred = AWAIT store.usableCredential(identity.account)   // by captured id, never by "active"
    CHECK identity still live in this epoch/generation      // A→B switch already makes this false
    IF cred is null or expired beyond recovery
        REJECT typed AuthUnavailable (reauthentication required)   // NEVER send anonymously
    ATTACH cred.accessToken
    response = AWAIT transport.send(req)
    CHECK identity still live before decode or publication
    RETURN decode(response)

Redirects
    followRedirects = false on the API client    // audit M15 needs only "token to the approved origin"
    a 3xx is returned to the caller as a typed failure; NEVER forward Authorization anywhere else
```

**Done when:**

- A request captured as account A and released after a switch to B sends no B credential and returns typed cancellation.
- A request to `https://example.com/file` in `account` mode is rejected before any bytes leave.
- An anonymous auth call runs with no `Authorization` header even if a session exists.
- `http_client.dart` contains no reference to `SessionStore` (`grep -n SessionStore` finds none).

**Tests:** `apps/mobile/test/features/auth/session_interceptor_test.dart` — (a) A→B switch during a held request produces no B token, (b) external origin rejected in account mode and stripped in anonymous mode, (c) expired credential yields `AuthUnavailable`, not an anonymous request, (d) a redirect is not followed and no `Authorization` header is forwarded.

**Watch out for:** resolving the credential by "current account" anywhere in the dispatch path. Only the captured `identity.account` may be used.

**Depends on:** Steps 4, 6.

_Simplified from design doc §5.4 (capturedExit capability, redirect re-validation) because logout only needs to pass the token it captured before removal; safety preserved because a captured token can reach only the approved origin and never triggers refresh or replay._

---

### Step 9: Complete every interceptor path exactly once

**Phase:** P1
**Delivery group:** G2.
**Audit reference:** §3 [M15] "HTTP failure paths can hang, misclassify, or leak credentials to external origins", Recommended fix (complete every handler exactly once, preserve replay failure/uncertainty, share auth dispatch semantics with downloads); §3 [C01] "Requests can cross account boundaries during replay". Evidence `apps/mobile/lib/_shared/services/session_interceptor.dart:96–150,166–175`, `apps/mobile/lib/_shared/services/http_client.dart:439–490`.
**Why (plain words):** A storage exception can escape the 401 handler without completing it, so the caller waits forever; and a replay transport failure is reported as the original 401, so "offline" looks like "bad credentials". A single completion guard makes the outcome total: pass, reject-with-this-reason, or replay — chosen once.

**Files**

| File (exact path from repo root)                                  | Action | Kind    | Required? | Notes                                                          |
| ----------------------------------------------------------------- | ------ | ------- | --------- | -------------------------------------------------------------- |
| `apps/mobile/lib/features/auth/services/session_interceptor.dart` | Modify | service | Required  | Add `_completeOnce` guard around every hook.                   |
| `apps/mobile/lib/_shared/services/http_client.dart`               | Modify | service | Required  | Route download 401 through the same policy as normal requests. |
| `apps/mobile/test/features/auth/session_interceptor_test.dart`    | Modify | test    | Required  | Completion matrix.                                             |

**Pseudo-code**

```
FUNCTION handleInvocation(hook)
    done = false
    decision = null
    TRY
        decision = compute(hook)                    // pass | resolve | reject(reason)
    CATCH storageFailure   -> decision = reject(StorageUnavailable)
    CATCH cancelled        -> decision = reject(Cancelled)
    CATCH refreshFailure   -> decision = reject(thatRefreshCategory)
    CATCH unexpected(e)    -> decision = reject(InternalFailure); reportSanitised(e)
    FINALLY
        IF decision is null -> decision = reject(InternalFailure)
        IF NOT done
            done = true
            COMPLETE handler with exactly that decision     // one call, from this stack frame
    // NEVER complete from a detached callback later

FUNCTION replayOnce(original, identity, priorError)
    REQUIRE server rejection was a definite auth refusal (not a transport error)
    REQUIRE body is replayable (no consumed stream) and no prior replay happened
    IF credential revision advanced USE the newer confirmed credential
    ELSE AWAIT renewOnce(identity)
    CHECK identity still live
    response = AWAIT dispatch(original, accountMode, same logicalKey)
    IF response is 401 REJECT ReauthenticationRequired        // never a second refresh
    IF transport failed REJECT thatTransportFailure           // NOT the original 401
    RETURN response

Download
    SAME 401 policy as ordinary requests (no separate branch)
    STAGE to an owner temp file; stale/cancelled caller removes the staged file
```

**Done when:**

- Every failure injection in the tests below produces exactly one terminal outcome and no timeout.
- A replay that hits a transport error surfaces the transport error, not `401`.
- `grep -n "download" apps/mobile/lib/_shared/services/http_client.dart` shows the same policy call as normal dispatch.
- No `Completer` in the interceptor can be left uncompleted (a debug assertion fires in tests if one is).

**Tests:** `apps/mobile/test/features/auth/session_interceptor_test.dart` — (a) storage read throws → `StorageUnavailable` once, (b) replay transport failure → transport error, (c) cancelled request during refresh → `Cancelled` once, (d) download 401 replays under the captured identity only.

**Watch out for:** completing the handler inside a `.then` on a fire-and-forget future. The completion must be in the `finally` of the same invocation.

**Depends on:** Step 8.

---

### Step 10: Classify refresh outcomes and never retry a possibly-consumed token

**Phase:** P1
**Audit reference:** §3 [H09] "Refresh failures destroy sessions, including ambiguous rotated-token outcomes", Recommended fix (distinguish definite rejection, temporary unavailability and uncertain-after-dispatch; do not blindly retry a single-use refresh token); §3 [C02] "Session-store mutations are neither atomic nor identity-conditional". Evidence `apps/mobile/lib/_shared/services/session_interceptor.dart:211–261`, `apps/api/src/modules/auth/token.rs:439–486`.
**Why (plain words):** Today every refresh failure collapses to `null` and signs the user out, so a flaky network logs people out; and because the backend rotates single-use refresh tokens _before_ its later fallible work, a lost response can leave a consumed token in the vault that later triggers session revocation. Four explicit outcomes plus a persisted three-value `renewalState` remove both failure modes.

_Simplified from design doc §5.5 (five outcomes, a persisted pending record carrying attempt id, revision and generation, a persistenceBlocked state, backend stored-outcome replay) because the audit names three categories (audit §3 H09); safety preserved because every uncertain outcome still requires re-login, a crash mid-refresh is detected by `renewalState == inFlight`, and the same refresh token is never sent twice._

**Files**

| File (exact path from repo root)                                  | Action | Kind    | Required? | Notes                                                           |
| ----------------------------------------------------------------- | ------ | ------- | --------- | --------------------------------------------------------------- |
| `apps/mobile/lib/features/auth/model/refresh_outcome.dart`        | Create | model   | Required  | Native `sealed class` with four cases. No freezed.              |
| `apps/mobile/lib/features/auth/services/session_interceptor.dart` | Modify | service | Required  | Classification + single-flight keyed by account/epoch/revision. |
| `apps/mobile/test/features/auth/refresh_outcome_test.dart`        | Create | test    | Required  | The outcome table.                                              |

**Pseudo-code**

```
RefreshOutcome (native sealed class)
    Succeeded(record)                       // complete, same-account pair
    Rejected                                // documented invalid/expired/revoked (distinct backend error codes, Step 66)
    Unavailable(retryAfter?)                // offline preflight, connect failure, documented 429/maintenance that did NOT rotate
    Uncertain(reason)                       // timeout, possible dispatch, malformed success, 5xx, id mismatch

FUNCTION renewOnce(identity) -> RefreshOutcome
    IF an in-flight attempt exists for (account, epoch, revision) RETURN join it     // single flight
    IF a newer revision is already committed RETURN Succeeded(that one)              // never spend an old token
    marked = AWAIT store.markRenewalInFlight(account, epoch, revision)   // persist renewalState=inFlight BEFORE dispatch
    IF marked is not committed RETURN Uncertain(dispatchNotPrepared)
    TRY
        raw = AWAIT anonTransport.post(/auth/refresh, capturedRefreshToken)   // no interceptor recursion
        outcome = classify(raw, identity)                      // status + body, no guessing
    CATCH network(e)
        outcome = offline(e) ? Unavailable : Uncertain(unknownDispatch)
    IF outcome is Succeeded
        mapped = mapAtReceipt(outcome.body, clock)              // Step 6 rules
        IF mapped.accountId != identity.accountId
            outcome = Uncertain(accountMismatch)                // never adopt another identity
    AWAIT store.settleRenewal(account, epoch, revision, outcome) // conditional, inside the vault queue
    REMOVE the in-flight entry only if it still names this attempt
    RETURN outcome

Vault settlement (inside the queue)
    Succeeded   -> conditionalReplace(tokens, absoluteExpiry, revision+1, renewalState=idle)
    Rejected    -> removeCredentials(for that epoch only), mark remembered needs-auth
    Unavailable -> renewalState=idle, KEEP pair                  // retryable later
    Uncertain   -> renewalState=uncertain; record is NOT returned by usableCredential

On startup: any record whose renewalState is inFlight is treated as uncertain (a crash mid-refresh forces a re-login)

UI effect
    Unavailable -> keep using the access token while it is still valid; requests fail with a typed error, retry later
    Uncertain   -> require reauthentication   (never a "retry refresh" button)
```

**Done when:**

- Ten concurrent 401s from the same revision cause exactly one refresh dispatch.
- A timeout after dispatch marks the record uncertain and never retries the same token.
- An offline `Unavailable` leaves the credential usable and the session intact.
- A refresh completing after logout writes nothing (`stale`).
- A record found `inFlight` at startup is not usable and requires a login.

**Tests:** `apps/mobile/test/features/auth/refresh_outcome_test.dart` — (a) full outcome table incl. unclassified 5xx → `Uncertain`, (b) one-dispatch guarantee under N callers, (c) unknown account in the response → `Uncertain`, (d) `Uncertain` or `inFlight`-at-startup record is not returned by `usableCredential`.

**Watch out for:** treating "503" as an outage by default. The backend rotates before later steps, so an unclassified non-success must be `Uncertain`.

**Depends on:** Steps 4, 9.

---

### Step 11: Fix retry timing and reuse the captured identity

**Phase:** P1
**Delivery group:** G2.
**Audit reference:** §3 [M15] "HTTP failure paths can hang, misclassify, or leak credentials to external origins", Recommended fix (share auth dispatch semantics; retries annotated with identity); §3 [L03] "Timer semantics and retry timing are inaccurate", Recommended fix (apply jitter before enforcing the server's minimum wait; HTTP part only). Evidence `apps/mobile/lib/_shared/services/http_client.dart:439–490,589–605`.
**Why (plain words):** Wrapper retries re-enter authentication without the originating identity, and the retry delay applies shortening jitter _after_ choosing the server's `Retry-After`, so the app can retry earlier than the server allowed. This step makes retries reuse the captured identity and never wait less than the server minimum.

_Simplified from design doc §5.4 (a foreground retry budget and a `RetryLater` outcome) because the audit asks only for jitter-before-minimum ordering and identity reuse (L03, M15); safety preserved because the delay can only be longer than the server minimum and a switch during the delay stops the retry._

**Files**

| File (exact path from repo root)                          | Action | Kind    | Required? | Notes                                                                     |
| --------------------------------------------------------- | ------ | ------- | --------- | ------------------------------------------------------------------------- |
| `apps/mobile/lib/_shared/services/http_client.dart`       | Modify | service | Required  | Retry loop takes the captured identity; `Retry-After` floor applied last. |
| `apps/mobile/test/_shared/services/http_client_test.dart` | Modify | test    | Required  | Retry matrix with `FakeClock`.                                            |

**Pseudo-code**

```
FUNCTION sendWithRetry(req, identity, policy)
    attempts = 0
    LOOP
        attempts += 1
        CHECK identity live before every attempt        // never recapture
        response = AWAIT dispatch(req, identity)
        IF response not retryable OR attempts >= 3 RETURN response
        wait = backoff(attempts) + jitter()             // jitter only ADDS
        IF response has Retry-After
            wait = max(wait, parsed(retryAfter, clock)) // server minimum wins (L03)
        AWAIT cancellableDelay(wait)
        CHECK identity live                             // a switch during the delay stops here

Default policy
    safe reads: retry up to 3 total attempts
    mutations: NO automatic retry unless the manager supplied a logical key (Step 58)
    never retry: refresh, certificate failure, cancellation, consumed stream, ambiguous mutation
```

**Done when:**

- A `Retry-After: 30` response is honoured for at least 30s under `FakeClock`, never less.
- Switching accounts during the delay produces no second dispatch.
- A mutation without a logical key is never retried.
- Bad-certificate failure is terminal and surfaces as a security failure, not a network outage.

**Tests:** `apps/mobile/test/_shared/services/http_client_test.dart` — (a) `Retry-After` floor, (b) no dispatch after generation exit mid-delay, (c) mutation retry only with a logical key, (d) certificate failure not retried.

**Watch out for:** applying jitter to the server value (it can only add, never subtract).

**Depends on:** Steps 8, 9.

---

### Step 12: Align client validation with the server and stop validating during serialization

**Phase:** P1
**Audit reference:** §3 [H25] "Client validation rejects server-valid accounts and can break credential storage", Recommended fix (shared accepted/rejected fixtures, aligned documented policies, separate wire/storage mapping, do not discover invalidity during serialization); evidence `apps/mobile/lib/_core/value_objects/email.dart:9–35`, `password.dart:67–84`, `text_value_objects.dart:17–18,52`, `apps/mobile/lib/_shared/model/dtos.dart:35–55`, `apps/api/src/modules/auth/validators.rs:128–270`.
**Why (plain words):** The client email regex, password length counting (code units vs Unicode scalars) and three-character name minimum disagree with the server. A server-valid account can therefore fail login, and a value that was fine on the wire can throw when it is written to the vault. One shared fixture file makes the policies provably equal, and mapping stops using form-policy constructors.

**Files**

| File (exact path from repo root)                              | Action | Kind          | Required? | Notes                                                                                                                                                                                                                                    |
| ------------------------------------------------------------- | ------ | ------------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `<fixtures>/auth-validation.json`                             | Create | config        | Required  | Language-neutral accepted/rejected cases, checked in. **`<fixtures>` is the folder where the repo already keeps files shared between apps (verify).** Create a new top-level folder only if none exists (D-22).                          |
| `apps/mobile/lib/_core/value_objects/email.dart`              | Modify | config        | Required  | Match the server's real accepted policy.                                                                                                                                                                                                 |
| `apps/mobile/lib/_core/value_objects/password.dart`           | Modify | config        | Required  | Match the server's length rule counted in Unicode scalars and any composition rule it really enforces. **Derive every number from `apps/api/src/modules/auth/validators.rs:128–270`; do not hard-code the values in this guide (A-10).** |
| `apps/mobile/lib/_core/value_objects/text_value_objects.dart` | Modify | config        | Required  | Full-name minimum taken from `validators.rs`, not from this guide.                                                                                                                                                                       |
| `apps/mobile/lib/_shared/model/dtos.dart`                     | Modify | freezed model | Required  | Server→domain mapping no longer runs through throwing form constructors.                                                                                                                                                                 |
| `apps/api/tests/auth_validators.rs`                           | Create | test          | Required  | Same fixture, Rust side.                                                                                                                                                                                                                 |

**Pseudo-code**

```
Fixture file (owned jointly with the API)
    { "accepted": [ {kind:"email", value:"a+b_c@example.co.uk"}, ... ],
      "rejected": [ {kind:"password", value:"short"}, ... ] }
    INCLUDE plus/underscore/hyphen emails, trailing garbage, the Unicode-scalar boundaries at BOTH ends of the server's password range,
            names at the server's minimum length, long passwords, whitespace/normalisation cases

Mobile validation
    isValidEmail(v)  -> same rule set the server documents; no "universal" regex
    isValidPassword(v) -> length in RUNES (not .length) within the range validators.rs defines; no rule the server lacks
    isValidFullName(v) -> trimmed runes >= the minimum validators.rs defines
    // READ apps/api/src/modules/auth/validators.rs:128-270 FIRST and copy every limit from it; the numbers in older drafts of this guide
    // (8-128, no symbol, name minimum 2) are examples, not authority
    Registration form shows these; LOGIN password check is presence-only

Server → domain mapping
    decode wire value as a raw String
    IF it is needed for future form submission, apply policy then; the mapping itself accepts server truth
    NEVER call getOrCrash / .value on a form value object inside toCredentialRecord
```

**Done when:**

- The mobile policy test and the Rust validator test both read the same JSON and both pass.
- Changing one accepted case in the fixture makes the other language's test fail (prove it once, then revert).
- A response containing a server-valid short name and a letters-only password (if the server accepts one) persists into a `CredentialRecord` without throwing.
- `grep -n "getOrCrash" apps/mobile/lib/_shared/model/dtos.dart` finds nothing.

**Tests:** `apps/mobile/test/_core/value_objects/validation_parity_test.dart` (fixture-driven accepted/rejected), `apps/api/tests/auth_validators.rs` (same fixture), `apps/mobile/test/_shared/model/dtos_test.dart` (server-valid values map without throwing).

**Watch out for:** tightening the password policy beyond what `validators.rs` enforces (for example requiring a symbol) because it feels safer. Client-only strictness breaks server-valid accounts.

**Depends on:** Steps 2, 6.

---

### Step 13: Report production failures safely and stop the blanket sign-out

**Phase:** P1
**Audit reference:** §3 [M23] "Production logging suppresses every severity", Recommended fix (explicit sanitised production warning/error policy and reporting sink; never log credentials or private content); §3 [M15] "HTTP failure paths can hang, misclassify, or leak credentials to external origins" and design doc §5 (identity-bearing failures; no blanket sign-out from any 401). This is a design-doc requirement, not part of the audit's C01 text (D-23). Evidence `apps/mobile/lib/_core/logging/meno_logger.dart:68,100–143`, `apps/mobile/lib/_core/exceptions/global_exception_handler.dart:13–39`.
**Why (plain words):** `_on == env.isDev` gates warnings, errors and fatal events, so a production lifecycle or storage failure leaves no trace. In the other direction, any 401 reaches a global handler that signs out "the current account" — which is exactly wrong once identity is per-request. Both are about who is allowed to react to a failure, so they change together.

**Files**

| File (exact path from repo root)                                 | Action | Kind    | Required? | Notes                                                        |
| ---------------------------------------------------------------- | ------ | ------- | --------- | ------------------------------------------------------------ |
| `apps/mobile/lib/_core/logging/meno_logger.dart`                 | Modify | service | Required  | Level policy by environment; allowlisted fields.             |
| `apps/mobile/lib/_core/exceptions/global_exception_handler.dart` | Modify | config  | Required  | Routes identity-bearing auth failures; no blanket `signOut`. |
| `apps/mobile/test/_core/logging/meno_logger_test.dart`           | Modify | test    | Required  | Redaction with sentinel secrets.                             |

**Pseudo-code**

```
MenoLogger policy
    production: info(counters only) ON, warning ON, error ON, fatal ON, debug OFF
    development: same + debug ON
    NEVER log: Authorization, access/refresh/media tokens, OTP, Google token, password,
               serialized vault envelope, private profile/message/note text, raw URLs with query
    BEFORE writing a transport failure
        replace concrete path segments with endpoint templates; strip query and fragment
    REDACT exception messages/stacks that embed request values
        // allowlist fields; do not rely on regex masking alone

Global error pipeline
    ON auth failure carrying a RequestIdentity
        IF identity no longer matches the live account -> DROP (stale, affects nobody)
        ELSE hand an identity-specific rejection to the account owner (P2 Step 18)
    ON login/verification credential error -> local form error, never global sign-out
    ONE reporter: no duplicate local + global toasts for the same error
```

**Done when:**

- A production-mode logger emits a warning and an error (test asserts the sink received them).
- Sentinel secrets (token, OTP, password, note body) placed in typed exceptions and URLs never appear in captured output at any level.
- A 401 with a stale identity triggers no sign-out and no navigation.
- `grep -n "signOut" apps/mobile/lib/_core/exceptions/global_exception_handler.dart` finds nothing.

**Tests:** `apps/mobile/test/_core/logging/meno_logger_test.dart` — (a) production emits warning/error/fatal, (b) sentinel secrets redacted at every level, (c) URL query stripped, (d) stale identity rejection is dropped.

**Watch out for:** adding an external crash-reporting SDK. Audit §M23/design §5.13 keep sanitised platform logging until a retention/consent decision exists.

**Depends on:** Steps 1, 8.

---

### Step 14: Make production configuration fail loudly instead of silently

**Phase:** P1
**Audit reference:** §3 [M18] "Production can select a successful silent transport; recovery stores a bearer token in preferences", Recommended fix (reject invalid production configuration; allow the silent transport only through explicit test/dev configuration); evidence `apps/mobile/lib/locator.dart:348–358`, `apps/mobile/lib/_core/config/config.dart:34–38`.
**Why (plain words):** An empty LiveKit room URL currently falls back to the fake transport, so a release can show "live" with no audio and no error. A configuration mistake must be a visible startup failure, and the fake must be reachable only by an explicit test/dev switch.

**Files**

| File (exact path from repo root)                 | Action | Kind   | Required? | Notes                                                                           |
| ------------------------------------------------ | ------ | ------ | --------- | ------------------------------------------------------------------------------- |
| `apps/mobile/lib/_core/config/config.dart`       | Modify | config | Required  | `validate()`: required URLs, scheme checks, no empty room config in production. |
| `apps/mobile/lib/locator.dart`                   | Modify | config | Required  | Transport selection reads an explicit `useFakeRoomTransport` flag.              |
| `apps/mobile/test/_core/config/config_test.dart` | Modify | test   | Required  | Validation matrix.                                                              |

**Pseudo-code**

```
Config.validate() -> Result
    CHECK apiUrl is absolute, HTTPS in production (loopback allowed in dev only)
    CHECK socketUrl absolute and ws/wss matching the API environment
    CHECK livekitUrl non-empty and wss:// in production
    CHECK production never sets useFakeRoomTransport
    IF any check fails RETURN typed ConfigurationFailure(which field, no secret values)
    // Startup turns this into a visible configuration error screen; never SilentRoomTransport success

Transport selection
    IF config.useFakeRoomTransport AND NOT config.isProduction -> SilentRoomTransport
    ELSE -> LiveKitRoomTransport(config)
    // no third branch that guesses
```

**Done when:**

- Production config with an empty LiveKit URL produces a visible configuration failure and no app frame.
- `useFakeRoomTransport: true` in production is rejected at validation.
- Development config with a loopback API origin still passes.

**Tests:** `apps/mobile/test/_core/config/config_test.dart` — (a) empty production room URL fails, (b) fake transport in production fails, (c) valid production config passes, (d) dev loopback passes.

**Watch out for:** logging the offending URL in full — query strings may carry tokens.

**Depends on:** Step 13 (for the sanitised reporting path).

---

### Step 15: Fence the old live tier so it cannot outlive a transition

**Phase:** P1
**Audit reference:** §3 [H02] "Live transitions can outlive account teardown or supersede newer sessions", Recommended fix (one account-owned transition owner; dispose/generation checks after awaits; quiesce before disposing resources — interim step); §3 [H04] "Failed end/leave skips session closure"; audit §1.4 ("repair teardown before restructuring"). Evidence `apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart:155–223,289–340`, `apps/mobile/lib/features/auth/manager/user_scope_manager.dart:87–123,139–142`.
**Why (plain words):** Account teardown and live-session opening use two independent queues, so a late `_open()` can push a scope into a stack that is being popped, and cleanup can borrow the next account's token. Until P3 replaces the tier, one admission flag plus an awaited close is enough to make overlapping accounts impossible.
**If P1–P3 ship together, skip this step** and do P3 right after P2.

_Simplified from design doc §4.3 (three idempotent exit layers, a fence plus a temporary adapter that Step 35 deletes, and the old Step 27) because this is a bridge that is thrown away; safety preserved because new opens are refused from the moment exit begins, the in-flight open is awaited and closed, and the transition blocks if that fails. Step 31 swaps the single call for `ActiveBroadcastManager.closeForAccountExit()`._

**Files**

| File (exact path from repo root)                                     | Action | Kind    | Required? | Notes                                                                                                  |
| -------------------------------------------------------------------- | ------ | ------- | --------- | ------------------------------------------------------------------------------------------------------ |
| `apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart` | Modify | manager | Required  | About 20 lines: an `_acceptingEntries` flag, `beginAccountExit()`, flag checked after every await.     |
| `apps/mobile/lib/features/auth/manager/user_scope_manager.dart`      | Modify | manager | Required  | Call `beginAccountExit()` directly before popping; block the transition if it fails. No adapter class. |
| `apps/mobile/test/features/broadcast/live_exit_fence_test.dart`      | Create | test    | Required  | Opening-vs-exit races.                                                                                 |

**Pseudo-code**

```
LiveScopeManager additions
    _acceptingEntries = true

FUNCTION beginAccountExit() -> Future<void>
    _acceptingEntries = false                      // rejects new opens from this instant
    CANCEL queued opens with typed Cancelled
    AWAIT the in-flight open (bounded, e.g. 10s)
    IF it completed, close what it created         // no half-open session is left behind
    IF still in flight at the deadline THROW       // the transition stays blocked; never overlap two accounts
    AWAIT _close() if a session exists

FUNCTION _open()
    REQUIRE _acceptingEntries BEFORE pushing anything
    ... existing work ...
    CHECK _acceptingEntries after EVERY await; if false -> close locally, RETURN Cancelled

UserScopeManager transition
    AWAIT liveScopeManager.beginAccountExit()      // Step 31 replaces this one call
    IF that throws -> PUBLISH blocked, do NOT pop the account scope
    THEN pop descendant scopes, then the account scope
```

**Done when:**

- Starting a live session and then logging out in the same tick leaves no registered live scope and no open transport.
- With an artificial 30s in-flight open, the transition blocks instead of creating the next account's scope.
- Draft/settings writes from the old account are unaffected.

**Tests:** `apps/mobile/test/features/broadcast/live_exit_fence_test.dart` — (a) exit during open closes locally, (b) failed drain blocks the transition, (c) no DI push after exit began.

**Watch out for:** treating "cancelled" as "closed". A cancelled open may still have created a transport; the drain must close whatever exists.

**Depends on:** Steps 7, 8.

---

### Step 16: Stop writing secrets to preferences and enforce owner-qualified local access

**Phase:** P1
**Audit reference:** §3 [M18] "Production can select a successful silent transport; recovery stores a bearer token in preferences", Recommended fix (persist minimal recovery intent); §3 [H14] "Viewer-specific profile cache state leaks across accounts", Recommended fix (viewer-keyed cache); §3 [M05] "Local storage isolation is optional rather than enforced", Recommended fix (owner-qualified reads/deletes/upserts and foreign-object rejection — the safety guard only, the owner-bound constructors arrive in Step 36). Evidence `apps/mobile/lib/features/broadcast/services/live_session_store.dart:54–63`, `apps/mobile/lib/features/profile/services/profile_cache_service.dart:43–51`, `apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart:20–46,100–104`.
**Why (plain words):** Old builds stored a LiveKit bearer token in preferences and cached `isFollowing` under a subject-only key. The _new_ code must stop doing both before switching is possible. Old data already on devices is deleted by the one startup routine from Step 5.

_Simplified from design doc §7 (a blocking scrub with a retry screen and a scrub-failure readiness gate) because the audit requires only that the new code stops writing secrets and that old ones are not read (M18, H14); safety preserved because legacy keys are deleted on sight, never read, and a failed delete is retried on the next launch. If assumption A13 finds no installed builds, drop the legacy deletions and keep only the new-code fixes below._

**Files**

| File (exact path from repo root)                                           | Action | Kind    | Required? | Notes                                                                                                                                                           |
| -------------------------------------------------------------------------- | ------ | ------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/services/live_session_store.dart`      | Modify | service | Required  | New record holds owner + broadcast id + role hint + savedAt only; never a token.                                                                                |
| `apps/mobile/lib/features/profile/services/profile_cache_service.dart`     | Modify | service | Required  | Key includes the viewer (`profile.v2/<viewerId>/<subjectId>`, Step 38); legacy `profile.public_<subjectId>` never read.                                         |
| `apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart` | Modify | service | Required  | Every draft read/delete/upsert requires an explicit `ownerId`; foreign objects rejected.                                                                        |
| `apps/mobile/lib/features/auth/services/legacy_cleanup.dart`               | Modify | service | Required  | Add: delete legacy `broadcast.live_session_<userId>` records and `profile.public_*` keys, in the same routine as Step 5. Skip if A13 finds no installed builds. |
| `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart`    | Create | test    | Required  | Owner-violation matrix (this file keeps the isolation tests; there is no separate `legacy_scrub_test.dart`).                                                    |

**Pseudo-code**

```
LiveSessionStore
    NEW record: { version, ownerId, broadcastId, intention, roleHint, savedAt, operationId? }
    NEVER persist a media token; recovery always fetches a fresh one

ProfileCacheService
    key = 'profile.v2/<viewerId>/<subjectId>'      // viewer-relative (H14); one key, whole response (Step 38)
    own profile: 'account.v2/<ownerId>/own-profile'
    malformed entry -> treat as MISS and remove that one key

BroadcastLocalService (safety guard now, owner-bound constructor later in Step 36)
    readDraft / deleteDraft / upsertDraft REQUIRE ownerId argument
    IF row.ownerId != ownerId -> REJECT typed OwnerViolation (never rewrite the owner)
    lists keep filtering by owner (already correct)

Legacy cleanup (Step 5 routine)
    delete legacy token-bearing live-session records for EVERY owner, and legacy profile.public_* keys
    a failed delete is logged by key CATEGORY only and retried next launch
```

**Done when:**

- The new live-session record contains no token field.
- A public-profile cache write/read without a viewer part is refused.
- A draft read for owner A cannot return owner B's row (typed rejection, not empty).
- If A13 says installed builds exist: a legacy record containing a token is deleted on startup, including for inactive owners.

**Tests:** `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart` — (a) foreign draft read rejected, (b) foreign upsert rejected, (c) viewer-less profile key refused.

**Watch out for:** deleting the whole local database. Drafts are user data; only token-bearing and viewer-less records are removed.

**Depends on:** Step 5 (or Step 4 if Step 5 was deleted under A13).

---

### Step 17: Move the session model out of `_shared` and fix the barrels

**Phase:** P1
**Audit reference:** §3 [M22] "Shared infrastructure imports features and broad barrels hide the dependency graph", Recommended fix (move auth persistence into auth; narrow imports); design §8 "Move/modify" rows for `session_store.dart`, `session_interceptor.dart`, `session.dart`. Evidence `apps/mobile/lib/_shared/_shared.dart:1–23`, `apps/mobile/lib/_shared/services/session_store.dart:7`.
**Why (plain words):** `_shared` currently imports `features/auth` and re-exports it through a barrel, so the claimed feature-independent bottom layer is not real. `session_store.dart` and `session_interceptor.dart` were already moved with `git mv` in Steps 4 and 8, so nothing old and new coexists (D-7). This step moves the last auth file and fixes the barrels so the graph is one-way before P2 adds more app-level code.

**Files**

| File (exact path from repo root)                                        | Action          | Kind   | Required? | Notes                                                                                       |
| ----------------------------------------------------------------------- | --------------- | ------ | --------- | ------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/_shared/model/session.dart`                            | Move (`git mv`) | model  | Required  | → `apps/mobile/lib/features/auth/model/session.dart`. Update every import in the same step. |
| `apps/mobile/lib/_shared/_shared.dart`                                  | Modify          | config | Required  | Stop exporting the moved files.                                                             |
| `apps/mobile/lib/features/auth/_services.dart`, `.../model/_model.dart` | Modify          | config | Required  | Export from the feature's own narrow barrels.                                               |
| `apps/mobile/test/locator_test.dart`                                    | Modify          | test   | Optional  | Only if it asserted the old import path.                                                    |

**Pseudo-code**

```
MOVE session.dart; update every import
    RUN `rg -l "model/session" apps/mobile/lib apps/mobile/test` and fix each
    // session_store.dart and session_interceptor.dart already moved in Steps 4 and 8: nothing else to move
NO re-export shim anywhere
SHARED barrel
    REMOVE the auth exports
    LEAVE genuinely shared exports alone
FEATURE barrels
    auth/_services.dart exports session_store + session_interceptor
    auth/model/_model.dart exports session + account_context + credential_record + refresh_outcome
    // account_snapshot is added by Step 18, where it is created
CHECK direction
    `rg -n "features/" apps/mobile/lib/_shared` -> only the known, planned exceptions still to be handled in Step 82
    Until Step 82 creates `tool/check_import_direction.sh`, use the `rg` command above; once it exists, run it by hand before merging (or from a git pre-push hook). If a CI system already exists (verify, A14), run it there too.
```

**Done when:**

- No file under `lib/_shared` imports `features/auth` for session types.
- `lib/_shared/_shared.dart` does not export anything from `features/`.
- No old copy of `session_store.dart` or `session_interceptor.dart` exists under `_shared`.
- `dart analyze` clean; the app boots and login/logout still work against a stub API.

**Tests:** run the existing `apps/mobile/test/locator_test.dart` and `apps/mobile/test/app_test.dart` unchanged; they fail if a registration or import moved incorrectly.

**Watch out for:** leaving an `export 'package:.../features/auth/...'` shim "to avoid touching call sites". That keeps the bad direction and hides it again.

**Depends on:** Steps 4, 8.

---

**Delivery group G12 — backend security fixes (Steps 65 and 67). They ship right after Phase P1 (audit §8 priority 2). The step text below uses the same numbers as everywhere else.**

### Step 65: Enforce account-wide revocation in the HTTP guard

**Phase:** U (Backend)
**Delivery group:** G12 (Steps 65 and 67 share a module and test file). **Ships right after Phase P1** (audit §8 priority 2).
**Audit reference:** §3 [C05] "Backend access-token revocation is not enforced by the actual HTTP guard", Recommended fix (validate both token-specific and user-wide revocation using the verified subject and issuance time; test through a protected router using production wiring); evidence `apps/api/src/modules/auth/token.rs:540–565,600–614`, `apps/api/src/modules/auth/cache/redis_store.rs:155–187`, `apps/api/src/middleware/auth.rs:281–290,456–459`, `apps/api/src/state.rs:314–321`.
**Why (plain words):** Logout-all and password reset write a user-wide invalidation marker, but the installed guard only asks "is this JTI revoked?", so existing access tokens keep working for the rest of their lifetime. The blocklist implementation already knows how to answer the user-wide question — the guard never asks it.

**Files**

| File (exact path from repo root)                  | Action | Kind        | Required? | Notes                                              |
| ------------------------------------------------- | ------ | ----------- | --------- | -------------------------------------------------- |
| `apps/api/src/middleware/auth.rs`                 | Modify | Rust module | Required  | Add the trait method; call it in `authenticate`.   |
| `apps/api/src/modules/auth/cache/redis_store.rs`  | Modify | Rust module | Required  | Delegate to the existing `is_user_tokens_blocked`. |
| `apps/api/src/modules/auth/cache/memory_store.rs` | Modify | Rust module | Required  | Same behaviour for in-memory wiring and tests.     |
| `apps/api/tests/auth_router.rs`                   | Modify | test        | Required  | Production-wiring guard test.                      |

**Pseudo-code**

```
trait TokenBlocklist
    async fn is_revoked(jti, expires_at) -> Result<bool>
    ADD async fn is_user_revoked(user_id, issued_at) -> Result<bool>          // the missing question

AuthState::authenticate(token)
    claims = verifier.verify(token)?                    // signature/expiry first (unchanged)
    IF blocklist.is_revoked(claims.jti, claims.exp)? -> Unauthorized(InvalidToken)
    ADD IF blocklist.is_user_revoked(claims.sub, claims.iat)? -> Unauthorized(InvalidToken)
        // verified subject + issuance time; a token issued AFTER the marker survives
    RETURN AuthUser::from_claims(claims)
    // a blocklist backend error stays an infrastructure error, NOT an auth failure
    // (same generic InvalidToken code for every rejection: no oracle)

RedisBlocklist::is_user_revoked(user_id, issued_at)
    RETURN self.is_user_tokens_blocked(user_id, issued_at)     // marker >= issued_at means revoked
MemoryBlocklist::is_user_revoked -> same semantics against the in-memory marker
```

**Done when:**

- `POST /auth/logout-all` followed by a request with the old access token returns `401 InvalidToken`.
- A token issued _after_ logout-all still works (the marker comparison is `<=`).
- A Redis/in-memory backend failure returns an infrastructure error, not a 401.
- The test drives the production guarded router, not a hand-built one.

**Tests:** `apps/api/tests/auth_router.rs` — (a) logout-all invalidates existing access tokens, (b) a fresh login survives, (c) blocklist failure is not an auth failure, (d) JTI-level revocation still works.

**Watch out for:** checking the user-wide marker before signature verification. That makes the blocklist a probe for arbitrary tokens.

**Mobile fallback until this lands:** Local logout remains authoritative and is described as "remote revocation unconfirmed"; the client never claims account-wide invalidation already took effect.

**Depends on:** nothing (no mobile dependency; it can ship the day P1 is done).

---

### Step 67: Revoke sessions only after reset ownership is proven

**Phase:** U (Backend)
**Delivery group:** G12. **Ships right after Phase P1.**
**Audit reference:** §3 [H21] "Password-reset requests revoke sessions before proving ownership", Recommended fix (revoke only after successful reset-code verification and password replacement; retain enumeration-resistant responses); evidence `apps/api/src/routes/auth.rs:95–110`, `apps/api/src/modules/auth/credentials.rs:168–189`.
**Why (plain words):** The public forgot-password endpoint revokes the target user's sessions before any code is checked, so knowing an email address is enough to disrupt that account — and Step 65 makes the disruption immediate. Moving revocation after verification removes the lever while keeping the response identical for unknown addresses.

**Files**

| File (exact path from repo root)           | Action | Kind        | Required? | Notes                                              |
| ------------------------------------------ | ------ | ----------- | --------- | -------------------------------------------------- |
| `apps/api/src/routes/auth.rs`              | Modify | Rust module | Required  | Forgot-password no longer revokes.                 |
| `apps/api/src/modules/auth/credentials.rs` | Modify | Rust module | Required  | Reset path revokes after the password is replaced. |
| `apps/api/tests/auth_router.rs`            | Modify | test        | Required  | Ordering proof.                                    |

**Pseudo-code**

```
FUNCTION forgot_password(email)                    // unauthenticated
    user = find_user(email)                        // may be None
    IF user exists SEND reset code
    RETURN the SAME response body and timing class in both cases   // no enumeration oracle
    // DELETE the block_all_user_tokens call that used to live here

FUNCTION reset_password(email, code, new_password)
    VERIFY code for that user (constant-time compare, attempt limits)
    REQUIRE valid ELSE generic invalid-code response
    HASH + store the new password
    THEN block_all_user_tokens(user_id)            // revocation follows ownership proof
    RETURN success
```

**Done when:**

- Calling forgot-password twice leaves existing access tokens valid.
- A successful reset invalidates them (after Step 65: immediately).
- An unknown email and a known email produce identical responses.
- A failed code attempt leaves sessions untouched.

**Tests:** `apps/api/tests/auth_router.rs` — (a) request does not revoke, (b) successful reset revokes, (c) identical responses for unknown email, (d) wrong code does not revoke.

**Watch out for:** Revoking on request to be safe. That is the vulnerability; revocation belongs after verification.

**Mobile fallback until this lands:** Requesting a reset is never treated as ownership proof, and the client does not log the current account out on a reset request.

**Depends on:** Step 65.

---

---

## 8. Phase P2 — Authoritative account lifecycle

**Findings finished here:** H01, M01, M02 (final), M12, M14, M19, L02 (auth projection slice), plus the M24 suite for the transition owner, and the second H02 bridge.

**What you will have when this phase is done (3–5 lines):**

- One owner (`AccountTransitionOwner`) with one FIFO queue that is the only code that pushes or pops the `authenticated` scope.
- One `AccountSnapshot` telling the UI exactly where the app is: signed out, preparing, ready, quiescing or blocked — so no private screen can build before its dependencies exist.
- An account subtree keyed by generation and remounted on every activation; the owner waits one frame for the old subtree to be gone before it pops the scope.
- A router that keeps one validated pending destination through login, verification and account switch, and refreshes on account identity rather than on unchanged booleans.
- A startup retry that actually resets and recreates failed root dependencies.

**How to know it's safe to ship:** the transition-owner tests (Steps 18–19) pass; A→B switching never shows A's data under B's chrome; a slow login cannot land after a logout; the app can be cold-started, retried after an induced failure, and killed mid-transition without mounting the wrong account.

**Order:** 18 → 19 → 20 → 21 → 22 → 23 → 24 → 25 → 26. (Step 27 was removed; see its stub.) Delivery group **G4** (Steps 18–19) must land together (a publisher with no gate would leave the UI unchanged). Delivery group **G5** (Steps 21–22) lands together (a guard without the pending slot loses the destination it promised to preserve).

**What we are deliberately NOT doing in P2:** no live-session redesign (P3), no feed or profile-data work (P4/F), no notification delivery (U), no localization, no persisted generation, no unmount hub, no bootstrap cleanup ledger, and no new "throwable scope" tier — the audit §4 ledger rejects one.

---

### Step 18: Replace `UserScopeManager` with one transition owner

**Phase:** P2
**Delivery group:** G4 (Steps 18 and 19 land as one change).
**Audit reference:** §3 [H01] "Authenticated UI is not synchronized with account-scope readiness", Recommended fix (publish account transition/readiness and generation; gate authenticated routes); §3 [M14] "Account-scoped managers redundantly track global account selection", Recommended fix (inject immutable account context; initialize once); audit §1.5 (account id plus generation, readiness observable). Evidence `apps/mobile/lib/features/auth/manager/user_scope_manager.dart:56–78,87–134`.
**Why (plain words):** The current manager passively follows `AuthManager`'s credential stream and swallows registration errors, so nothing tells the UI whether an account is usable. One owner with an explicit FIFO queue, a published snapshot and typed results replaces "hope the stream arrived".

_Simplified from design doc §5.2 (persisted generation reservation, interrupted-transition marker, an unmount-acknowledgement hub, an `exit channel`, a second attempt counter) because the audit asks for an in-process generation, a readiness observable, remount by generation and invalidation of in-flight work; safety preserved because a private in-memory generation fences late work, `AccountGate` shows a neutral page while quiescing, and the owner waits for that frame before popping the scope._

**Files**

| File (exact path from repo root)                                | Action                                                                | Kind    | Required? | Notes                                                                                                                                                                                      |
| --------------------------------------------------------------- | --------------------------------------------------------------------- | ------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `apps/mobile/lib/app/account_transition_owner.dart`             | Create (moved from `features/auth/manager/user_scope_manager.dart`)   | manager | Required  | Owns the queue, snapshot, scope handle, the private generation counter and the attempt counter.                                                                                            |
| `apps/mobile/lib/features/auth/model/account_snapshot.dart`     | Create                                                                | model   | Required  | `enum AccountPhase { signedOut, preparing, ready, quiescing, blocked }` and freezed `AccountSnapshot`; the single readiness observable. Created here because this is its first use (D-14). |
| `apps/mobile/lib/features/auth/model/_model.dart`               | Modify                                                                | config  | Required  | Export `account_snapshot.dart`.                                                                                                                                                            |
| `apps/mobile/lib/features/auth/manager/user_scope_manager.dart` | Delete                                                                | manager | Required  | After call sites move (same step).                                                                                                                                                         |
| `apps/mobile/lib/app/composition/account_registrations.dart`    | Create                                                                | config  | Required  | The registrar callback the owner invokes (grows in P3/P4).                                                                                                                                 |
| `apps/mobile/test/app/account_transition_owner_test.dart`       | Create (moved from `test/features/auth/user_scope_manager_test.dart`) | test    | Required  | Replaces the commented-out suite; also covers `AccountContext` equality.                                                                                                                   |

**Pseudo-code**

```
AccountPhase (enum): signedOut | preparing | ready | quiescing | blocked
    // offline with a valid access token stays `ready`; requests fail with a typed error (no `readyDegraded`)
    // while restore() runs the owner publishes `preparing`; the gate shows a loading shell for it

AccountSnapshot (freezed)
    phase: AccountPhase
    context: AccountContext?      // present only for ready / quiescing
    isEmailVerified: bool
    failure: BlockedFailure?      // typed reason, used by the blocked screen
    bool get isReady => phase == ready
    // equality matters: watchValue(select(snapshot)) must not rebuild on a same-value re-publish

AccountTransitionOwner (root, eager)
    snapshot : ValueNotifier<AccountSnapshot>          // one value for the whole app
    _queue   : FIFO tail
    _scopeHandle : ScopeHandle?
    _generation : int                                  // PRIVATE, in memory, incremented per activation. Never persisted.
    _attemptSerial : int                               // the ONLY attempt counter in the app

FUNCTION beginAttempt() -> int                          // AuthManager asks for a ticket here (Step 23)
    RETURN ++_attemptSerial                             // any new intent supersedes older ones

FUNCTION admit(job) -> TransitionResult                 // TransitionResult is a native sealed class
    IF job is candidateAdoption AND job.attemptTicket != _attemptSerial RETURN cancelled
    ENQUEUE job
    RETURN AWAIT job.result

FUNCTION restore()
    PUBLISH preparing
    target = store.readSnapshot()
    IF target.activeAccountId is null PUBLISH signedOut
    ELSE IF store.usableCredential(target.activeAccountId) is null PUBLISH blocked(authRequired)
    ELSE AWAIT prepare(target.activeAccountId)

FUNCTION prepare(accountId)
    context = AccountContext(accountId, ++_generation)
    PUBLISH preparing
    TRY
        PUSH exactly one `authenticated` scope, registering the map in dependency order
        AWAIT local initialisation of eager managers (profile/settings local reads only)
        CHECK ticket and generation still live after every await
        AWAIT store.activate(accountId)
        _scopeHandle = that scope
        PUBLISH ready(context, verified)
    CATCH failure
        PUBLISH blocked(failure)
        AWAIT popPartialScope()                                  // even half-registered objects
        AWAIT store.deactivate()                                 // credentials retained, inactive
        IF that also fails PUBLISH blocked(cleanupIncomplete)    // never activate another account

FUNCTION exit(reason)            // logout / switch / add-account all funnel here
    PUBLISH quiescing                                   // AccountGate shows NeutralTransitionPage in that build (Step 19)
    AWAIT store.deactivate()
    AWAIT SchedulerBinding.instance.endOfFrame          // the old subtree is gone (verify the API in the pinned Flutter)
    AWAIT liveScopeManager.beginAccountExit()           // Step 15's call; Step 31 swaps it for ActiveBroadcastManager.closeForAccountExit()
    AWAIT stopConnectionAndManagers()
    AWAIT popScope(_scopeHandle)                        // awaited pop, per get_it
    IF pop or cleanup fails PUBLISH blocked(cleanupIncomplete)
    ELSE PUBLISH signedOut (logout) or CONTINUE to prepare(target)

FUNCTION shutdown()
    _attemptSerial += 1
    AWAIT _queue drained; AWAIT exit(current)
```

**Done when:**

- Login publishes `ready` only after the scope's eager registrations exist and the active id is stored.
- A registration exception mid-`prepare` leaves no `authenticated` scope and publishes `blocked`.
- Two rapid `admit` calls run one after the other (FIFO), never interleaved.
- `grep -rn "pushNewScope" apps/mobile/lib` shows exactly one call site, in this file.
- A failed job does not poison the queue: the next admitted job runs.
- Two `AccountContext`s with the same id and generation compare equal; re-entering the same account gets a new generation.

**Tests:** `apps/mobile/test/app/account_transition_owner_test.dart` — (a) FIFO ordering with a held registrar, (b) partial registration rolls back and blocks, (c) a slow login superseded by logout settles `cancelled` and writes nothing, (d) same account re-entry gets a new generation and `AccountContext` equality holds.

**Watch out for:** awaiting the transition from inside a request/refresh callback (design §5: never). Auth failures are _enqueued_, not awaited inline.

**Depends on:** Steps 4, 7, 15.

---

### Step 19: Gate the account subtree by generation

**Phase:** P2
**Delivery group:** G4.
**Audit reference:** §3 [H01] "Authenticated UI is not synchronized with account-scope readiness", Recommended fix (gate authenticated routes and remount the account subtree by generation); §3 [H15] "Async operations continue after their owner is disposed", Recommended fix (check owner/session generation after every await, invalidate continuations on disposal); design §4.4. Evidence `apps/mobile/lib/app.dart:106–109`, `apps/mobile/lib/_routing/router.dart:364–389`.
**Why (plain words):** Watches and `createOnce()` feeds cache the managers they first resolved, so after a switch they can keep pointing at disposed objects. Keying the subtree by generation forces fresh cached watches and feed sources. The owner learns that the old subtree is gone the way Flutter already says it: the next frame.

_Simplified from design doc §4.4 (an unmount-registration hub with per-generation acknowledgement and a bounded timeout) because Flutter already answers "is the old subtree gone" with the frame; safety preserved because `AccountGate` replaces the subtree with `NeutralTransitionPage` while `quiescing`, and the owner awaits the end of that frame before closing live, stopping the connection and popping the scope._

**Files**

| File (exact path from repo root)              | Action | Kind   | Required? | Notes                                                                  |
| --------------------------------------------- | ------ | ------ | --------- | ---------------------------------------------------------------------- |
| `apps/mobile/lib/app/account_gate.dart`       | Create | widget | Required  | `WatchingWidget` that switches on the snapshot and keys by generation. |
| `apps/mobile/lib/app.dart`                    | Modify | widget | Required  | Insert the gate between onboarding and the authenticated shell.        |
| `apps/mobile/test/app/account_gate_test.dart` | Create | test   | Required  | Remount + dispose-before-pop.                                          |

**Pseudo-code**

```
AccountGate (WatchingWidget)
    snapshot = watchValue((AccountTransitionOwner o) => o.snapshot)
    SWITCH snapshot.phase
        signedOut     -> public/auth routes (no account lookup anywhere in this branch)
        preparing     -> NeutralTransitionPage / loading shell
        ready         ->
            KEY = ValueKey('account-${context.accountId}-${context.generation}')
            RETURN AccountScopeHost(key: KEY, context: context)   // remount => fresh watches/feeds
        quiescing     -> NeutralTransitionPage                   // replaces the subtree in THIS build
        blocked       -> BlockedFailurePage(failure)

Owner side (Step 18 exit())
    PUBLISH quiescing; AWAIT SchedulerBinding.instance.endOfFrame; only then close live and pop the scope
    // no hub, no acknowledgement, no timeout state
```

**Done when:**

- Switching A→B disposes A's subtree before the pop and builds B with new keys.
- Re-entering A later still uses a fresh generation key.
- No account-typed lookup happens in the `signedOut`/`preparing` branches (`grep -n "di<" apps/mobile/lib/app/account_gate.dart` shows only the `watchValue` selector).
- A widget test that records `dispose` order proves unmount happens before pop.

**Tests:** `apps/mobile/test/app/account_gate_test.dart` — (a) generation change remounts, (b) dispose recorded before pop, (c) unready branch performs no account lookup.

**Watch out for:** `watchValue` selectors that resolve account objects conditionally. Keep every `watch*` call in the same order on every build (watch_it ordering rule). Do not add a registration or acknowledgement class: the frame is the signal.

**Depends on:** Step 18.

---

### Step 20: Make startup retry actually rebuild what failed

**Phase:** P2
**Audit reference:** §3 [H01] "Authenticated UI is not synchronized with account-scope readiness", Recommended fix (readiness observable, gate before mounting); §3 [M24] "Critical lifecycle tests are commented out", Recommended fix (real retry coverage); audit §1.2 (retry must recreate the failed service); design §5.10. Evidence `apps/mobile/lib/main.dart:6–23`, `apps/mobile/lib/app.dart:56–114`.
**Why (plain words):** `MenoApp._retry()` re-awaits the same failed readiness future, so a failed database or config initialisation can never recover. The fix is a bootstrap function that resets what is registered and builds it again, plus an error screen that does not itself need a failed dependency.

_Simplified from design doc §5.10 (a `BootstrapOwner` with attempt ids, a cleanup ledger recording every native resource before its await, `closeAll`, and a retry that stays blocked if any close is unconfirmed) because audit §1.2 says only that retry must recreate the failed service; safety preserved because `get_it` is reset with its dispose callbacks before every retry and the error screen ignores taps while a retry is running._

**Files**

| File (exact path from repo root)                 | Action | Kind   | Required? | Notes                                                      |
| ------------------------------------------------ | ------ | ------ | --------- | ---------------------------------------------------------- |
| `apps/mobile/lib/main.dart`                      | Modify | config | Required  | Start through one `bootstrap()` function.                  |
| `apps/mobile/lib/app.dart`                       | Modify | widget | Required  | Error shell + Retry button that calls `bootstrap()` again. |
| `apps/mobile/test/app/bootstrap_retry_test.dart` | Create | test   | Required  | One small retry test against a failing factory.            |

**Pseudo-code**

```
bool _running = false                          // the only retry guard

FUNCTION bootstrap()
    IF _running RETURN                        // ignore taps while a retry runs
    _running = true
    TRY
        VALIDATE config first (Step 14) -> configuration error shell, no locator dependency
        IF anything is registered: AWAIT getIt.reset(dispose: true)     // dispose callbacks close DB, HTTP, subscriptions
        register root dependencies
        AWAIT root.allReady()
        AWAIT transitionOwner.restore()
        MOUNT router + gate
    CATCH failure
        SHOW the error shell with Retry (the shell needs no failed dependency)
        IF the failure was closing the database THEN tell the user to restart the app   // never open a second DB handle
    FINALLY _running = false
```

**Done when:**

- A factory that throws on first attempt succeeds on retry; pressing Retry twice quickly runs one retry.
- The error shell renders with no dependency on the failed object.
- Only one `Database` handle exists at any time (assert via a counter in the test double).
- A close that throws tells the user to restart instead of opening a second handle.

**Tests:** `apps/mobile/test/app/bootstrap_retry_test.dart` — (a) failed init then successful retry, (b) double-tap runs one retry, (c) a failing close shows the restart message.

**Watch out for:** retrying by calling `allReady()` again on the same registrations. The registrations must be reset and recreated.

**Depends on:** Steps 14, 18.

---

### Step 21: Refresh the router on identity and preserve the intended destination

**Phase:** P2
**Delivery group:** G5 (Steps 21 and 22 land as one change).
**Audit reference:** §3 [H01] "Authenticated UI is not synchronized with account-scope readiness", part "router refresh observes booleans, not identity"; §3 [M19] "Routing discards identity and intended destinations", Recommended fix (preserve validated pending destinations; gate protected routes on readiness); design §5.9. Evidence `apps/mobile/lib/_routing/router.dart:364–389`, `apps/mobile/lib/_routing/router_guard.dart:18–37`.
**Why (plain words):** A→B keeps `isAuthenticated` and `emailVerified` unchanged, so the router never refreshes; and a redirect to login throws away the destination, so a deep link cannot resume. Refreshing on the account snapshot (phase, account, generation, verification) and stashing the destination fixes both.

_Simplified from design doc §5.9 (a second freezed `RoutingProjection` duplicating `AccountSnapshot`) because the snapshot already carries phase, account, generation and verification; safety preserved because the router refreshes on every snapshot change, including A→B._

**Files**

| File (exact path from repo root)                   | Action | Kind   | Required? | Notes                                                                                           |
| -------------------------------------------------- | ------ | ------ | --------- | ----------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/_routing/router.dart`             | Modify | config | Required  | `refreshListenable = Listenable.merge([snapshot, onboardingDone])`, created once as a field.    |
| `apps/mobile/lib/_routing/router_guard.dart`       | Modify | config | Required  | Guard reads the `AccountSnapshot` directly; returns redirect and keeps the pending destination. |
| `apps/mobile/test/_routing/router_guard_test.dart` | Create | test   | Required  | Guard matrix.                                                                                   |

**Pseudo-code**

```
router.refreshListenable = Listenable.merge([owner.snapshot, onboardingDone])   // built ONCE, cached as a field (audit L02)

FUNCTION guard(state, snapshot, onboardingComplete, pendingSlot)
    IF NOT onboardingComplete
        PRESERVE destination; RETURN redirect('/onboarding')
    IF snapshot.phase == signedOut
        PRESERVE destination; RETURN redirect('/login')
    IF snapshot.phase IN {preparing, quiescing, blocked}
        RETURN redirect(neutralTransitionOrErrorRoute)          // no private content
    IF NOT snapshot.isEmailVerified AND route requiresVerification
        PRESERVE destination; RETURN redirect('/verify-email')
    RETURN allow
    // A private route is only ever allowed inside the ready, generation-keyed subtree
```

**Done when:**

- A→B (both authenticated and verified) still triggers a router refresh.
- Signed-out deep link to a private route survives login and lands there once.
- A `blocked` snapshot cannot render a private route.
- The `refreshListenable` is created once (no new merge per build).
- No `RoutingProjection` class exists.

**Tests:** `apps/mobile/test/_routing/router_guard_test.dart` — (a) A→B refresh fires, (b) destination preserved through login, (c) blocked redirects neutral, (d) unverified redirect preserves destination.

**Watch out for:** `refreshListenable` built from a getter (a fresh chain each access). Cache it as a field (audit L02).

**Depends on:** Steps 18, 19.

---

### Step 22: Keep one pending notification/deep-link destination instead of navigating immediately

**Phase:** P2
**Delivery group:** G5.
**Audit reference:** §3 [M19] "Routing discards identity and intended destinations", Recommended fix (preserve validated pending destinations; add platform link registration); §3 [M16] "Push payloads, channels, and tap lifecycle disagree", Recommended fix (validated destinations, pending-intent queue, cold/warm handling); design §5.9. Evidence `apps/mobile/lib/_shared/services/local_notifications_service.dart:22–34,85–126`, `apps/mobile/lib/_shared/services/push_notifications_service.dart:13–36`, `apps/api/src/infrastructure/push/model.rs:17–28,87–95`.
**Why (plain words):** A tap that arrives before the navigator exists is dropped, and a tap handled directly from a platform callback can push a private route into the wrong account. One in-memory slot holding a validated destination, consumed only when the matching account is ready, removes both.

_Simplified from design doc §5.9 (a 20-item queue with 24 h expiry, an injected clock, a dedupe list and an account-chooser dialog) because the audit asks for "validated pending destinations" and "recipient attribution" (M19, M16), not a chooser; safety preserved because a private destination is never executed as the wrong account: a mismatched tap is dropped with a snackbar._

**Platform deep-link registration (audit M19) is explicitly DEFERRED.** Registering Android App Links / iOS universal links needs a domain and hosting decision that is not part of this remediation. It is recorded in `docs/release-checklist.md` as a decision, not silently omitted. The in-app slot below is ready for it.

**Files**

| File (exact path from repo root)                          | Action | Kind   | Required? | Notes                                                                                                                |
| --------------------------------------------------------- | ------ | ------ | --------- | -------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/_routing/pending_destination.dart`       | Create | model  | Required  | Hand-written `PendingDestination` (validated path + `recipientAccountId?`) and the one-slot holder in the same file. |
| `apps/mobile/lib/_routing/router.dart`                    | Modify | config | Required  | Consume the slot when the phase is ready and the recipient matches.                                                  |
| `apps/mobile/test/_routing/pending_destination_test.dart` | Create | test   | Required  | Slot matrix.                                                                                                         |

The push/local-notification platform adapters are **not** touched here; Step 45 changes them to write into this slot (D-18).

**Pseudo-code**

```
PendingDestination (final class)
    path: String                 // validated internal location
    recipientAccountId: Id?      // from backend user_id; attribution, never proof of access

PendingDestinationSlot (root, one slot)
    set(path, recipientAccountId)
        REQUIRE path matches the small allowlist of internal patterns and UUID/query schema
        REJECT external schemes, embedded tokens, auth/reset routes, malformed recipients
        REPLACE whatever was there                          // newest wins
    take() -> PendingDestination?                            // returns and clears

Router consumption
    WHEN snapshot.phase == ready
        d = slot.peek()
        IF d is null RETURN
        IF d.recipientAccountId == null OR == readyAccount.id
            slot.take(); AWAIT router.push(d.path); CHECK generation still the same
        ELSE
            slot.take(); SHOW snackbar "that notification is for another account"   // never execute it as the wrong account
```

**Done when:**

- A tap arriving before the navigator exists is held and consumed once when ready.
- A second tap before consumption replaces the first.
- A tap for another account shows the snackbar and does not navigate.
- External URLs and auth routes are rejected by the allowlist.
- No queue, expiry, injected clock, dedupe list or chooser exists.

**Tests:** `apps/mobile/test/_routing/pending_destination_test.dart` — (a) cold tap held then consumed once, (b) newest replaces older, (c) recipient mismatch dropped with snackbar, (d) external URL rejected.

**Watch out for:** persisting the destination to disk. Design §5.9 keeps it in memory; a persisted copy needs an approved retention decision.

**Depends on:** Step 21.

---

### Step 23: Reduce `AuthManager` to a UI facade with attempt tickets

**Phase:** P2
**Audit reference:** §3 [C02] "Session-store mutations are neither atomic nor identity-conditional", part "AuthManager must not write credentials separately"; §3 [C03] "Explicit logout can leave a usable session recoverable"; §3 [M02] "Add-account flow retains password-only login mode"; §3 [L02] "Derived observables are recreated by getters", Recommended fix (cache derived observables in fields); design §5.3. Evidence `apps/mobile/lib/features/auth/manager/auth_manager.dart:69–73,182–190,202–205,249–257,318–348`.
**Why (plain words):** `AuthManager` currently saves a response, adopts it, emits a credential stream and lets a second listener adopt again — three writers for one fact, and a slow login can land after the user already switched or logged out. Making it a command facade that asks the transition owner for a ticket (one counter, not two; review OE-8), and moving all writing to the vault and the transition owner, removes the double-adoption and the race.

**Files**

| File (exact path from repo root)                          | Action | Kind    | Required? | Notes                                                                                                                                       |
| --------------------------------------------------------- | ------ | ------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/auth/manager/auth_manager.dart` | Modify | manager | Required  | Submit commands, form mode, cached projections. The attempt ticket comes from the transition owner (no second counter).                     |
| `apps/mobile/lib/features/auth/manager/user_manager.dart` | Modify | manager | Optional  | Only to delete its dependency on `activeCredential` shapes; the projection rewrite is Step 24. Loses nothing except a duplicate write path. |
| `apps/mobile/test/features/auth/auth_manager_test.dart`   | Modify | test    | Required  | Attempt-race suite.                                                                                                                         |

**Pseudo-code**

```
AuthManager (root)
    loginCommand / registerCommand / googleCommand / logoutCommand / switchCommand
    formMode : ValueNotifier<LoginFormMode>             // fullEmail | prefillSelectedIdentity
    isAuthenticated / isVerified : cached derived observables created ONCE
    // NO attempt counter here: the transition owner is the only one (Step 18)

FUNCTION submitLogin(values)
    REQUIRE form valid AND no transition is running (command restriction; UI hint only)
    ticket = transitionOwner.beginAttempt()               // the owner's counter; a newer intent supersedes this one
    response = AWAIT authService.login(values)           // anonymous transport, no interceptor state
    result = AWAIT transitionOwner.adoptCandidate(response, ticket)   // owner returns cancelled if the ticket is stale
    SWITCH result
        ready        -> DONE (routing reacts to the snapshot; no navigation from here)
        blocked      -> show typed failure, keep form values
        cancelled    -> discard tokens, persist no remembered identity
    // NEVER: store.save(...), store.adopt(...), stream.add(...), second adoption listener

FUNCTION switchTo(remembered)
    IF remembered.requiresAuthentication
        formMode = prefillSelectedIdentity(remembered)   // explicitly chosen, not inferred
        SHOW auth form; no token dispatch
    ELSE AWAIT transitionOwner.switchTo(remembered.accountId)

FUNCTION logout()
    AWAIT transitionOwner.logout(currentContext)

FUNCTION updateMetadata(profile)
    AWAIT store.updateRememberedIdentity(currentContext, safeDisplayFieldsOnly)
    // never replaces tokens with a profile DTO
```

**Done when:**

- A login response that lands after a logout produces `cancelled`, no stored credential, and no remembered identity.
- Only `SessionStore` and `AccountTransitionOwner` write credentials (`grep -rn "credential" apps/mobile/lib/features/auth/manager/auth_manager.dart` shows reads only).
- `isAuthenticated`/`isVerified` are fields, not getters that build a new `select` each call.
- No `signOut`/navigation call exists in `AuthManager` outside its own commands.

**Tests:** `apps/mobile/test/features/auth/auth_manager_test.dart` — (a) slow login superseded by logout, (b) superseded by switch, (c) two concurrent logins: last admitted wins, first settles cancelled, (d) `prefillSelectedIdentity` requires an explicit remembered selection.

**Watch out for:** keeping a "credential stream" for convenience. It is the mechanism that produced the duplicate adoption.

**Depends on:** Steps 18, 7.

---

### Step 24: Give the current user exactly one presentation source

**Phase:** P2
**Audit reference:** §3 [M12] "Current-user presentation has multiple unsynchronized sources", Recommended fix (one presentation source for current-user name/avatar; synchronize credential metadata explicitly); §3 [L02] "Derived observables are recreated by getters"; §3 [M14] (a root object must not hold a reference into an account-scoped object). Evidence `apps/mobile/lib/_shared/widgets/user_avatar_widget.dart:14–20`, `apps/mobile/lib/features/broadcast/pages/home_page.dart:61–63`, `apps/mobile/lib/features/profile/manager/my_profile_manager.dart:82–85`.
**Why (plain words):** The avatar and greeting read the auth-stored user, while edits go to `MyProfileManager`, so a name change updates one screen and not the chrome. Signed-in chrome widgets now watch the ready account's `MyProfileManager` presentation directly (they already sit inside the generation-keyed gate), and the stored user is kept for the signed-out greeting only.

_Simplified from design doc §5.3 (`UserManager.bind/unbind` with generation rejection and an ordered unbind before scope pop) because a root object holding a reference into an account-scoped object needs exactly that machinery to be safe, which is the ownership smell M14 describes; safety preserved because widgets inside the gate are disposed with the generation and the root no longer holds any account-scoped reference._

**Files**

| File (exact path from repo root)                                        | Action | Kind    | Required? | Notes                                                                                                                      |
| ----------------------------------------------------------------------- | ------ | ------- | --------- | -------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/app/widgets/current_user_avatar.dart`                  | Create | widget  | Required  | Composition connector inside the gate: `watchValue` on `MyProfileManager.presentation`, passes plain data down.            |
| `apps/mobile/lib/_shared/widgets/user_avatar_widget.dart`               | Modify | widget  | Required  | Becomes presentational: takes an image URL/name, resolves nothing.                                                         |
| `apps/mobile/lib/features/broadcast/pages/home_page.dart`               | Modify | widget  | Required  | Greeting/avatar use the connector.                                                                                         |
| `apps/mobile/lib/features/auth/manager/user_manager.dart`               | Modify | manager | Required  | Keeps only the signed-out greeting from `RememberedIdentity`. **Delete the file if nothing else uses it** after this step. |
| `apps/mobile/test/features/profile/current_user_presentation_test.dart` | Create | test    | Required  | Single-source assertions.                                                                                                  |

**Pseudo-code**

```
UserManager (root; may be removed)
    signedOutGreeting : ValueListenable<RememberedIdentity?>     // metadata only
    // NO readyProfile, NO bind(), NO unbind(), NO generation rejection

CurrentUserAvatar (WatchingWidget, mounted only inside the ready, generation-keyed subtree)
    presentation = watchValue((MyProfileManager p) => p.presentation)
    RETURN UserAvatarWidget(name: presentation.name, imageUrl: presentation.avatarUrl)
    // never reads tokens

UserAvatarWidget (presentational, no DI)
    REQUIRED name; OPTIONAL imageUrl
    RENDERS fallback initial when imageUrl is null
```

**Done when:**

- Editing the display name updates the chrome without a restart.
- `grep -rn "UserCredential\|accessToken" apps/mobile/lib/_shared/widgets/user_avatar_widget.dart` finds nothing.
- Signed out shows a remembered greeting, never a "logged in" chrome.
- No root-scope object holds a reference to `MyProfileManager` (`grep -rn "bind(\|unbind(" apps/mobile/lib/features/auth/manager/user_manager.dart` finds nothing).

**Tests:** `apps/mobile/test/features/profile/current_user_presentation_test.dart` — (a) profile edit reaches chrome, (b) signed-out greeting uses metadata, (c) chrome is not built outside the ready subtree.

**Watch out for:** registering `CurrentUserPresentation` as DI state in the account scope _and_ caching it somewhere else. One source: the profile manager.

**Depends on:** Steps 23, 18.

---

### Step 25: Send verification codes only when the user taps

**Phase:** P2
**Audit reference:** §3 [M01] "Email resend availability drives an automatic resend loop", Recommended fix (send from the tap command; derive availability from cooldown and running state; keep one error handler); §3 [L02] "Derived observables are recreated by getters", Recommended fix (retain the derived availability). Evidence `apps/mobile/lib/features/auth/pages/email_verification_page.dart:42–56,64–67,92–97,182–184`, `apps/mobile/lib/features/auth/manager/email_verification_manager.dart:36–69`.
_Simplified from design doc §5.8 (Retry-After parsing, an uncertain-outcome cooldown, a `pageAlive` input) because the audit asks only that the tap is the sole sender (M01); safety preserved because the countdown reaching zero still sends nothing._
**Why (plain words):** `canResend` changing starts a cooldown and sends an OTP, so the countdown finishing sends _another_ code with no user action. Availability is a derived value, not a trigger; making the tap the only sender and caching the derived notifier fixes the loop and the duplicate error handlers at once.

**Files**

| File (exact path from repo root)                                        | Action | Kind    | Required? | Notes                                                    |
| ----------------------------------------------------------------------- | ------ | ------- | --------- | -------------------------------------------------------- |
| `apps/mobile/lib/features/auth/manager/email_verification_manager.dart` | Modify | manager | Required  | Explicit `resend()`; cached `canResend`; injected clock. |
| `apps/mobile/lib/features/auth/pages/email_verification_page.dart`      | Modify | widget  | Required  | Button calls `resend()`; one error handler.              |
| `apps/mobile/test/features/auth/email_verification_manager_test.dart`   | Create | test    | Required  | Send-count assertions.                                   |

**Pseudo-code**

```
EmailVerificationManager (widget-owned, created with createOnce)
    _cooldownDeadline : DateTime?
    _ticker : Timer?                     // updates remaining time only
    resendCommand : Command.createAsyncNoParamNoResult(resend)
    late final canResend = <derived ONCE>(deadline, tick, running)
        // built in the constructor and retained; NEVER inside build()

FUNCTION resend()
    REQUIRE clock.nowUtc() >= deadline AND owner still the captured account generation
    AWAIT authService.resendVerification(capturedEmail)
    CHECK ticket; ON success SET deadline = now + documentedCooldown
    ON throttling or an uncertain outcome SET the same fixed cooldown constant + "try again later"   // no Retry-After parsing
    // no observer, no listener, no build-time send

FUNCTION tick()
    _tick.value = clock.nowUtc()          // forces re-evaluation of canResend only

FUNCTION initializeFromReceipt(sentAt)
    IF the API sent a code during registration, start the cooldown from THAT receipt
    // never auto-send on page build

dispose(): cancel ticker, cancel subscriptions, dispose derived notifier

Page
    final manager = createOnce(() => EmailVerificationManager(...))
    ONE registerHandler on manager.resendCommand.errors
    onPressed: canResend ? manager.resendCommand.run : null
```

**Done when:**

- Ticking the cooldown to zero sends nothing (assert send count == 1 after one tap).
- Rebuilding the page does not send anything.
- One error produces exactly one visible message.
- Resuming the app recomputes the remaining cooldown from the deadline.

**Tests:** `apps/mobile/test/features/auth/email_verification_manager_test.dart` — (a) availability becoming true sends nothing, (b) one tap sends once, (c) a throttled response applies the fixed cooldown constant, (d) one error handler fires.

**Watch out for:** `registerHandler` on `canResend` with `executeImmediately: true`. That is the automatic send in a new costume.

**Depends on:** Step 23.

---

### Step 26: Inject immutable account context into account-scoped managers

**Phase:** P2
**Audit reference:** §3 [M14] "Account-scoped managers redundantly track global account selection", Recommended fix (inject immutable account context; initialize once; use generation/cancellation for pending work); §3 [H15] "Async operations continue after their owner is disposed". Evidence `apps/mobile/lib/features/profile/manager/my_profile_manager.dart:70–74,147–160`, `apps/mobile/lib/features/settings/manager/settings_manager.dart:208–215,347–362`, `apps/mobile/lib/locator.dart:250–281`.
**Why (plain words):** Managers that already live in the account scope subscribe to the global account selector to handle a lifecycle their scope owns, so a disposed manager can begin loading the _next_ account's data. Deleting those subscriptions and injecting the context removes the duplicate ownership; the existing disposal guards stay.

**Files**

| File (exact path from repo root)                                   | Action | Kind    | Required? | Notes                                                                               |
| ------------------------------------------------------------------ | ------ | ------- | --------- | ----------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/profile/manager/my_profile_manager.dart` | Modify | manager | Required  | Take `AccountContext`; delete `_onAccountChanged` and the `UserManager` dependency. |
| `apps/mobile/lib/features/settings/manager/settings_manager.dart`  | Modify | manager | Required  | Same deletion; keep legitimate account-data subscriptions.                          |
| `apps/mobile/lib/app/composition/account_registrations.dart`       | Modify | config  | Required  | Pass the context; await local initialisation before publishing ready.               |
| `apps/mobile/test/features/profile/my_profile_manager_test.dart`   | Modify | test    | Required  | Rewrite: no global-following expectations.                                          |

**Pseudo-code**

```
MyProfileManager (account scope, eager)
    new(context, profileHttp, cache, profileMetadataCallback)
    // no UserManager, no Store subscription, no onAccountChanged

FUNCTION initialize()          // awaited by the registrar before `ready`
    cached = AWAIT cache.readOwnProfile(context.accountId)
    IF cached valid INSTALL presentation
    ELSE INSTALL loading placeholder carrying context.accountId
    // remote GET starts as tracked work AFTER readiness, not awaited by it

FUNCTION load()                // tracked, cancellable
    profile = AWAIT profileHttp.me()
    CHECK context still live AND profile.id == context.accountId
    APPLY presentation + cache
    REQUEST safe remembered-metadata update (Step 23)              // never tokens

dispose(): mark closed, cancel tracked work, dispose presentation

SettingsManager: same shape
    initialize reads the owner-bound local cache once; remote GET is tracked
    no account selector subscription
```

**Done when:**

- `grep -n "UserManager\|_onAccountChanged\|activeAccount" apps/mobile/lib/features/profile/manager/my_profile_manager.dart apps/mobile/lib/features/settings/manager/settings_manager.dart` finds nothing.
- A disposed manager's late GET writes nothing.
- A response for another account id is rejected (typed), not applied.
- Local cache read completes before the scope publishes ready.

**Tests:** `apps/mobile/test/features/profile/my_profile_manager_test.dart` — (a) cached load is synchronous-after-init and instance-scoped, (b) late response after dispose is discarded, (c) mismatched account id rejected, (d) metadata callback never replaces tokens.

**Watch out for:** removing the _disposal_ guards along with the selector. Those guards are the pattern the audit §5 item 7 calls out as good.

**Depends on:** Steps 18, 24.

---

### Step 27: (Removed) Adapt the live tier to the transition owner's exit contract

**Phase:** P2
**Status:** **Removed** by review OE-7. The number is kept so no cross-reference breaks.
**Audit findings it used to carry:** H02, H04 (account-exit closure). They are still covered, by the steps named below.

_Removed because it built a temporary adapter, an idempotent exit three times over (Step 15, Step 27, Step 30) and a test file, all thrown away in Step 35; safety preserved because the same guarantees remain: (1) Step 15 stops new opens and awaits the in-flight one; (2) the transition owner (Step 18) calls `beginAccountExit()` directly and blocks the next account if it fails; (3) Step 31 swaps that one call for `ActiveBroadcastManager.closeForAccountExit()`, whose memoisation lives only in `ActiveBroadcastManager` (Step 30). Logout's server call is a plain call with the token captured before removal (Step 7)._

**Depends on:** nothing. **Nothing to build.**

---

**Delivery group G9 — raw WebSocket client and adapter (Steps 47, 48 and 50). They depend on P1 only and ship before Phase P3 (audit §8: transport before live teardown). Step 49, which needs the live manager, stays in Phase R.**

### Step 47: Replace the Socket.IO client with a raw-WebSocket client

**Phase:** R
**Delivery group:** G9 (Steps 47, 48 and 50; depends on P1 only, so it can ship early).
**Audit reference:** §3 [C04] "Mobile and backend use incompatible realtime protocols", Recommended fix (replace the Socket.IO implementation with a raw-WebSocket adapter matching `/ws`; implement heartbeat, connection authentication, bounded reconnect and reconciliation; do not invent Socket.IO acknowledgements); evidence `apps/mobile/lib/_shared/services/socket_client.dart:69–74,153–165`, `apps/api/src/routes/ws.rs:241–271,492–503`.
**Why (plain words):** The app speaks Socket.IO framing and handshake while the backend accepts raw WebSocket JSON frames, so realtime cannot work at all. Selecting Socket.IO's "websocket transport" does not change the framing. A direct `WebSocket` client with an explicit handshake and heartbeat is the only compatible implementation.

**Files**

| File (exact path from repo root)                            | Action | Kind    | Required? | Notes                                                                                                                                                                                                                                                           |
| ----------------------------------------------------------- | ------ | ------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/_shared/services/socket_client.dart`       | Modify | service | Required  | Rewritten as a raw-WebSocket client; same class name so feature adapters keep working. Takes a credential _supplier_ (read on connect and reconnect only) and exposes `start`/`stop`; this is the only place that change is made (Step 44 adds only the owner). |
| `apps/mobile/lib/_shared/model/socket.dart`                 | Modify | model   | Required  | Frame envelope + a `ConnectionPhase` enum.                                                                                                                                                                                                                      |
| `apps/mobile/pubspec.yaml`                                  | Modify | config  | Required  | Remove `socket_io_client`; no replacement package (uses `dart:io`).                                                                                                                                                                                             |
| `apps/mobile/test/_shared/services/socket_client_test.dart` | Create | test    | Required  | Frame/heartbeat/reconnect tests against a local server.                                                                                                                                                                                                         |

**Pseudo-code**

```
SocketClient (transport only; no account logic)
    new(uri, credentialSupplier, logger, clock)
    WebSocket? _socket                             // dart:io WebSocket directly; no interface per class (rule 2, D-15)
    phase : ValueNotifier<ConnectionPhase>          // idle | connecting | open | reconnecting | closed

FUNCTION start()
    REQUIRE not already holding a socket                // idempotent for the current iteration
    phase = connecting
    token = AWAIT credentialSupplier()                  // committed credential; never a constructor capture
    _socket = AWAIT WebSocket.connect(uri, headers: { authorization: bearer(token) })
        // match whatever handshake the backend validates in routes/ws.rs; do not invent a query token
    LISTEN frames -> decode envelope {event, data} -> emit typed event
    START heartbeat timer: send ping every N seconds, expect pong within M               // bounded
    phase = open

FUNCTION onDisconnect(code, reason)
    PRESERVE (code, reason)                              // H07: never discard terminal reasons
    IF code is terminal (auth rejected, room deleted, kicked) -> phase = closed; emit terminal
    ELSE IF attempt < maxAttempts -> delay = backoff(attempt); phase = reconnecting; AWAIT start()
    ELSE phase = closed; emit exhausted

FUNCTION stop()                                          // idempotent
    detach listeners first, then close, then cancel heartbeat
    phase = closed

FUNCTION send(event, data)
    REQUIRE phase == open ELSE throw typed NotConnected
    _socket.add(jsonEncode({ 'event': event, 'data': data }))
    // no ack protocol: the backend has none, and inventing one hides real failures
```

**Done when:**

- Against a locally running `apps/api`, the client completes the handshake and receives a known event.
- A heartbeat miss triggers exactly one reconnect attempt chain, bounded, with visible backoff.
- A close code the backend uses for auth rejection is reported as terminal and not retried.
- `socket_io_client` is no longer a dependency (`grep -n socket_io apps/mobile/pubspec.yaml`).

**Tests:** `apps/mobile/test/_shared/services/socket_client_test.dart` — (a) frame encode/decode round trip, (b) heartbeat miss reconnects, (c) terminal close code stops retrying, (d) `send` before open throws typed error.

**Watch out for:** keeping the Socket.IO `path`/`transports` semantics in the settings. The backend has one endpoint with one framing.

**Depends on:** P1 only (Steps 4 and 8, for the credential and the approved origin). It no longer depends on Step 44.

---

### Step 48: Make adapter install/detach explicit and idempotent

**Phase:** R
**Delivery group:** G9.
**Audit reference:** §3 [H06] "No production path starts realtime delivery", Recommended fix (an explicit account connection owner initializes handlers before connecting); evidence `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart:72–129`.
**Why (plain words):** The adapter only installs handlers inside `listen()`, and subscribing to its stream does not call it — so even a working transport delivered nothing. Making install/detach explicit, idempotent and composition-callable means the connection owner decides when delivery is active.

**Files**

| File (exact path from repo root)                                            | Action | Kind    | Required? | Notes                                                                                                 |
| --------------------------------------------------------------------------- | ------ | ------- | --------- | ----------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart` | Modify | service | Required  | `install()`/`detach()` replace `listen()`-implies-install; guarded decode; no side-effectful getters. |
| `apps/mobile/lib/features/broadcast/services/broadcast_events.dart`         | Create | model   | Required  | Typed event hierarchy the adapter emits (a native `sealed class`; no freezed).                        |
| `apps/mobile/test/features/broadcast/broadcast_socket_service_test.dart`    | Create | test    | Required  | Install/detach matrix.                                                                                |

**Pseudo-code**

```
BroadcastSocketService
    _installed = false
    _handles : List<ListenableSubscription>

FUNCTION install()
    IF _installed RETURN                                  // idempotent
    _installed = true
    _handles = [
        socket.on(event: newBroadcast).listen(_emitNew),
        socket.on(event: endedBroadcast).listen(_emitEnded),
        socket.on(event: participantJoined).listen(_emitJoin),
        ... one per event the feature consumes ...
    ]

FUNCTION detach()
    IF NOT _installed RETURN
    _installed = false
    local = _handles; _handles = []                       // detach SYNCHRONOUSLY into locals
    FOR h IN local AWAIT h.cancel()

FUNCTION _emitEnded(frame)
    typed = BroadcastEvent.tryDecode(frame)               // guarded; null -> log sanitised, emit nothing
    IF typed is null RETURN
    _events.add(typed)
```

**Done when:**

- `install()` twice creates one set of handles; `detach()` twice cancels once.
- A malformed frame emits nothing and does not throw.
- The connection owner calls `install()` before `socket.start()` (asserted by call order).
- Subscribing to the stream without `install()` receives nothing (proving install is required and explicit).

**Tests:** `apps/mobile/test/features/broadcast/broadcast_socket_service_test.dart` — (a) idempotent install, (b) detach cancels all handles, (c) malformed frame dropped, (d) install-before-start ordering.

**Watch out for:** cancelling handles across an `await` while a new install overwrites the list. Copy to locals first (the same bug shape as M09).

**Depends on:** Step 47.

---

### Step 50: Decode event payloads strictly and keep "ended" distinct from a summary

**Phase:** R
**Delivery group:** G9.
**Audit reference:** §3 [M10] "Invalidation parsing and event enrichment are inconsistent", Recommended fix (explicit RFC3339 typed payload; guarded decoding; keep end signals distinct from rich REST summaries and enrich/refetch); evidence `apps/mobile/lib/features/broadcast/model/dtos.dart:113–119`, `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart:120–126,167–189`, `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:202–205`, `apps/api/src/infrastructure/ws/dto.rs:169–173,234–238`.
**Why (plain words):** The backend serialises a bare `OffsetDateTime` (which is not RFC3339 text), invalidation payloads bypass the adapter's guarded decode, and an end event carrying only an id and reason is turned into a full card with empty fields. Explicit typed payloads, one guarded decode, and a refetch for anything richer fix all three.

**Files**

| File (exact path from repo root)                                            | Action | Kind    | Required? | Notes                                                                |
| --------------------------------------------------------------------------- | ------ | ------- | --------- | -------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/model/dtos.dart`                        | Modify | model   | Required  | Explicit `@JsonKey(fromJson:)` RFC3339 parsing for event timestamps. |
| `apps/mobile/lib/features/broadcast/services/broadcast_events.dart`         | Modify | model   | Required  | `EndedSignal(broadcastId, reason)` separate from `BroadcastSummary`. |
| `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart` | Modify | service | Required  | All decodes go through `tryDecode`.                                  |
| `apps/mobile/test/features/broadcast/broadcast_event_decode_test.dart`      | Create | test    | Required  | Payload fixtures from the backend.                                   |

**Pseudo-code**

```
Timestamps
    REQUIRE a string timestamp matching RFC3339; parse with DateTime.parse
    IF the value arrives as anything else (array/object/int) -> decode FAILS        // visible, not silent

BroadcastEvent (native sealed class)
    NewBroadcast(summary)
    EndedSignal(broadcastId, reason)           // carries NO summary fields
    ParticipantJoined(...) / Left(...)
    HomeInvalidated(at)

FUNCTION apply(EndedSignal)
    MARK the card as ended with reason
    SCHEDULE a refetch of that broadcast's summary (REST)      // never fabricate empty fields
    // the feed's invalidation path uses the same guarded decode, not a parallel parser
```

**Done when:**

- A backend-shaped payload with a real RFC3339 string decodes; an `OffsetDateTime`-as-array payload fails loudly in a test fixture.
- An ended signal never produces a card with empty title/creator.
- Invalidation decoding shares the adapter's guarded path (no second parser).
- Malformed events are dropped with one sanitised log line.

**Tests:** `apps/mobile/test/features/broadcast/broadcast_event_decode_test.dart` — (a) all event fixtures decode, (b) non-RFC3339 timestamp rejected, (c) ended signal triggers a refetch instead of a synthetic card, (d) guarded decode of an unknown event emits nothing.

**Watch out for:** `DateTime.parse` on a numeric or list value with a `try/catch` that silently defaults to `now`. Defaulting to "now" re-creates the H10 expiry problem in the feed.

**Depends on:** Step 48.

---

---

## 9. Phase P3 — Account-owned `ActiveBroadcastManager`

**Findings finished here:** H02, H03, H04, H08, M21, the H16 seam for roster ownership, and the L05 removal of the duplicate/obsolete live paths.

**What you will have when this phase is done (3–5 lines):**

- One account-scoped `ActiveBroadcastManager` that owns at most one ordinary `ActiveBroadcastSession` object — no `live:<id>` DI scope, no registration-as-state.
- One authoritative `LiveSnapshot` (phase + broadcast + failure + end reason) that the banner, the live page and the ended page all read.
- A single idempotent close path that runs in `finally`, with the end summary retained outside the disposed session.
- Creator authority, publish capability and end authority as three separate facts.
- An account exit that awaits `closeForAccountExit()` and a transition that cannot overlap two accounts.

**How to know it's safe to ship:** the `active_broadcast_manager_test.dart` exit cases pass; opening then logging out leaves no transport, no timer and no live scope; a failed REST end still closes locally and still shows the ended page; a cohost sees "leave", never "end"; navigating away from the live page keeps audio playing.

**Order:** 28 → 29 → 30 → 31 → 32 → 33 → 34 → 35. Steps 28–29 (G6) are internal to the session; Step 30 follows. Steps 31–35 (G7) are one shippable slice: the registration change, the page/banner migration and the deletion must move together, or a caller will resolve an object that no longer exists (the build breaks).

**What we are deliberately NOT doing in P3:** no LiveKit reconnect state machine work (Phase R/Step 49), no roster pagination (Phase B/Step 54), no new session framework, no `GetIt` per-session container — the audit §1.3 comparison rejects it as unnecessary for one audio session.

---

### Step 28: Replace the live state booleans with one phase and separate capabilities

**Phase:** P3
**Delivery group:** G6 (Steps 28 and 29 land together; the timer rewrite from Step 85 is folded in here, D-19).
**Audit reference:** §1.4 "State machine" (one authoritative session phase; keep role/capabilities separate); §3 [H08] "Publishing permission is mistaken for creator authority", Recommended fix (separate creator identity, publish capability and end authority). Evidence `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart:127–130,203–205`, `apps/mobile/lib/features/broadcast/model/entities.dart:207–208`, `apps/mobile/lib/features/broadcast/widgets/broadcast_control_buttons.dart:20–25`.
_Type note (D-25): `LivePhase` is a plain `enum` (payload-free set); only `LiveSnapshot` is freezed._
**Why (plain words):** `isHost` becomes true for anyone who may publish, so a cohost gets the creator-only `/end` call and cannot leave correctly. Independent `state`, `status`, `_isLeaving` and `_hasSession` also let the UI contradict itself. One phase plus three explicit capability booleans make the controls and the exit branch follow the same truth.

**Files**

| File (exact path from repo root)                                            | Action | Kind          | Required? | Notes                                                                                                                                                                             |
| --------------------------------------------------------------------------- | ------ | ------------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/model/live_snapshot.dart`               | Create | freezed model | Required  | `LivePhase` enum, `LiveFailure`, `LiveSnapshot` (freezed).                                                                                                                        |
| `apps/mobile/lib/features/broadcast/model/entities.dart`                    | Modify | freezed model | Required  | Add capability fields; keep `EndReason`.                                                                                                                                          |
| `apps/mobile/lib/features/broadcast/widgets/broadcast_control_buttons.dart` | Modify | widget        | Required  | Gate mic / stop / leave independently.                                                                                                                                            |
| `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart`      | Modify | manager       | Optional  | Only to expose phase/capabilities from existing state while internals are unchanged; skip if Step 29 lands in the same change. Loses nothing if Steps 29–30 are done immediately. |

**Pseudo-code**

```
enum LivePhase { idle, connecting, live, reconnecting, ending, ended, failed }

LiveSnapshot (freezed)
    phase: LivePhase
    broadcast: Broadcast?
    role: ParticipantRole?          // creator | cohost | listener | none
    canPublish: bool                // MAY publish audio
    canEndBroadcast: bool           // creator authority ONLY
    failure: LiveFailure?           // typed + retryable flag
    endReason: EndReason?
    endedSummary: BroadcastSummary? // retained after resources are gone (Step 29)
    // equality: a state re-publish with identical values must not rebuild widgets

Capabilities come from the server payload
    isCreator  = broadcast.creatorId == context.accountId
    canPublish = participant role allows publishing (creator OR accepted cohost)
    canEnd     = isCreator
    // NEVER `isHost = canPublish`

Controls
    mic control        -> canPublish
    "End broadcast"    -> canEnd
    "Leave"            -> !canEnd && in a live phase
```

**Done when:**

- A cohost session shows "Leave" and calls the leave endpoint; the creator shows "End".
- A cohost never issues `/end` (`grep -n "endBroadcast" apps/mobile/lib/features/broadcast | grep -v "canEnd"` shows no unconditional call).
- The UI derives everything from `LiveSnapshot.phase` and the three capabilities; no widget reads `state` and `status` separately.
- Two identical snapshots do not notify listeners (value equality).

**Tests:** `apps/mobile/test/features/broadcast/live_snapshot_test.dart` — (a) cohost capability matrix, (b) creator capability matrix, (c) identical snapshot does not notify, (d) `endedSummary` survives `copyWith(phase: idle)`.

**Watch out for:** keeping `LiveStatus` and adding `LivePhase` beside it. Delete the old enum and update its call sites (`rg -l "LiveStatus" apps/mobile/lib`) in the same change.

**Depends on:** Step 26.

---

### Step 29: Create the session's resources with the session and close them once

**Phase:** P3
**Delivery group:** G6 (lands with Step 28; owns the single timer-manager rewrite, which absorbs Step 85).
**Audit reference:** §3 [H03] "Session disposal can access an uninitialized timer", Recommended fix (create the timer with the session; do not invoke UI commands from disposal); §3 [H04] "Failed end/leave skips session closure; ended UI depends on disposed resources", Recommended fix (one idempotent close path in `finally`; retain the ending result outside active resources); §1.4 ownership table row "Leave/end". Evidence `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart:40–64,119–145,219,356–426`.
**Why (plain words):** The timer is `late final` and only the page initialises it, so a session that exits before the init page runs throws during cleanup; and a REST failure runs the disconnect in `finally` but then skips `_live.end()`, leaving recovery state behind. Owning the timer at construction and putting _all_ closure in one `finally` path removes both, and keeping the end summary on the manager means the ended page never needs the disposed object.

**Files**

| File (exact path from repo root)                                           | Action           | Kind    | Required? | Notes                                                                                                                                                                                                                       |
| -------------------------------------------------------------------------- | ---------------- | ------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/session/active_broadcast_session.dart` | Create           | manager | Required  | Session-owned timer, transport, subscriptions, one `close()`.                                                                                                                                                               |
| `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart`     | **Keep for now** | manager | Required  | Its responsibilities move here and to Step 30's manager. **Do not delete it in this step**: `LiveScopeManager` and the pages still use it until Steps 31–35, so deleting it breaks the build. **Step 35 deletes it** (D-6). |
| `apps/mobile/lib/features/broadcast/manager/broadcast_timer_manager.dart`  | Modify           | manager | Required  | Plain methods, not commands (M21); monotonic anchor, and the one-second rebuild-granularity fix from Step 85. This is the **only** timer-manager rewrite (D-19).                                                            |
| `apps/mobile/test/features/broadcast/active_broadcast_session_test.dart`   | Create           | test    | Required  | Close-path matrix.                                                                                                                                                                                                          |

**Pseudo-code**

```
ActiveBroadcastSession (private resource object; created only after entry is accepted)
    new(context, session, roomTransport, participantsController, clock)
    _timer = SessionTimer(startedAt: clock.monotonic())      // created HERE, not by a page
    _closed = false
    _closeResult : Future<void>?

FUNCTION start()
    AWAIT roomTransport.connect()                            // listeners installed BEFORE connect
    AWAIT participants.attach()

FUNCTION close(reason, {serverOutcome}) -> Future<void>      // idempotent
    IF _closeResult exists RETURN it
    _closeResult = run(reason)
    RETURN _closeResult
FUNCTION run(reason)
    TRY
        IF reason is userExit
            serverOutcome = AWAIT attemptServerLeaveOrEnd()   // captured identity, one attempt
        // a server failure is RECORDED, never thrown out of close()
    CATCH e
        serverOutcome = unconfirmed(e)
    FINALLY
        _timer.dispose()                                     // safe: it always exists
        AWAIT participants.detach()
        AWAIT roomTransport.dispose()
        CANCEL all subscriptions (handles captured into locals first)
        _closed = true

Summary ownership
    manager.retainEndSummary(builtFrom(lastKnownBroadcast, serverOutcome, reason))
    // the ended page reads manager.snapshot.endedSummary, never the session
```

**Done when:**

- Opening a session and closing it before `start()` completes throws no `LateInitializationError`.
- A REST end failure still disposes the timer, roster and transport exactly once.
- `close()` called twice awaits the same future.
- Changing the device clock does not change elapsed; a received `initialElapsed` survives the first tick; a server reconciliation moves elapsed once (monotonic anchor, audit L03).
- No `Command` is run from `dispose()` (`grep -n "Command" apps/mobile/lib/features/broadcast/session/active_broadcast_session.dart` finds none).

**Tests:** `apps/mobile/test/features/broadcast/active_broadcast_session_test.dart` — (a) close before start, (b) server failure still closes everything once, (c) double close, (d) summary retained after close. `apps/mobile/test/features/broadcast/broadcast_timer_manager_test.dart` — (e) initial elapsed preserved, (f) wall-clock change ignored, (g) server reconciliation.

**Watch out for:** `finally` that only disconnects the transport. Timer, roster, subscriptions and transport are all "local resources" and all close.

**Depends on:** Step 28.

---

### Step 30: Build `ActiveBroadcastManager` as the single live owner

**Phase:** P3
**Audit reference:** §1.3 "Account manager with nullable active session" (recommended option); §1.4 "Recommended approach", "Ownership and lifecycle", "Short code sketch"; §3 [H02] "Live transitions can outlive account teardown or supersede newer sessions", Recommended fix (one account-owned transition owner; generation checks after awaits; quiesce before disposing); §3 [M21] "Scope/connection orchestration is over-commanded", Recommended fix (plain async entry/close methods; commands only for user actions). Evidence `apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart:155–173,195–223,289–340`.
**Why (plain words):** Live work currently exists in a DI scope whose _registration_ doubles as connection state, navigation readiness and session identity. One manager holding `ActiveBroadcastSession?`, serialising enter/replace/close and publishing one snapshot gives the app a stable owner across pages (needed for the banner) without the registration protocol.

**Files**

| File (exact path from repo root)                                           | Action | Kind    | Required? | Notes                                                       |
| -------------------------------------------------------------------------- | ------ | ------- | --------- | ----------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/manager/active_broadcast_manager.dart` | Create | manager | Required  | The owner described below; registered in the account scope. |
| `apps/mobile/lib/features/broadcast/services/live_session_store.dart`      | Modify | service | Required  | Recovery intent only (already non-secret from Step 16).     |
| `apps/mobile/test/features/broadcast/active_broadcast_manager_test.dart`   | Create | test    | Required  | Enter/replace/close/recover races.                          |

**Pseudo-code**

```
ActiveBroadcastManager (account scope, implements Disposable)
    context : AccountContext
    _session : ActiveBroadcastSession?
    _snapshot : ValueNotifier<LiveSnapshot>(idle)
    _operationGeneration : int
    _queue : FIFO tail
    _exiting : bool

VALUE LISTENABLE  snapshot -> LiveSnapshot
COMMANDS          join(Command<EnterRequest, void>), leaveOrEnd(Command<void, void>)   // user actions only

FUNCTION enter(request) -> Future<EnterResult>          // public: queues, then runs the private _enter
    RETURN AWAIT _enqueue(() => _enter(request))

FUNCTION _enter(request) -> Future<EnterResult>         // PRIVATE and NOT queued: only ever called from inside a queued job
    IF _exiting RETURN rejected(accountExiting)
    gen = ++_operationGeneration                         // invalidates older in-flight work
    PUBLISH connecting
    IF _session exists AWAIT _session.close(replaced)    // never overlap two sessions
    TRY
        accepted = AWAIT broadcastHttp.joinOrStart(request, context)   // captured identity
        CHECK gen == _operationGeneration AND NOT _exiting
        session = ActiveBroadcastSession(...)            // ordinary object, not in DI
        _session = session                               // ASSIGN BEFORE start(), so a failure can still find and close it (D-5)
        AWAIT session.start()
        CHECK gen == _operationGeneration                // if superseded, the newer operation closes this session
        PUBLISH live(snapshotFor(session))
    CATCH recoverable -> PUBLISH reconnecting (SDK reconnect owns the first attempt)
    CATCH terminal
        AWAIT closeLocal()                               // closes _session (timer, roster, transport) if start() threw
        _session = null
        PUBLISH failed(typed, retryable)
    RETURN result

FUNCTION leaveOrEnd()
    ENQUEUE:
        session = _session
        IF session is null OR phase IN {failed, ended} RETURN   // null-guard (D-5)
        PUBLISH ending
        AWAIT session.close(userExit)                      // close() does the REST attempt + local close
        RETAIN ended summary (reason, server outcome)
        _session = null
        PUBLISH ended(summary)

FUNCTION recover()
    ENQUEUE:                                               // runs INSIDE the queue
        intent = AWAIT liveSessionStore.read(context.accountId)
        IF intent is null RETURN
        IF a session already exists RETURN                 // recovery never replaces a newer session
        AWAIT _enter(fromRecovery(intent))                 // calls the PRIVATE unqueued _enter: calling enter() here would
                                                           // enqueue behind this very job and deadlock (D-4)
        // _enter increments _operationGeneration itself; a newer operation supersedes this one

FUNCTION closeForAccountExit() -> Future<void>              // the account exit contract
    IF _pending exists RETURN _pending
    _exiting = true; _operationGeneration += 1              // every in-flight enter is now stale
    _pending = (): AWAIT _queue drained; IF _session exists AWAIT _session.close(accountExit); PUBLISH idle
    RETURN _pending                                         // idempotent, awaited before scope pop

FUNCTION applyConnectionPhase(phase)                        // fed by Step 49
    IGNORE if no session; map reconnecting/live/terminal; a terminal entry calls leaveOrEnd terminal path

dispose(): closeForAccountExit() then dispose _snapshot
```

**Done when:**

- `recover()` completes (no FIFO deadlock): it calls the private `_enter`, never the queued `enter`.
- `session.start()` throwing leaves no leaked timer or transport, and a later `leaveOrEnd()` with no session is a no-op.
- Two `enter` calls in a row leave exactly one session and one transport.
- `closeForAccountExit()` during a held `enter` results in no session and no publish of `live`.
- `recover()` while a session exists does nothing.
- Every published phase is reachable and monotone within a single operation (no `live` after `ending`).
- The manager is the only writer of `LiveSnapshot`.

**Tests:** `apps/mobile/test/features/broadcast/active_broadcast_manager_test.dart` — (a) enter replaces an existing session once, (b) exit during enter publishes nothing live, (c) recovery does not replace a newer session, (d) leave on terminal failure still publishes `ended` with a summary, (e) `recover()` does not deadlock, (f) a throwing `start()` closes the half-built session.

**Watch out for:** making `enter()` a command pipeline. Plain methods; the two commands are the user-facing wrappers (audit M21).

**Depends on:** Step 29. The `closeForAccountExit()` memoisation (`_pending`) lives **only** here; no other layer repeats it (review OE-7).

---

### Step 31: Register only `ActiveBroadcastManager` in the account scope

**Phase:** P3
**Delivery group:** G7 (Steps 31, 32, 33, 34 and 35 land as one change with a checklist per part: the registration change, page migration and deletions must move together or the build breaks).
**Audit reference:** §1.4 "DI registration map" (register only `ActiveBroadcastManager`; the session is an ordinary owned object); §3 [H02] "Live transitions can outlive account teardown or supersede newer sessions", Recommended fix (one account-owned transition owner; quiesce before disposing resources); §3 [M21] "Scope/connection orchestration is over-commanded", Recommended fix (remove session DI readiness). Evidence `apps/mobile/lib/locator.dart:320–389`.
**Why (plain words):** The per-live registrar creates participants, transport and a conditional session manager, so "is it registered?" answers three different questions. Registering one manager in the account scope and letting it own an ordinary session removes the whole registration protocol from the DI map.

**Files**

| File (exact path from repo root)                             | Action | Kind    | Required? | Notes                                                                                                                                                          |
| ------------------------------------------------------------ | ------ | ------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/app/composition/account_registrations.dart` | Modify | config  | Required  | Register `ActiveBroadcastManager`; no live registrar.                                                                                                          |
| `apps/mobile/lib/locator.dart`                               | Modify | config  | Required  | Delete `_registerLiveScope` and the live scope names.                                                                                                          |
| `apps/mobile/lib/app/account_transition_owner.dart`          | Modify | manager | Required  | Swap the one call `liveScopeManager.beginAccountExit()` (Step 18) for `activeBroadcastManager.closeForAccountExit()`. No adapter exists (Step 27 was removed). |
| `apps/mobile/test/locator_test.dart`                         | Modify | test    | Required  | Assert the two-tier registration map.                                                                                                                          |

**Pseudo-code**

```
Account registrar (dependency order; disposal is the reverse)
    1 AccountContext (eager, immutable)
    2 Account HTTP facade + SessionInterceptor (eager)         // Step 8/9
    3 (reserved; no generic local-data facade exists)
    4 owner-bound local services: BroadcastLocalService, ProfileCacheService, SettingsLocalService (eager, each takes ownerId)   // Step 36
    5 Broadcast/Profile/Settings HTTP services (lazy)
    6 UserProfileRepository (eager)
    7 MyProfileManager, SettingsManager (eager)                 // Step 26
    8 SocketClient + BroadcastSocketService (eager when connection enabled)
    9 AccountConnectionOwner (eager)                            // Step 44
   10 LiveSessionStore (eager)
   11 ActiveBroadcastManager (eager registration point)
   12 FavouritesManager (lazy)             // StreamManager is deleted (Step 34)

REMOVE
    _registerLiveScope, the `live:<id>` scope name, conditional LiveSessionManager registration
    WillSignalReady / signalReady integration
    any `di.maybeGet<LiveSessionManager>()` lookup

Transition owner exit
    AWAIT activeBroadcastManager.closeForAccountExit()          // same contract as Step 15's call, new owner
```

**Done when:**

- `grep -rn "live_" apps/mobile/lib/locator.dart` finds no scope name.
- After login there are exactly two application scopes: `root` and `authenticated` (test asserts).
- No file outside the broadcast feature resolves a live-scoped type.
- Account exit still awaits a real closure (Step 30's contract).

**Tests:** `apps/mobile/test/locator_test.dart` — (a) single authenticated scope after login, (b) no live scope after entering a broadcast, (c) `ActiveBroadcastManager` resolves inside the account scope and not at root.

**Watch out for:** keeping the old registrar "just in case". Two owners is precisely the failure mode the audit describes.

**Depends on:** Step 30.

---

### Step 32: Port the live pages to the manager's snapshot

**Phase:** P3
**Delivery group:** G7 (Steps 31, 32, 33, 34 and 35 land as one change with a checklist per part: the registration change, page migration and deletions must move together or the build breaks).
**Audit reference:** §1.2 "Broadcast scope" failures table (registration existence used as connection state; transition page polls and self-disposes); §3 [H04] "Failed end/leave skips session closure; ended UI depends on disposed resources"; §3 [M21] "Scope/connection orchestration is over-commanded", Recommended fix (use phase observation for the transition page, not DI readiness). Evidence `apps/mobile/lib/_shared/pages/live_session_init_page.dart:98–174`, `apps/mobile/lib/_shared/layouts/live_session_shell.dart`, `apps/mobile/lib/features/broadcast/widgets/live_session_banner.dart:34–47`.
**Why (plain words):** The init page bridges notifier listeners to futures and only settles on a manual signal, and the shell asks whether a scope is registered — a check that does not even schedule a rebuild. Reading `LivePhase` from the manager is both simpler and the only correct source once the scope is gone.

**Files**

| File (exact path from repo root)                                       | Action | Kind   | Required? | Notes                                                                                                                                                                                                                                                                    |
| ---------------------------------------------------------------------- | ------ | ------ | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `apps/mobile/lib/features/broadcast/pages/live_session_init_page.dart` | Move   | widget | Required  | From `_shared/pages/`; rewrite to observe the phase.                                                                                                                                                                                                                     |
| `apps/mobile/lib/features/broadcast/layouts/live_session_shell.dart`   | Move   | widget | Required  | From `_shared/layouts/`; no `isRegistered`, no maybeGet.                                                                                                                                                                                                                 |
| `apps/mobile/lib/features/broadcast/pages/live_broadcast_page.dart`    | Create | widget | Required  | The live surface: watches phase + capabilities.                                                                                                                                                                                                                          |
| `apps/mobile/lib/features/broadcast/widgets/pre_stream_modal.dart`     | Modify | widget | Required  | Live entry consults the session **phase**, not "is there an active id" (audit M19 evidence `pre_stream_modal.dart:16–19`). Uses `LivePhase`: while a session is connecting/live/reconnecting/ending the modal offers return-or-leave instead of starting a second entry. |
| `apps/mobile/test/features/broadcast/live_phase_routing_test.dart`     | Create | test   | Required  | Phase→screen matrix.                                                                                                                                                                                                                                                     |

**Pseudo-code**

```
LiveSessionInitPage (WatchingWidget)
    phase = watchValue((ActiveBroadcastManager m) => m.snapshot)
    switch phase.phase
        connecting | reconnecting -> progress with attempt text
        live                     -> redirect to the live page (once, via callOnceAfterThisBuild)
        failed                   -> typed failure + retry (retry calls manager.join again)
        ending                   -> progress, inputs disabled
        ended                    -> go to the ended page with the retained summary
        idle                     -> leave the flow (no session)
    // no listeners bridged to futures, no manual signalReady, no "settle" command

PreStreamModal (A-3 / M19)
    phase = manager.snapshot.value.phase
    IF phase IN {connecting, live, reconnecting, ending}   // a session already exists
        OFFER "return to broadcast" / "leave first"; NEVER start a second entry
    ELSE proceed to start/join
    // decide from LivePhase, never from `activeId != null`

LiveSessionShell (WatchingWidget)
    snapshot = watchValue((ActiveBroadcastManager m) => m.snapshot)
    // shell visibility derives from the PHASE, never from scope registration
```

**Done when:**

- Killing and reopening the init route during `connecting` shows progress and eventually `live`, with no manual bridge.
- The shell hides itself for `idle`/`ended` and shows for every other phase, including `failed`.
- No `maybeGet` or `isRegistered` remains in either page.
- Navigation into the live page happens once per `live` transition (not per rebuild).
- Opening the pre-stream modal while a session is live offers return/leave and never starts a second entry.

**Tests:** `apps/mobile/test/features/broadcast/live_phase_routing_test.dart` — (a) each phase renders the right surface, (b) `live` navigates once, (c) `ended` shows the retained summary, (d) no DI probe is needed to build the shell, (e) the pre-stream modal decides from `LivePhase`.

**Watch out for:** redirecting on every rebuild of the `live` state. Use `callOnceAfterThisBuild` or a captured "already navigated for this generation" flag.

**Depends on:** Step 31.

---

### Step 33: Move the banner into authenticated chrome and load the ended page by id

**Phase:** P3
**Delivery group:** G7 (Steps 31, 32, 33, 34 and 35 land as one change with a checklist per part: the registration change, page migration and deletions must move together or the build breaks).
**Audit reference:** §1.2 "Early-registration/banner assessment" (the banner already avoids the historical first-build bug — keep that fixed) and the exit-path table; §3 [H04] "Failed end/leave skips session closure; ended UI depends on disposed resources", Recommended fix (ended route loads by its broadcast ID; keep the ending result outside active resources). Evidence `apps/mobile/lib/features/broadcast/widgets/live_session_banner.dart:34–47`, `apps/mobile/lib/features/broadcast/pages/ended_broadcast_page.dart:30–33`, `apps/mobile/lib/_routing/router.dart:334–355`.
**Why (plain words):** The banner lives only inside `HomePage`, so it disappears on other tabs, and the confirmation dialog holds a manager reference across an `await` that a switch can dispose. The ended page resolves a manager that termination has already removed. Moving the banner into app chrome and giving the ended route its own broadcast id fixes both without inventing new state.
**Qualification (audit §1.2):** the banner's current `maybeGet` plus "id published after registration" is already correct for the first-build case. Do not "fix" that; only remove the conditional lookup itself, since the manager now always exists while authenticated.

**Files**

| File (exact path from repo root)                                      | Action | Kind   | Required? | Notes                                                                              |
| --------------------------------------------------------------------- | ------ | ------ | --------- | ---------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/widgets/live_session_banner.dart` | Modify | widget | Required  | Watch `ActiveBroadcastManager.snapshot`; capture data before dialogs.              |
| `apps/mobile/lib/app/layouts/authenticated_shell.dart`                | Create | widget | Required  | Mounts the banner above the tab content.                                           |
| `apps/mobile/lib/_shared/layouts/root_shell.dart`                     | Move   | widget | Required  | → `apps/mobile/lib/app/layouts/root_shell.dart`; composes the authenticated shell. |
| `apps/mobile/lib/features/broadcast/pages/ended_broadcast_page.dart`  | Modify | widget | Required  | Takes `broadcastId`; loads details by id when the summary is absent.               |
| `apps/mobile/test/features/broadcast/ended_page_test.dart`            | Create | test   | Required  | Page works without a session.                                                      |

**Pseudo-code**

```
LiveSessionBanner (WatchingWidget)
    snapshot = watchValue((ActiveBroadcastManager m) => m.snapshot)
    IF snapshot.phase == idle RETURN SizedBox.shrink()
    ON tap of "confirm leave"
        CAPTURE (broadcastId, phase, canEnd) BEFORE any await
        AWAIT showDialog(...)                          // dialog data comes from the captured values
        IF confirmed AND manager.snapshot still matches the captured broadcastId
            manager.leaveOrEnd.run()
        // a switch during the dialog makes the second check false: nothing runs
    // no maybeGet, no isRegistered, no three-observable juggling

EndedBroadcastPage(broadcastId)
    summary = watchValue((ActiveBroadcastManager m) => m.snapshot.endedSummary)
    IF summary matches broadcastId RENDER it
    ELSE AWAIT broadcastDetailsManager.load(broadcastId)    // widget-owned manager
    // resolves nothing from DI that could have been disposed

AuthenticatedShell
    Chat-friendly frame: banner + IndexedStack of tabs
```

**Done when:**

- The banner is visible on every authenticated tab, not just home.
- Answering the leave dialog after a switch performs no action.
- The ended page renders after a process-level termination event that removed the session.
- The ended route uses its `broadcastId` (`grep -n "broadcastId" apps/mobile/lib/features/broadcast/pages/ended_broadcast_page.dart` shows it is used for loading).

**Tests:** `apps/mobile/test/features/broadcast/ended_page_test.dart` — (a) summary path, (b) load-by-id path, (c) unknown id shows a typed not-found, (d) banner dialog does nothing after a switch.

**Watch out for:** holding `manager` (or any manager) in a dialog callback and using it after the await. Capture values, re-check the snapshot.

**Depends on:** Step 32.

---

### Step 34: Point the editor at the manager, delete `StreamManager`, and make the editor widget-owned

**Phase:** P3
**Delivery group:** G7.
**Audit reference:** §3 [M21] "Scope/connection orchestration is over-commanded", Recommended fix (plain async entry/close; commands only at the UI boundary); §3 [H15] "Async operations continue after their owner is disposed" (editor continuation safety); §1.4 "Ownership and lifecycle" row "Navigate away" (no teardown on navigation). Evidence `apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart:68–84`, `apps/mobile/lib/features/broadcast/manager/stream_manager.dart:34–46`, `apps/mobile/lib/locator.dart:156–299`.
**Why (plain words):** The editor and stream managers currently reach into the live scope to start a session, which is why the scope could never be removed. Injecting `ActiveBroadcastManager` into the editor — and creating the editor per page rather than per account — removes the last dependency on registration and means abandoning a draft page cannot leave a stale editor behind. `StreamManager`'s only remaining job would be forwarding `join()`, so it is deleted.

_Simplified from design doc §4.5 (keep `StreamManager` as a thin delegate) because a class whose only job is forwarding one call adds a file, a registration and a test rewrite for nothing; safety preserved because callers use `ActiveBroadcastManager.join` directly and the account-context check lives inside `ActiveBroadcastManager`._

**Files**

| File (exact path from repo root)                                           | Action | Kind    | Required? | Notes                                                                                                            |
| -------------------------------------------------------------------------- | ------ | ------- | --------- | ---------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/manager/stream_manager.dart`           | Delete | manager | Required  | Callers (`rg -n "StreamManager" apps/mobile`) call `ActiveBroadcastManager.join` directly.                       |
| `apps/mobile/test/features/broadcast/stream_manager_test.dart`             | Delete | test    | Required  | It was a commented-out suite; the `join` behaviour is covered by `active_broadcast_manager_test.dart` (Step 30). |
| `apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart` | Modify | manager | Required  | Takes the manager; no scope calls. Widget-owned from now on (Step 57 finishes its retry semantics).              |
| `apps/mobile/lib/features/broadcast/pages/broadcast_editor_page.dart`      | Modify | widget  | Required  | `createOnce` the editor manager with the ready-account collaborators.                                            |

**Pseudo-code**

```
DELETE StreamManager and its registration; every caller becomes
    AWAIT activeBroadcastManager.join(EnterRequest(broadcastId, asListener))

BroadcastEditorManager (created per page)
    new(context, activeBroadcastManager, broadcastHttp, mediaService, local)
    // Step 57 turns create/upload/start/entry into one operation; this step only swaps the dependency

BroadcastEditorPage
    final editor = createOnce(() => BroadcastEditorManager(...ready collaborators...))
    onDispose: editor.dispose()          // createOnce calls dispose automatically
```

**Done when:**

- `grep -rn "LiveScopeManager\|liveScope\|StreamManager" apps/mobile/lib` finds nothing.
- Opening and closing the editor page creates and disposes exactly one editor manager.
- Starting a broadcast from the editor goes through `ActiveBroadcastManager.join`.

**Tests:** editor disposal cancels pending work (in `broadcast_editor_manager_test.dart`); `join` delegation is covered in Step 30's suite.

**Watch out for:** registering the editor in the account scope "because it used to be there". Per-account lifetime means a draft left open on account A would still exist on account B.

**Depends on:** Step 31.

---

### Step 35: Delete the live scope, its readiness signalling and the duplicate paths

**Phase:** P3
**Delivery group:** G7 (Steps 31, 32, 33, 34 and 35 land as one change with a checklist per part: the registration change, page migration and deletions must move together or the build breaks).
**Audit reference:** §1.4 "Migration steps" item 7 (remove `LiveScopeManager`, live scope names, scope polling and `WillSignalReady` session signalling); §3 [M21] "Scope/connection orchestration is over-commanded"; §3 [L05] "Dead paths, fixtures, and documentation obscure the reference implementation", Recommended fix (remove unused implementations, correct lifecycle comments). Evidence `apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart`, `apps/mobile/lib/features/broadcast/widgets/live_broadcast_tab.dart:36–44`, `apps/mobile/lib/features/broadcast/manager/broadcast_timer_manager.dart:21–26`.
**Why (plain words):** Leaving the old owner in place next to the new one guarantees that someone will call it. Deleting it, its readiness signalling and the duplicate live tab is what makes the two-tier structure real rather than aspirational.

**Files**

| File (exact path from repo root)                                       | Action | Kind    | Required? | Notes                                                                                                                                                                      |
| ---------------------------------------------------------------------- | ------ | ------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart`   | Delete | manager | Required  | Replaced by Step 30.                                                                                                                                                       |
| `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart` | Delete | manager | Required  | The old session manager. It was deliberately kept by Step 29 and is deleted here, together with its call sites and tests (`rg -l "LiveSessionManager" apps/mobile`) (D-6). |
| `apps/mobile/lib/features/broadcast/widgets/live_broadcast_tab.dart`   | Delete | widget  | Required  | Duplicate implementation that resolves an unregistered timer.                                                                                                              |
| `apps/mobile/test/features/broadcast/live_scope_manager_test.dart`     | Delete | test    | Required  | Replaced by Step 30's manager suite.                                                                                                                                       |

**Pseudo-code**

```
DELETE live_scope_manager.dart and every reference
    RUN `rg -n "LiveScopeManager|live_scope|signalReady|WillSignalReady" apps/mobile`
    FIX each remaining call site before deleting
DELETE live_session_manager.dart and its tests now that nothing references it
(Step 31 already made the transition owner call manager.closeForAccountExit() directly; there is no adapter to delete)
DELETE the duplicate live tab; the routed live page is the only implementation
Timer manager: already rewritten ONCE in Step 29 (plain methods, one-second rebuild granularity); do not touch it again here
Update lifecycle comments in the remaining files to match what the code does
```

**Done when:**

- `rg -n "LiveScopeManager|WillSignalReady|signalReady|live_:" apps/mobile/lib` returns nothing.
- Only one live-tab implementation exists and it is routed.
- `rg -n "LiveSessionManager" apps/mobile` returns nothing; the UI still shows elapsed time.
- `dart analyze` clean; the two-tier locator test from Step 31 still passes.

**Tests:** `apps/mobile/test/locator_test.dart` (unchanged, still asserts two scopes) and `apps/mobile/test/features/broadcast/broadcast_timer_manager_test.dart` (written in Step 29).

**Watch out for:** deleting the file but keeping a `typedef` or re-export "for compatibility". That keeps the old shape discoverable.

**Depends on:** Steps 32, 33, 34.

---

## 10. Phase P4 — Owner-bound persistence, profile, settings, connection, notifications

**Findings finished here:** H13, H14, H15, M03, M04, M05, M06, M11, M13, M16 (ingestion slice), I02 (native schema gate), plus the M18 recovery-record completion.

**What you will have when this phase is done (3–5 lines):**

- Three owner-bound local services (`BroadcastLocalService`, `ProfileCacheService`, `SettingsLocalService`), each taking `ownerId` in its constructor and never exposing a `Box`; every private row and preference key goes through them.
- ObjectBox writes that cannot duplicate a logical row, and queries that are always closed.
- A profile repository with stable subject identity and correct reference counting, a viewer-qualified profile cache (one key), and an editor that compares what it displays.
- Settings edits sent as field-level PATCHes through one serial queue, so unknown preference keys survive by construction.
- An account connection owner that installs handlers, starts after readiness, reads the credential on connect/reconnect and stops before disposal.

**How to know it's safe to ship:** the P4 test list in §17 passes; two cold profiles fetched concurrently keep distinct identities; B never sees A's `isFollowing`; a foreign-owner draft id is rejected; unknown settings keys survive a field-level PATCH; a connection start released after an exit delivers nothing.

**Order:** 36 → 37 → 38 → 39 → 40 → 41 → 42 → 43 → 44 → 45 → 46. Delivery group **G8** (Steps 36–37) is the storage floor and must land before 38–43. Step 44 (and Step 49) need the live manager (Step 31); Phase R's pure client rewrite (Steps 47, 48, 50) does not and moves earlier (section 6.2).

**What we are deliberately NOT doing in P4:** no reactive favourites redesign beyond Step 59, no feed pagination work (Phase F), no notification inbox (Phase U), no per-account database handle, and no repository/use-case layer per endpoint (audit §7 "Repositories: only where shared identity, local truth, or synchronization requires them").

---

### Step 36: Make the local services owner-bound

**Phase:** P4
**Delivery group:** G8 (Steps 36 and 37 land as one change).
**Audit reference:** §3 [M05] "Local storage isolation is optional rather than enforced", Recommended fix (immutable owner-bound local facade; owner-qualified reads/deletes/upserts and foreign-object rejection); §3 [M06] "ObjectBox writes can duplicate logical rows; queries lack deterministic cleanup", Recommended fix (transactional upserts, unique logical keys, query closure); §3 [H14] "Viewer-specific profile cache state leaks across accounts". Evidence `apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart:20–46,100–104`, `apps/mobile/lib/_shared/services/database.dart:42–47`, `apps/mobile/lib/_shared/services/local_storage.dart`.
**Why (plain words):** The raw boxes and the unrestricted preference API are reachable from every manager, so an owner check is a convention rather than a guarantee. The three existing local services take the captured owner in their constructor, never hand out a `Box`, and qualify every method by owner, so isolation is enforced where the data is touched.

_Simplified from design doc §7 (a generic `AccountLocalData` layer with `isOwnerLive`, `quiesce()` and a kind/logicalId API over typed ObjectBox entities) because audit M05 asks for owner-bound facades on the existing services, not a new generic layer; safety preserved because the services live in the account scope, so scope disposal already stops them, and every method is owner-qualified and rejects foreign rows._

**Files**

| File (exact path from repo root)                                           | Action | Kind    | Required? | Notes                                                                                                |
| -------------------------------------------------------------------------- | ------ | ------- | --------- | ---------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart` | Modify | service | Required  | `new(ownerId, database)`; owner taken from the constructor, never from the caller; no `Box` exposed. |
| `apps/mobile/lib/features/profile/services/profile_cache_service.dart`     | Modify | service | Required  | `new(ownerId, localStorage)`; keys built as `profile.v2/<ownerId>/...`.                              |
| `apps/mobile/lib/features/settings/services/settings_local_service.dart`   | Modify | service | Required  | `new(ownerId, localStorage)`; keys `account.v2/<ownerId>/...`.                                       |
| `apps/mobile/lib/_shared/services/local_storage.dart`                      | Modify | service | Required  | Private account namespaces are reachable only through those three services.                          |
| `apps/mobile/lib/app/composition/account_registrations.dart`               | Modify | config  | Required  | Construct the three services from `AccountContext.accountId`.                                        |
| `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart`    | Modify | test    | Required  | The owner-isolation cases live here (no separate `account_local_data_test.dart`).                    |

**Pseudo-code**

```
BroadcastLocalService (account scope, eager)
    new(ownerId: String, database)
    FUNCTION read/upsert/delete(...)
        QUERY / WRITE only where ownerId matches the constructor value      // caller never supplies the owner
        REJECT a row whose ownerId differs (typed OwnerViolation)
        ALWAYS close each query in finally

ProfileCacheService / SettingsLocalService
    BUILD every key as '<prefix>/<ownerId>/<leaf>'                          // caller supplies only the leaf
    ProfileCacheService.read(subjectId) uses viewerId == ownerId            // a viewer cannot impersonate another

// no AccountLocalData, no isOwnerLive, no quiesce(): the account scope's disposal stops these services,
// and a committed old-account write stays under the old owner's namespace; it is never retargeted
```

**Done when:**

- A row belonging to another owner cannot be read, updated or deleted through any of the three services (typed rejection).
- Every constructed query is closed (test double counts opens and closes; equal).
- Preference keys written by the services all contain the owner segment.
- No `Box` is reachable from a manager (`grep -rn "Box<" apps/mobile/lib/features/*/manager` finds nothing).

**Tests:** extend `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart` — (a) foreign row rejected, (b) query closure on throw, (c) preference key namespacing.

**Watch out for:** exposing the `Box` from a service "for the feature that needs it later". That reopens the hole.

**Depends on:** Steps 18, 26.

---

### Step 37: Make ObjectBox writes transactional with unique logical keys

**Phase:** P4
**Audit reference:** §3 [M06] "ObjectBox writes can duplicate logical rows; queries lack deterministic cleanup", Recommended fix (transactional upserts, unique logical keys, idempotent desired-state setters, `try/finally` query closure); design §7 "Cache, rows and sensitive preferences" item 3. Evidence `apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart:21–40,51–104`, `apps/mobile/lib/features/broadcast/services/broadcast_rows.dart:30–34,76–80`.
**First check (verify):** whether ObjectBox in the pinned `6.0.0-preview.4` supports a unique key with replace-on-conflict (`@Unique(onConflict: ConflictStrategy.replace)`). **If yes**, an upsert is a single `put` and the lookup-plus-transaction code below disappears (review OE-12). If no, use the transaction version below.
**Why (plain words):** Lookup-then-write is not atomic and the business key has no uniqueness, so two concurrent saves can create two rows; and built queries are never closed, which leaks native resources. A compound unique key plus one transaction per mutation makes the desired state idempotent.

**Delivery group:** G8.

**Files**

| File (exact path from repo root)                                           | Action             | Kind    | Required? | Notes                                                                  |
| -------------------------------------------------------------------------- | ------------------ | ------- | --------- | ---------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/services/broadcast_rows.dart`          | Modify             | service | Required  | Add `uniqueKey` (`<ownerId>/<logicalId>`) with `@Index(unique: true)`. |
| `apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart` | Modify             | service | Required  | Transactional upserts, desired-state setters, query closure.           |
| `apps/mobile/lib/objectbox-model.json`                                     | Modify             | config  | Required  | Schema addition; keep existing entity/property/index UIDs.             |
| `apps/mobile/lib/objectbox.g.dart`                                         | Modify (generated) | config  | Required  | Regenerate through ObjectBox tooling only.                             |
| `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart`    | Create             | test    | Required  | Duplicate-prevention matrix.                                           |

**Pseudo-code**

```
Rows
    ADD uniqueKey: String = '<ownerId>/<draftId>'          // encoded, not concatenated blindly
    @Index(unique: true)
    KEEP existing UIDs and enum ordinal order

FUNCTION upsertDraft(draft)
    REQUIRE draft.ownerId == ownerId
    runInTransaction:
        existing = box.query(uniqueKey.equals(draft.uniqueKey)).build().findFirst()
        // query closed in finally
        IF existing != null draft.id = existing.id          // reuse the native row id
        box.put(draft)                                     // insert or update in the same tx
FUNCTION setFavourite(broadcastId, isFavourite)             // desired state, not toggle
    WITH transaction: upsert row with isFavourite as given   // idempotent by construction
FUNCTION dedupeOnUpgrade()                                  // ONLY if assumption A13 says installed builds exist; otherwise delete
    WITH one transaction:
        group by uniqueKey; keep newest lastModified, tie-break highest native row id
        delete the others; then enforce uniqueness
    // never merge different draft operation stages
```

**Done when:**

- Two concurrent `upsertDraft` calls for the same draft leave exactly one row.
- `setFavourite(id, true)` twice leaves one favourite row (no flip-flop).
- Every `build()` in this file has a matching `close()` in `finally`.
- (Only if A13 says installed builds exist) Opening the app over an existing database with duplicates deduplicates deterministically and preserves the surviving draft's fields.

**Tests:** `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart` — (a) concurrent upsert yields one row, (b) desired-state setter is idempotent, (c) query close on exception, (d) (A13 only) dedupe keeps the newest draft.

**Watch out for:** changing an existing index's UID. That is a schema break for installed builds (see Step 80).

**Depends on:** Step 36 (same delivery group).

---

### Step 38: Key the profile cache by viewer and subject

**Phase:** P4
**Audit reference:** §3 [H14] "Viewer-specific profile cache state leaks across accounts", Recommended fix (key viewer-relative state by `(viewerId, subjectId)` **or** split public and relationship records; invalidate legacy cache entries); evidence `apps/mobile/lib/features/profile/services/profile_cache_service.dart:43–51`, `apps/mobile/lib/features/profile/model/dtos.dart:134–145`, `apps/api/src/modules/profile/service.rs:172–189`.
**Why (plain words):** The cached profile stores `isFollowing`, which depends on who is looking, under a key that only names the subject. Account B therefore sees A's follow relationship and can act on the wrong state before a refresh. One key per viewer and subject, holding the whole response, fixes it.

_Simplified from design doc §7 (two keys, a `PublicProfileSnapshot`/`ViewerRelation` split, a tri-state "unknown" follow that disables the button, and reconcile code in `profile_http_service`) because audit H14 offers one option OR the other and the saving from not storing the public part per viewer is negligible with 1–3 accounts on a device; safety preserved because the viewer is part of the key, so B can never read A's relation._

**Files**

| File (exact path from repo root)                                       | Action | Kind    | Required? | Notes                                                                                                                |
| ---------------------------------------------------------------------- | ------ | ------- | --------- | -------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/profile/services/profile_cache_service.dart` | Modify | service | Required  | One key `profile.v2/<viewerId>/<subjectId>` holding the whole response; viewer comes from the constructor (Step 36). |
| `apps/mobile/lib/features/auth/services/legacy_cleanup.dart`           | Modify | service | Optional  | Delete legacy `profile.public_*` keys (shared with Step 16). Skip if A13 finds no installed builds.                  |
| `apps/mobile/test/features/profile/profile_cache_test.dart`            | Create | test    | Required  | Cross-account isolation.                                                                                             |

**Pseudo-code**

```
Cache layout (through the owner-bound ProfileCacheService)
    'profile.v2/<viewerId>/<subjectId>' -> the WHOLE profile response (public part and viewer-relative part together)

FUNCTION read(subjectId) -> Profile?
    RETURN parsed value or null (malformed -> miss, remove that one key)
FUNCTION write(subjectId, response)
    WRITE the whole response under this viewer's key
// no PublicProfileSnapshot, no ViewerRelation, no "unknown" state, no extra reconcile code
```

**Done when:**

- After A follows a subject and switches to B, B's cache read is a miss (not `isFollowing: true`).
- A malformed entry is a miss and only that key is removed.
- Legacy `profile.public_*` keys are gone after a start-up on a device that had them (A13 only).
- `grep -rn "PublicProfileSnapshot\|ViewerRelation" apps/mobile/lib` finds nothing.

**Tests:** `apps/mobile/test/features/profile/profile_cache_test.dart` — (a) A→B isolation, (b) malformed entry is a miss, (c) legacy invalidation (A13 only).

**Watch out for:** a key without the viewer part. Refuse it (Step 16).

**Depends on:** Steps 36, 26.

---

### Step 39: Give the profile repository a stable subject identity

**Phase:** P4
**Audit reference:** §3 [H13] "Cold profile proxies share an empty identity and cannot be released correctly", Recommended fix (acquire using the immutable requested identity, use an ID-bearing loading placeholder, forbid target-ID changes, release against stable ownership); audit §4 "Shared placeholder profile proxy" (simpler alternative: ID-bearing loading state); design §5.7. Evidence `apps/mobile/lib/features/profile/manager/user_profile_manager.dart:44,77–82`, `apps/mobile/lib/features/profile/model/proxies/user_profile_proxy.dart:173–198`.
**Why (plain words):** Every uncached profile acquires the same `Id.empty` proxy and fetching then mutates that identity, so two concurrent cold profiles overwrite each other and the repository's release lookup misses the original key and leaks the entry. Keying the repository by the _requested_ subject and forbidding identity changes fixes both.
_Simplified from design doc §5.7 because only one repository and one proxy type exist; safety preserved because the acquisition key is immutable and release uses it, not the fetched id._

**Files**

| File (exact path from repo root)                                         | Action | Kind          | Required? | Notes                                                |
| ------------------------------------------------------------------------ | ------ | ------------- | --------- | ---------------------------------------------------- |
| `apps/mobile/lib/features/profile/model/proxies/user_profile_proxy.dart` | Modify | freezed model | Required  | `subjectId` final; loading placeholder carries it.   |
| `apps/mobile/lib/features/profile/manager/user_profile_manager.dart`     | Modify | manager       | Required  | Acquire/release by requested id.                     |
| `apps/mobile/lib/features/profile/services/user_profile_repository.dart` | Create | service       | Required  | Small map + refcount; deliberately not an interface. |
| `apps/mobile/test/features/profile/user_profile_repository_test.dart`    | Create | test          | Required  | Concurrent cold-profile matrix.                      |

**Pseudo-code**

```
UserProfileProxy (observable)
    final subjectId : Id                      // set at construction, NEVER reassigned
    _target : PublicProfile?                  // null while loading
    _relationOverride : bool?                 // optimistic follow state (audit §5 item 6 — keep)
    _referenceCount : int
    String get displayName => _target?.name ?? ''     // empty while loading; the widget renders a skeleton
    FUNCTION setTarget(profile)
        REQUIRE profile.id == subjectId       // reject a mismatched response, do not mutate

UserProfileRepository (account scope)
    _bySubject : Map<String, UserProfileProxy>

    FUNCTION acquire(subjectId) -> UserProfileProxy
        REQUIRE subjectId is a valid UUID
        proxy = _bySubject[subjectId] ??= UserProfileProxy(subjectId)
        proxy._referenceCount += 1
        RETURN proxy
    FUNCTION release(proxy)
        proxy._referenceCount -= 1
        IF 0 -> proxy.dispose(); _bySubject.remove(proxy.subjectId)   // stable key, not the target's
        // releasing twice is a programming error: assert in debug

UserProfileManager (widget-owned detail manager)
    profile = repository.acquire(subjectId)
    TRY AWAIT http.profile(subjectId); CHECK page ticket; repository.apply(profile, response)
    FINALLY repository.release(profile) on page dispose                    // exactly once
```

**Done when:**

- Two cold profiles acquired concurrently keep distinct `subjectId`s and both finish correctly.
- After both are released, the repository map is empty (no leak).
- A response for a different id is rejected rather than applied.
- `grep -n "Id.empty" apps/mobile/lib/features/profile` finds nothing.

**Tests:** `apps/mobile/test/features/profile/user_profile_repository_test.dart` — (a) concurrent acquire/fetch/release in reverse order, (b) refcount reaches zero and the map empties, (c) mismatched response rejected, (d) double release asserts.

**Watch out for:** using the fetched entity's id to release. That is the exact bug the audit describes.

**Depends on:** Steps 38, 36.

---

### Step 40: Target the page's own proxy in the public profile page

**Phase:** P4
**Audit reference:** §3 [M04] "Public profile page resolves an unregistered proxy", Recommended fix (target `manager.profile`; do not register every entity proxy globally); evidence `apps/mobile/lib/features/profile/pages/user_profile_page.dart:30–40,58–61`.
**Why (plain words):** The follow-error handler asks DI for a `UserProfileProxy`, but the proxy belongs to a locally created manager, so the page throws when the handler runs. Pointing the handler at the local object removes a latent crash and avoids the temptation to register entity proxies globally.

**Files**

| File (exact path from repo root)                                | Action | Kind   | Required? | Notes                                          |
| --------------------------------------------------------------- | ------ | ------ | --------- | ---------------------------------------------- |
| `apps/mobile/lib/features/profile/pages/user_profile_page.dart` | Modify | widget | Required  | Handler selects the local proxy via `target:`. |
| `apps/mobile/test/features/profile/user_profile_page_test.dart` | Create | test   | Required  | Error-handler test with no DI registration.    |

**Pseudo-code**

```
UserProfilePage (WatchingWidget)
    manager = createOnce(() => UserProfileManager(subjectId: widget.subjectId, ...))
    // created here, so its proxy is page-owned

    registerHandler(
        target: manager.profile,                          // local target, not a DI lookup
        select: (UserProfileProxy p) => p.followCommand.errors,
        handler: (context, error, _) { showFollowError(context, error!.error); },
    )
```

**Done when:**

- With no `UserProfileProxy` registered in DI, triggering a follow error shows one message and does not throw.
- The page creates one manager per page instance and disposes it on unmount.
- No entity proxy is registered globally (`grep -rn "registerSingleton<UserProfileProxy>" apps/mobile/lib` finds nothing).

**Tests:** `apps/mobile/test/features/profile/user_profile_page_test.dart` — (a) follow error handled without DI, (b) page dispose releases the proxy, (c) no locator lookup occurs during build.

**Watch out for:** registering the proxy in the account scope "so the page can find it". That reintroduces global entity state and leaks across pages.

**Depends on:** Step 39.

---

### Step 41: Compare and validate the profile editor correctly

**Phase:** P4
**Audit reference:** §3 [M11] "Profile editor compares different types and ignores bio validity", Recommended fix (compare unwrapped values consistently, include bio validity, align edit validation with the backend, correct misleading comments); evidence `apps/mobile/lib/features/profile/manager/profile_editor_manager.dart:34–39,54–69,123–129`.
**Why (plain words):** Bio strings are compared with a `MultiLineString` value object, so an unchanged bio looks changed; validity only checks the name, so an invalid bio can be saved as a deletion; and comments claim a relaxed name type that the field does not use. All three are one comparison-and-validity change.

**Files**

| File (exact path from repo root)                                       | Action | Kind    | Required? | Notes                                                 |
| ---------------------------------------------------------------------- | ------ | ------- | --------- | ----------------------------------------------------- |
| `apps/mobile/lib/features/profile/manager/profile_editor_manager.dart` | Modify | manager | Required  | Unwrapped comparisons, bio validity, honest comments. |
| `apps/mobile/lib/features/profile/widgets/profile_editor_modal.dart`   | Modify | widget  | Required  | Disable save on invalid bio.                          |
| `apps/mobile/test/features/profile/profile_editor_manager_test.dart`   | Create | test    | Required  | Diff matrix.                                          |

**Pseudo-code**

```
Original values captured at construction (plain Strings)
    _originalName = profile.name.value or ''
    _originalBio  = profile.bio ?? ''                     // the RAW text, not the value object

isDirty
    name.trim() != _originalName.trim()  OR  bioText != _originalBio     // same types both sides

isValid
    name policy passes AND (bio is empty OR bio policy passes)            // empty bio = intentional delete
    // an INVALID bio disables save; it is not treated as "clear the bio"

canSave = isDirty AND isValid AND NOT isSaving

Comments
    REWRITE any comment claiming a relaxed name type while the field is FullName
```

**Done when:**

- Untouched bio yields `isDirty == false`.
- An over-long bio disables save instead of scheduling a deletion.
- A two-character valid name can be saved (server-valid).
- Save is disabled while a save runs.

**Tests:** `apps/mobile/test/features/profile/profile_editor_manager_test.dart` — (a) unchanged bio not dirty, (b) invalid bio blocks save, (c) empty bio allowed, (d) two-character name accepted.

**Watch out for:** trimming in one comparison and not the other. Compare like with like and state the normalisation in one place.

**Depends on:** Step 12.

---

### Step 42: Send settings as field-level patches through one serial queue

**Phase:** P4
**Audit reference:** §3 [M13] "Settings writes race and overwrite unknown preference keys", Recommended fix (serialize/coalesce settings mutations, version reads, reconcile responses against newer edits, preserve unknown keys **or** use field-level patches); design §5.7 `settings edit field`. Evidence `apps/mobile/lib/features/settings/manager/settings_manager.dart:90–145,246–251,299–315`, `apps/mobile/lib/features/settings/model/dtos.dart:12–19,64–72`.
**Why (plain words):** Each toggle sends a whole preferences object from its own command, a late GET can replace an optimistic edit, and unknown JSON keys are dropped during conversion — so two quick toggles lose one, and a newer client's preference can be deleted by an older one. Sending **only the changed fields**, one request at a time, fixes both: unknown keys survive by construction because they are never sent.

_Simplified from design doc §5.7 (a client overlay with per-field revisions, a flush loop with four failure branches, backend `baseVersion`, a 409 conflict and a version column) because audit M13 says "preserve unknown keys OR use field-level patches"; safety preserved because the backend deep-merges only the supplied fields (Step 72), so an old client cannot delete a key it never sent._

**Files**

| File (exact path from repo root)                                  | Action | Kind    | Required? | Notes                                                                                    |
| ----------------------------------------------------------------- | ------ | ------- | --------- | ---------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/settings/manager/settings_manager.dart` | Modify | manager | Required  | One serial queue; each job sends only the changed fields and adopts the returned object. |
| `apps/mobile/lib/features/settings/model/dtos.dart`               | Modify | model   | Required  | A patch DTO containing only the changed fields.                                          |
| `apps/mobile/test/features/settings/settings_manager_test.dart`   | Create | test    | Required  | Race and patch-shape matrix.                                                             |

**Pseudo-code**

```
SettingsManager (account scope)
    _serial : FIFO tail                  // the ONLY concurrency control
    _editsQueued : int                   // > 0 while a patch is queued or in flight

FUNCTION editField(field, value)
    _editsQueued += 1
    PUBLISH optimistic value for that field
    ENQUEUE on _serial:
        response = AWAIT http.patch({ field: value })        // ONLY the changed field(s)
        CHECK owner/generation
        ON success   ADOPT the returned object as canonical
        ON rejection REVERT only that field; show one typed error
        ON failure   KEEP the optimistic value as "unsaved"; do NOT auto-retry
        _editsQueued -= 1

FUNCTION load()
    response = AWAIT http.get()
    CHECK owner/generation
    IF _editsQueued > 0 IGNORE the GET result       // a late GET must not clobber a queued edit
    ELSE adopt it
// no overlay, no revision map, no baseVersion, no 409, no "changed elsewhere" UI
```

**Done when:**

- Toggling two fields quickly sends two ordered PATCHes, each carrying only its own field, and no value is lost.
- A slow GET while an edit is queued does not overwrite the edit.
- A PATCH body never contains a field the user did not change.
- A rejection reverts only the rejected field.

**Tests:** `apps/mobile/test/features/settings/settings_manager_test.dart` — (a) two rapid toggles, (b) late GET ignored while an edit is queued, (c) PATCH body contains only the changed field, (d) rejection keeps sibling edits.

**Watch out for:** sending the whole preferences object "to be safe". That is how unknown keys disappear.

**Depends on:** Steps 36, 26.

---

### Step 43: Fence profile and settings continuations against owner death

**Phase:** P4
**Audit reference:** §3 [H15] "Async operations continue after their owner is disposed", Recommended fix (cancel requests where supported; check owner/session generation after every await; invalidate mutation and undo continuations on disposal); evidence `apps/mobile/lib/features/profile/manager/profile_editor_manager.dart:109–137`, `apps/mobile/lib/features/settings/manager/settings_manager.dart:246–251`, `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:75–107,220–224`.
**Why (plain words):** A profile upload followed by a patch can cross an account transition, and disposed notifiers can still be written. Disposing a command is not cancelling its body, so each continuation needs an explicit liveness check — the pattern that makes upload-then-patch safe and that notes autosave will depend on later.

**Files**

| File (exact path from repo root)                                       | Action | Kind    | Required? | Notes                                     |
| ---------------------------------------------------------------------- | ------ | ------- | --------- | ----------------------------------------- |
| `apps/mobile/lib/features/profile/manager/profile_editor_manager.dart` | Modify | manager | Required  | Ticket before upload, check before PATCH. |
| `apps/mobile/lib/features/settings/manager/settings_manager.dart`      | Modify | manager | Required  | Owner checks after each await.            |
| `apps/mobile/lib/features/profile/manager/user_profile_manager.dart`   | Modify | manager | Required  | Page ticket after fetch.                  |
| `apps/mobile/test/features/profile/profile_editor_manager_test.dart`   | Modify | test    | Required  | Continuation matrix.                      |

**Pseudo-code**

```
FUNCTION save()
    ticket = _ticket  (page-scoped, incremented on dispose and on each new save)
    IF imageChanged
        uploaded = AWAIT upload(image)
        CHECK ticket live AND owner live           // a switch here must not PATCH
    CHECK ticket live
    result = AWAIT patch(values)
    CHECK ticket live
    APPLY result to presentation and cache
    // ON dispose: _ticket += 1, cancel in-flight request where supported

Every manager
    closed = false; dispose() sets closed = true
    AFTER each await: IF closed RETURN        // no notifier write, no follow-up request, no navigation
```

**Done when:**

- Disposing the editor during an upload issues no PATCH afterwards.
- A late profile GET after dispose writes no notifier.
- Settings writes after the manager is disposed are rejected.
- The test double records zero requests after disposal.

**Tests:** `apps/mobile/test/features/profile/profile_editor_manager_test.dart` — (a) dispose between upload and patch, (b) late GET after dispose writes nothing, (c) a queued settings patch after dispose sends nothing.

**Watch out for:** relying on `command.dispose()` to stop the body. It does not; the body continues.

**Depends on:** Steps 39, 41, 42.

---

### Step 44: Add the account connection owner

**Phase:** P4
**Audit reference:** §3 [H06] "No production path starts realtime delivery", Recommended fix (an explicit account connection owner initializes handlers before connecting, updates credentials, and closes on account exit); §3 [M03] "Scoped credentials and socket tokens stay stale", Recommended fix (stable account credential holder; deliberate connection credential refresh; avoid unregister/re-register for ordinary state changes); design §5.6. Evidence `apps/mobile/lib/locator.dart:170–185,290–299`, `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart:72–129`.
**Why (plain words):** Nothing in production ever calls `connect()` or `listen()`, and `SocketClient` captures the credential passed to the registrar, so a rotated token never reaches the socket. One account-owned coordinator installs handlers first, starts after readiness and stops before disposal; the socket asks a **credential supplier** for the current token each time it connects or reconnects.

_Simplified from design doc §5.6 (a vault revision stream and an `onRevision` handler that stops and restarts the socket on every token refresh) because a WebSocket authenticates at the handshake, so a refreshed token matters only at the next connect (audit M03: "deliberate refresh"); safety preserved because the supplier always returns the committed credential, so a reconnect never uses a stale token._

**Files**

| File (exact path from repo root)                                            | Action | Kind    | Required? | Notes                                                                                                                  |
| --------------------------------------------------------------------------- | ------ | ------- | --------- | ---------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/auth/services/account_connection_owner.dart`      | Create | service | Required  | Lifecycle only; owns no media state and no credential stream.                                                          |
| `apps/mobile/lib/_shared/services/socket_client.dart`                       | Modify | service | Optional  | Already takes a credential supplier and `start`/`stop` if Step 47 has landed; only make the change here if it has not. |
| `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart` | Modify | service | Required  | `install()`/`detach()` idempotent, composition-callable.                                                               |
| `apps/mobile/lib/app/composition/account_registrations.dart`                | Modify | config  | Required  | Register and start after `ready`.                                                                                      |
| `apps/mobile/test/features/auth/account_connection_owner_test.dart`         | Create | test    | Required  | Start/stop matrix.                                                                                                     |

**Pseudo-code**

```
AccountConnectionOwner (account scope)
    new(context, socket, adapters, store)                    // `store` is a constructor parameter (D-9)
    _stopping = false
    _stopResult : Future<void>?
    credentialSupplier = () => store.usableCredential(context.accountId)?.accessToken   // committed value each call

FUNCTION initialize()
    REQUIRE context live AND not stopping
    FOR each adapter: adapter.install()                     // handlers BEFORE any connection attempt

FUNCTION start()                                            // called after `ready` AND verified
    REQUIRE context live, ready, verified, credential usable
    AWAIT socket.start(credentialSupplier)                  // supplier is read on connect and on every reconnect
    CHECK context live; IF stale AWAIT socket.stop(); RETURN   // publish nothing
    IF start failed PUBLISH connectionUnavailable           // HTTP session stays valid

FUNCTION stop()
    _stopping = true                                        // rejects start and further callbacks
    AWAIT socket.stop()
    FOR each adapter: AWAIT adapter.detach()
    // completes only when delivery has actually stopped; repeats join the same future
// no onRevision, no revision stream, no restart on token refresh
```

**Done when:**

- After login, handlers are installed before the first connection attempt (test asserts call order).
- A reconnect after a token refresh uses the new token (supplier returns the committed revision) with no socket restart in between.
- `stop()` twice awaits one completion and no further delivery occurs.
- Account exit awaits `stop()` before the socket is disposed.
- `grep -rn "credentialRevisions\|onRevision" apps/mobile/lib` finds nothing.

**Tests:** `apps/mobile/test/features/auth/account_connection_owner_test.dart` — (a) install-before-start order, (b) supplier read at connect, (c) late start after stop publishes nothing, (d) stop idempotent.

**Watch out for:** holding the token in a field. The supplier reads the committed credential each time — that is the fix for the captured-token defect.

**Depends on:** Steps 31, 10.

---

### Step 45: Fix the push payload and platform adapters at the root; render only in the foreground

**Phase:** P4
**Audit reference:** §3 [H24] "Push and durable notifications are not implemented end-to-end" (root platform adapter part only; the coordinator is gated on the backend inbox contract, audit §2 DI map and §6); §3 [M16] "Push payloads, channels, and tap lifecycle disagree", Recommended fix (contract fixtures, recipient attribution, validated destinations, pending-intent queue, **launch-detail handling**, foreground-only local rendering policy); design §5.10. Evidence `apps/mobile/lib/main.dart:6–23`, `apps/mobile/lib/_shared/services/push_notifications_service.dart:13–36`, `apps/mobile/lib/_shared/services/local_notifications_service.dart:22–34,85–126`, `apps/api/src/infrastructure/push/model.rs:17–28,87–95`.
**Why (plain words):** Firebase is never initialised, no consumer exists for foreground/open/token-refresh messages, local notifications are shown from every background callback (duplicating the OS notification), and the payload fields the backend actually sends (`deep_link`, `user_id`) are not the ones the app reads (`route`). This step fixes the root adapters and the payload only. **`NotificationCoordinator` is not created here**: the audit says "when backend contract exists", so it is created in Step 71, after the inbox exists (A-7). This step is the whole remediation-track notification work; Steps 70, 71 and 73 are the separate Feature track.

_Simplified from design doc §5.10 (coordinator, device-token association and unread state created before any inbox exists) because the audit gates the coordinator on the backend contract; safety preserved because taps are validated, attributed to a recipient and held in the single pending slot (Step 22) rather than navigated from a platform callback._

**Files**

| File (exact path from repo root)                                    | Action | Kind    | Required? | Notes                                                                                                                                                                            |
| ------------------------------------------------------------------- | ------ | ------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/_shared/services/push_notifications_service.dart`  | Modify | service | Required  | Read `deep_link`/`user_id` + legacy `route` during the compatibility window; expose taps and token changes as streams. Writes validated taps into the pending slot from Step 22. |
| `apps/mobile/lib/_shared/services/local_notifications_service.dart` | Modify | service | Required  | Foreground-only `show`; channel ids aligned with the backend; reads the app-launch details on cold start; no direct navigation.                                                  |
| `apps/mobile/lib/main.dart`                                         | Modify | config  | Required  | Initialise only when platform config exists; ingest the initial message **and** the local-notification launch details.                                                           |
| `apps/mobile/test/_shared/services/push_payload_test.dart`          | Create | test    | Required  | Payload contract fixtures.                                                                                                                                                       |

(Step 22 owns the pending slot and router consumption; this step owns the platform adapters, per D-18.)

**Pseudo-code**

```
Root adapter (no BuildContext, no account lookup)
    initialize() -> IF no platform config: publish notificationsUnavailable, RETURN
        AWAIT Firebase.initializeApp()
        SUBSCRIBE onMessage / onMessageOpenedApp / onTokenRefresh
        initial = AWAIT getInitialMessage()          // terminated-launch tap (push)
        launch  = AWAIT localNotifications.getNotificationAppLaunchDetails()   // terminated-launch tap (local)  (A-4, M16)
        FEED both through the same parse -> pendingSlot.set(...)
    onMessageOpenedApp(m) -> pendingSlot.set(parseTap(m.data))
    parseTap(data)
        ACCEPT deep_link + user_id; ACCEPT legacy route during the compatibility window only
        VALIDATE through the Step 22 allowlist; REJECT malformed; never navigate here

Local rendering policy
    SHOW a local notification ONLY when the app is in the foreground
    NEVER show from a background/terminated handler            // the OS already rendered it
    CHANNEL ids must match the backend's ids (fixture-tested)
```

**Done when:**

- A payload with `deep_link` + `user_id` produces a validated pending destination; one with a bogus scheme is rejected.
- A cold start from a push tap and a cold start from a local-notification tap both end in the same pending slot.
- Foreground delivery shows exactly one local notification; a background callback shows none.
- Channel ids equal the backend's ids (fixture test).
- Without Firebase config the app still signs in and shows "notifications unavailable".
- No `NotificationCoordinator` exists yet.

**Tests:** `apps/mobile/test/_shared/services/push_payload_test.dart` — (a) backend payload parses, (b) legacy `route` accepted only in the window, (c) malformed rejected, (d) channel ids match fixtures, (e) launch details feed the slot.

**Watch out for:** navigating from the platform callback. Everything goes through the pending slot (Step 22).

**Depends on:** Step 22 (not Step 44: ingestion needs no account connection).

---

### Step 46: Record the ObjectBox preview risk and gate native schema changes

**Phase:** P4
**Audit reference:** §3 [I02] "Dependency risk is explicit, not proven obsolescence", Recommended fix (document preview acceptance and migration policy; standardize mocking when tests are repaired; keep the strict analyzer baseline); evidence `apps/mobile/pubspec.yaml:38–39,55–62`.
**Why (plain words):** The database, its native libraries and its generator all sit on `6.0.0-preview.4`, and two mocking libraries coexist. Neither is a proven defect, but both need a written policy, or the first schema change silently becomes a release risk.

_Simplified from design doc §7 (a checked-in UID map and a `schema_ids_test`) because the audit asks for a documented policy and a gate, not a test file to maintain; safety preserved because the gate is a written rule plus one line in `docs/release-checklist.md` (Step 80), and the generator diff is reviewed for UID churn._

**Files**

| File (exact path from repo root)       | Action | Kind   | Required? | Notes                                                 |
| -------------------------------------- | ------ | ------ | --------- | ----------------------------------------------------- |
| `apps/mobile/docs/objectbox-policy.md` | Create | config | Required  | Preview acceptance, migration rules, rollback stance. |
| `apps/mobile/README.md`                | Modify | config | Optional  | Link the policy from the top-level readme.            |

**Pseudo-code**

```
Policy document contents
    ACCEPTED RISK: objectbox/objectbox_flutter_libs/objectbox_generator pinned to preview.4
    MIGRATION RULES
        never delete objectbox-model.json or edit UIDs by hand
        additions only through the generator; review the diff for UID churn (existing UIDs unchanged, one handle, preview risk documented)
        enum ordinal order is persisted data: do not reorder without an explicit migration
        uniqueness/enum changes require the upgrade check in docs/release-checklist.md BEFORE release
    ROLLBACK STANCE
        forward-only for credentials; local rows need a compatible reader
        never delete the store to "fix" a migration
    MOCKING
        new and repaired tests use Mocktail; untouched Mockito tests stay
```

**Done when:**

- The policy document exists and states the four rules above without hedging.
- No `pubspec.yaml` version changed.

**Tests:** none (policy step). The schema gate is the release-checklist line (Step 80).

**Watch out for:** "fixing" a migration problem by deleting the model file. That is a data-loss path the design explicitly forbids.

**Depends on:** Step 37.

---

## 11. Phase R — Realtime protocol alignment

**Findings finished here:** C04, H06 (final), H07, M10 (mobile half), and the M03 handoff completion. _Steps 47, 48 and 50 now appear before Phase P3 (delivery group G9); only Step 49 remains in this section._

**What you will have when this phase is done (3–5 lines):**

- A raw-WebSocket client that speaks the backend's actual `{event, data}` frames over `/ws`, with heartbeat, bounded reconnect and typed decoding.
- Handlers installed before connecting and detached on stop, so delivery cannot silently be inactive.
- One coherent connection phase feeding the broadcast manager, with disconnection reasons preserved and terminal reasons never retried.
- Event payloads decoded strictly, with dates parsed as RFC3339 and "ended" signals kept distinct from rich REST summaries.

**How to know it's safe to ship:** a local backend run shows the handshake, one heartbeat cycle, a forced disconnect that reconnects within bounds, and a host-ended event that ends the session without a retry loop. Until this passes, ship with realtime explicitly unavailable (a visible state, not a silent one).

**Order:** 47 → 48 → 49 → 50 in the numbered list, but delivery group **G9** (Steps 47, 48, 50: the pure client and adapter rewrite) depends on P1 only and goes **before P3** (audit §8: transport before live teardown). Only Step 49 needs the live manager (Step 30), and Step 44 (the connection owner) needs Step 31.

**What we are deliberately NOT doing in R:** no Socket.IO acknowledgement emulation, no room-event replay substitute for the missing durable inbox, no third-party realtime SDK, and no claim that reconnect provides missed-message recovery.

---

### Step 49: Reconcile reconnection into one phase and honour terminal reasons

**Phase:** R
**Audit reference:** §3 [H07] "Reconnection has stale credentials, conflicting state, and ignored terminal reasons", Recommended fix (one phase machine, preserve disconnect reasons, let the SDK reconnect first, obtain fresh entry credentials when a full reconnect requires them, reconcile after transport return); §3 [M03] "Scoped credentials and socket tokens stay stale", part "deliberate connection credential refresh"; evidence `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart:276–289,307–384`, `apps/mobile/lib/_shared/services/livekit_client.dart:266–278`.
**Why (plain words):** Session `state` and `status` can contradict each other, failed initial connections paint over later events, and disconnection reasons are discarded — so a kick or room deletion is retried as if it were a network blip, and a long session retries with an expired media token. Mapping transport events into the single `LivePhase` and classifying terminal reasons removes both.

**Files**

| File (exact path from repo root)                                           | Action | Kind          | Required? | Notes                                                |
| -------------------------------------------------------------------------- | ------ | ------------- | --------- | ---------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/manager/active_broadcast_manager.dart` | Modify | manager       | Required  | Consume connection/room phases; classify terminal.   |
| `apps/mobile/lib/_shared/services/livekit_client.dart`                     | Modify | service       | Required  | Expose typed disconnect reasons; no state painting.  |
| `apps/mobile/lib/features/broadcast/model/live_snapshot.dart`              | Modify | freezed model | Required  | Add `LiveFailure.terminal(reason)` and retryability. |
| `apps/mobile/test/features/broadcast/live_reconnect_test.dart`             | Create | test          | Required  | Reason matrix.                                       |

**Pseudo-code**

```
FUNCTION onRoomEvent(event)
    SWITCH event
        reconnecting      -> IF phase == live PUBLISH reconnecting        // SDK owns the first attempt
        reconnected       -> AWAIT reconcileRosterAndFeeds(); PUBLISH live
        disconnected(r)   -> classify(r)
        participantLeft   -> roster delta (Step 54)
        terminated        -> terminal(hostEnded | kicked | roomDeleted)

FUNCTION classify(reason)
    IF reason in {kicked, roomDeleted, hostEnded, authRejected}
        CANCEL pending retries
        AWAIT activeBroadcastManager.leaveOrEnd(terminal: reason)        // close once, keep summary
        PUBLISH ended(reason)
    ELSE IF reason == networkLoss
        PUBLISH reconnecting                                            // no competing loop
        IF transport stays down beyond budget
            AWAIT refreshEntryCredentials()                              // fresh media token, never the old one
            RETRY once; on failure PUBLISH failed(retryable: true)

Unreachable states
    ASSERT a published phase never moves from ending back to live
    ASSERT a failed initial connect does not later publish live without a new join
```

**Done when:**

- A forced kick ends the session once and never reconnects.
- A network drop publishes `reconnecting` and recovers to `live` without a duplicate session.
- A retry after a long outage asks for fresh entry credentials rather than reusing the old media token.
- `LivePhase` transitions are asserted monotone per operation.

**Tests:** `apps/mobile/test/features/broadcast/live_reconnect_test.dart` — (a) kick is terminal, (b) network reconnect rejoins `live`, (c) expired-token retry fetches new credentials, (d) no `live` after `ending`.

**Watch out for:** two independent listeners updating state. One listener feeds the manager; the manager publishes.

**Depends on:** Steps 30, 44, 47.

---

## 12. Phase B — Broadcast and backend contracts

**Findings finished here:** H05, H16, H17, H18, H19, H20, M07, plus the mobile halves of H20's logical keys.

**What you will have when this phase is done (3–5 lines):**

- End and cohost-removal requests and responses that match what the backend actually sends and expects, with a checked-in fixture that both the Rust and Dart tests assert against.
- A roster that understands "present now" versus "attendance history", pages by cursor until the present set is complete, and reconciles deltas over snapshots; hosts get live deltas, listeners pull the roster when they open it and refresh the count on a timer.
- Backend cohost removal that atomically revokes role, presence and media permission, and broadcast transitions that are safe to retry.
- An editor whose create → upload → start → entry is one operation with honest retry, and mutations that carry a stable logical key.
- Favourites backed by one account-level reactive source instead of disconnected snapshots.

**How to know it's safe to ship:** backend contract tests pass in `cargo test`; a cohost removal in an integration test leaves no publish permission; an end call with a failed response still closes the session locally (Step 29); the editor's retry either completes entry or says exactly which phase failed.

**Order:** 51 → 52 → 53 → 54 → 55 → 56 → 57 → 58 → 59. Steps 51 and 52 are a pair (client expects what the server sends). Step 54 depends on Step 30's session ownership. Delivery group **G10** is Step 57 plus the mobile half of Step 58 (the D-1 cycle is gone because Step 58 is mobile-only after OE-25).

**What we are deliberately NOT doing in B:** no new broadcast endpoints beyond what the audit names, no general event-sourcing layer, no retry framework, and no roster UI redesign beyond correctness.

---

### Step 51: Fix the end and cohost-removal contracts on the client

**Phase:** B
**Audit reference:** §3 [H05] "Broadcast end and cohost-removal contracts are wrong", Recommended fix (align end DTO/enrichment; make cohost removal return void and send the actual JSON body; add backend-produced fixtures); evidence `apps/mobile/lib/features/broadcast/model/dtos.dart:124–136`, `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart:181–186,244–255`, `apps/api/src/modules/broadcast/dto.rs:185–193,537–557`.
**Why (plain words):** The client requires `creatorName` in the end response, which the backend does not send, so a successful end becomes a client error; and cohost removal sends a query parameter instead of the JSON body `removeFromRoom` while expecting a DTO from an endpoint that returns no data. Both turn success into failure.

**Files**

| File (exact path from repo root)                                                                            | Action | Kind          | Required? | Notes                                                                                                             |
| ----------------------------------------------------------------------------------------------------------- | ------ | ------------- | --------- | ----------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/model/dtos.dart`                                                        | Modify | freezed model | Required  | End response matches the backend's fields exactly.                                                                |
| `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart`                                   | Modify | service       | Required  | `removeCohost` returns `Future<void>`, sends a JSON body, and takes a **required** `removeFromRoom` (no default). |
| `apps/mobile/lib/features/broadcast/widgets/add_cohost_modal.dart` (and the cohost-removal modal beside it) | Modify | widget        | Required  | Passes the user's explicit "remove from room" vs "keep listening" choice.                                         |
| `apps/mobile/test/features/broadcast/broadcast_contract_test.dart`                                          | Create | test          | Required  | Fixtures captured from the backend.                                                                               |
| `apps/mobile/test/features/broadcast/broadcast_dto_test.dart`                                               | Modify | test          | Required  | Update end-response expectations.                                                                                 |

**Pseudo-code**

```
EndBroadcastResponseDto (mirror the backend exactly)
    broadcastId, broadcastTitle, broadcastImageUrl?, creatorId,
    endedReason, endedAt (RFC3339), durationSecs, totalParticipants, recordingEnabled
    // NO creatorName: it is not in the payload
    // display enrichment (creator name) comes from the local profile/proxy, not this DTO

FUNCTION end(broadcastId) -> BroadcastSummary
    response = AWAIT post('/broadcasts/$id/end')
    summary = summaryFrom(response, creatorName: localNameOrEmpty)
    RETURN summary                       // success is success even without a name

FUNCTION removeCohost(broadcastId, cohostId, {required bool removeFromRoom}) -> void   // NO default (A-2, H05)
    AWAIT delete('/broadcasts/$id/cohosts/$cohostId', body: { removeFromRoom: removeFromRoom })
    // parse NOTHING; the backend returns no body
    // the caller (the removal modal) passes the user's explicit choice; "keep listening" is a real option and must never be
    // silently turned into eviction by a default (audit H05: "Keep listening can default to eviction")
```

**Done when:**

- An end response without `creatorName` decodes and shows a summary.
- `removeCohost` sends `{"removeFromRoom": <the user's choice>}` in the body and none in the query; the modal offers both choices and passes the selected one.
- A 204/empty body from cohost removal does not throw.
- The contract fixture is the checked-in file from Step 52, asserted by both sides.

**Tests:** `apps/mobile/test/features/broadcast/broadcast_contract_test.dart` — (a) end fixture decodes, (b) empty cohost body accepted, (c) request body shape asserted, (d) missing name renders a fallback, (e) `removeCohost` cannot be called without an explicit `removeFromRoom` (compile-time) and "keep listening" sends `false`.

**Watch out for:** making `creatorName` optional and _still_ sending a `?removeFromRoom=` query. Both halves of the contract must match.

**Depends on:** Step 29.

---

### Step 52: Make the backend end/cohost contract explicit and fixture-tested

**Phase:** B
**Audit reference:** §3 [H05] "Broadcast end and cohost-removal contracts are wrong" (backend half; "add backend-produced fixtures"); evidence `apps/api/src/modules/broadcast/dto.rs:185–193,537–557`, `apps/api/src/modules/broadcast/handlers.rs:327–340`.
**Why (plain words):** The client cannot be correct if the server's contract drifts. This step pins the response shapes with a Rust test and checks a JSON fixture into the repo that **both** sides assert against, so drift in either direction fails a test.

_Simplified from design doc §6 (a Rust test that writes the fixture, plus a 400 rule for the wrong query parameter) because a test that writes the file the Dart test then reads makes drift undetectable, and a 400 rule is not required by the audit; safety preserved because the fixture is checked in and the Rust test asserts the serialized output equals it._

**Files**

| File (exact path from repo root)             | Action | Kind        | Required? | Notes                                                                                                                                                                                                               |
| -------------------------------------------- | ------ | ----------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `<fixtures>/broadcast-end.json`              | Create | config      | Required  | Checked-in fixture. `<fixtures>` is the folder where the repo already keeps shared files (verify; same folder as Step 12). It is **not** named `account-contract-fixtures`, because it holds broadcast data (D-22). |
| `apps/api/src/modules/broadcast/dto.rs`      | Modify | Rust module | Required  | Document the contract; keep `EndBroadcastResponse` fields as-is (they are sufficient).                                                                                                                              |
| `apps/api/src/modules/broadcast/handlers.rs` | Modify | Rust module | Required  | Cohost removal returns `204`/empty explicitly.                                                                                                                                                                      |
| `apps/api/tests/broadcast_contract.rs`       | Create | test        | Required  | Serialization assertions against the checked-in fixture.                                                                                                                                                            |

**Pseudo-code**

```
EndBroadcastResponse
    KEEP existing fields (broadcastId, broadcastTitle, broadcastImageUrl, creatorId,
                          endedReason, endedAt, durationSecs, totalParticipants, recordingEnabled)
    DO NOT add creatorName; display enrichment belongs to the client's profile source
    ADD a doc line: "asserted against <fixtures>/broadcast-end.json by the API and mobile tests"

RemoveCohost handler
    READ body RemoveCohostRequest { removeFromRoom: bool }
    RETURN 204 No Content on success
    // no special 400 for a query parameter: the audit does not ask for one

Test (apps/api/tests/broadcast_contract.rs)
    SERIALIZE a representative EndBroadcastResponse; ASSERT it equals the checked-in fixture (it never writes the fixture)
    ASSERT cohost removal returns no body
Dart test (Step 51) decodes the SAME file
```

**Done when:**

- `cargo test -p api --test broadcast_contract` passes.
- Editing a field in the fixture makes the Rust test fail, and the Dart test fail, until both sides agree (prove it once, then revert).
- Cohost removal returns an empty body with status 204.

**Tests:** `apps/api/tests/broadcast_contract.rs` — (a) end response equals the fixture, (b) 204 with no body on removal.

**Watch out for:** "helpfully" adding `creatorName` to the response to satisfy the old client. That would create a second source for display identity (M12).

**Depends on:** nothing in mobile; can run in parallel with Step 51.

---

### Step 53: Define the roster audience and a listener refresh policy

**Phase:** B
**Audit reference:** §3 [H16] "Participant UI does not implement the backend roster contract", Recommended fix (define present-only versus attendance history, correct the event audience **or** listener refresh policy); evidence `apps/api/src/modules/broadcast/service.rs:931–967,1073–1108`.
**Why (plain words):** Roster events currently target hosts and cohosts, so ordinary listeners can never keep an accurate list even with a working transport. The audit allows either a wider audience or a deliberate listener refresh policy. This step chooses the **refresh policy**, because sending a nudge to every listener on every join or leave is O(n) messages per join and O(n²) churn in a large room.

_Simplified from design doc §6 (a roster nudge plus a count event sent to every listener) because audit H16 allows a listener refresh policy; safety preserved because listeners still get an accurate roster (pulled when they open it) and an accurate count (refreshed on a timer and after reconnect), and no listener event is added._

**Files**

| File (exact path from repo root)            | Action | Kind        | Required? | Notes                                                                                                                         |
| ------------------------------------------- | ------ | ----------- | --------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `apps/api/src/modules/broadcast/events.rs`  | Modify | Rust module | Required  | Audience rule per event, as code with a comment.                                                                              |
| `apps/api/src/modules/broadcast/service.rs` | Modify | Rust module | Required  | Join/leave deltas to hosts and cohosts carry the authoritative present count; the room's REST summary exposes the same count. |
| `apps/api/tests/broadcast_roster_events.rs` | Create | test        | Required  | Audience matrix.                                                                                                              |

**Pseudo-code**

```
Audience policy (decide once, document once)
    participantJoined / participantLeft -> host + cohosts ONLY (as today), with the authoritative presentCount
    roleChanged                         -> targeted: the affected user + host/cohosts
    listeners                           -> NO new event. They PULL:
                                           roster: when the listener opens the participant list
                                           count:  refreshed from the room's REST summary every ~30 s and on reconnect

Event payload
    INCLUDE broadcastId, the affected userId where relevant, and the authoritative
            presentCount (joined AND active), never cumulative attendance
```

**Done when:**

- A host receives full join/leave detail with the present count.
- The REST summary returns the present count, not cumulative joins.
- No listener receives a per-join message.
- The policy is written in a comment above the emitter, not only in the test.

**Tests:** `apps/api/tests/broadcast_roster_events.rs` — (a) listener receives no join/leave event, (b) host receives detail, (c) count excludes departed users, (d) role change is targeted.

**Depends on:** nothing in mobile.

**Watch out for:** adding a "roster changed" broadcast to listeners later because it feels more real-time. That is the O(n²) cost this step avoids.

---

### Step 54: Page and reconcile the roster on the client

**Phase:** B
**Audit reference:** §3 [H16] "Participant UI does not implement the backend roster contract", Recommended fix (traverse cursor pages, use authoritative presence counts, reconcile deltas over snapshots); evidence `apps/mobile/lib/features/broadcast/manager/participants_manager.dart:27–43,53–66,212–244`, `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart:47–61`.
**First check (verify):** whether the participants endpoint can filter to present users only. If it can, use that filter and the loop below collapses to a single paged load.
**Why (plain words):** The client treats one historical page as the complete current roster, ignores `isJoined`, discards cursors and counts loaded rows, so departed users appear present and rooms over 50 are incomplete. Cursor pages plus delta reconciliation over a snapshot give an accurate list.

**Files**

| File (exact path from repo root)                                                 | Action | Kind    | Required? | Notes                                          |
| -------------------------------------------------------------------------------- | ------ | ------- | --------- | ---------------------------------------------- |
| `apps/mobile/lib/features/broadcast/manager/participants_manager.dart`           | Modify | manager | Required  | Cursor paging, present-only view, delta apply. |
| `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart`        | Modify | service | Required  | Expose cursor + `isJoined`.                    |
| `apps/mobile/lib/features/broadcast/widgets/participant_list_header_widget.dart` | Modify | widget  | Required  | Show the authoritative count.                  |
| `apps/mobile/test/features/broadcast/participants_manager_test.dart`             | Create | test    | Required  | Paging + delta tests.                          |

**Pseudo-code**

```
ParticipantsManager (owned by ActiveBroadcastSession)
    _present : Map<Id, Participant>          // only isJoined == true
    _attendanceCursor : String?
    _snapshotGeneration = 0

FUNCTION loadPresent()
    gen = ++_snapshotGeneration
    cursor = null; present = {}
    LOOP                                                       // page until the present set is complete (D-10)
        page = AWAIT http.participants(broadcastId, cursor: cursor)
        CHECK session live AND gen == _snapshotGeneration
        present.addAll(page.items where p.isJoined)             // history rows are not "present"
        cursor = page.nextCursor
        UNTIL cursor is null OR a bounded page budget is reached
    _present = present; _attendanceCursor = cursor
    // listeners call this when they OPEN the list, and refresh the count from the REST summary every ~30 s and on reconnect (Step 53)

FUNCTION loadMoreAttendance()
    IF cursor null RETURN
    page = AWAIT http.participants(broadcastId, cursor: cursor)
    APPEND to attendance history only; do not add departed users to _present

FUNCTION applyDelta(event)                 // over the snapshot, never replacing it
    SWITCH event
        joined(u)  -> _present[u.id] = u
        left(u)    -> _present.remove(u.id)
        count(n)   -> _authoritativeCount.value = n            // display this, not _present.length
    // a late first page must not overwrite deltas: apply page only if gen is current
```

**Done when:**

- A participant who left is removed from the present list.
- A room with 60 participants loads all present users across pages (or uses the backend present filter).
- The header shows the authoritative count, not the loaded-row count.
- A first page arriving after a delta does not resurrect a departed user.

**Tests:** `apps/mobile/test/features/broadcast/participants_manager_test.dart` — (a) departed removed, (b) multi-page load, (c) count authority, (d) stale page discarded.

**Watch out for:** Showing `_present.length` as the room count. That is the loaded page, not the room; display the authoritative count.

**Depends on:** Steps 30, 53.

---

### Step 55: Revoke cohost role, presence and media permission atomically

**Phase:** B
**Audit reference:** §3 [H17] "Backend cohost removal does not reliably revoke role or presence", Recommended fix (dedicated atomic role/presence operations, connected media permission updates, matching events); evidence `apps/api/src/modules/broadcast/service.rs:896–912`, `apps/api/src/modules/broadcast/repository/pg.rs:427–447`, `apps/api/src/modules/broadcast/media.rs:41–95`.
_Simplified from design doc §6 (a hidden "retried" media revoke) because a hidden retry mechanism is not needed; safety preserved because removal is idempotent and a failed media call is returned to the host as an error, so the host can simply retry the removal._
**Join stays idempotent (D-2).** `join` keeps returning the existing presence when the _same user_ joins again, because Step 56 needs idempotent join and Step 30's `recover()` re-enters through join. Only the **demotion path** stops using `join`.
**Why (plain words):** Removal calls `join` to demote, which returns `AlreadyPresent` for an active row (no role change) and turns an absent invitee into a joined participant; eviction never closes the participant row, and keeping the participant never revokes their existing publish permission. A dedicated revocation path that changes role, presence and media authorisation together is the only shape that cannot leave a removed cohost with rights.

**Files**

| File (exact path from repo root)                   | Action | Kind        | Required? | Notes                                                           |
| -------------------------------------------------- | ------ | ----------- | --------- | --------------------------------------------------------------- |
| `apps/api/src/modules/broadcast/repository/mod.rs` | Modify | Rust module | Required  | Add `revoke_cohost` / `close_presence` to the repository trait. |
| `apps/api/src/modules/broadcast/repository/pg.rs`  | Modify | Rust module | Required  | One transaction: role + presence + count.                       |
| `apps/api/src/modules/broadcast/service.rs`        | Modify | Rust module | Required  | Orchestrate DB + media + events; no `join` for demotion.        |
| `apps/api/src/modules/broadcast/media.rs`          | Modify | Rust module | Required  | `update_permissions(room, identity, canPublish: false)`.        |
| `apps/api/tests/broadcast_service.rs`              | Modify | test        | Required  | Removal matrix.                                                 |

**Pseudo-code**

```
FUNCTION remove_cohost(broadcast_id, actor, cohost_id, remove_from_room)
    REQUIRE actor is the creator (forbidden otherwise)
    BEGIN transaction
        REQUIRE broadcast is live and cohost_id is a cohost          // not "joined as someone else"
        SET role = listener (or delete the invite row if it was pending)
        IF remove_from_room SET left_at = now() AND present = false
        RECOMPUTE present count from rows, not from a counter delta
    COMMIT                                                          // removing an already-removed cohost is a no-op success
    AWAIT media.update_permissions(room, cohost_id, can_publish=false)   // ALWAYS revoke publish, whether or not they stay in the room
        // a media failure does NOT roll back the DB; it RETURNS the error (partial revocation) so the host can retry the idempotent removal
        // no hidden retry mechanism
    EMIT cohost_removed(broadcast_id, cohost_id) + presence_count_changed(n)

FUNCTION join(broadcast_id, user)
    IF the SAME user is already present RETURN the existing presence   // idempotent; recover() and retries rely on this
    ... existing insert path ...
    // the demotion path in remove_cohost never calls join
```

**Done when:**

- Removing an active cohost changes the role in the same transaction that flips presence.
- Removing a pending invitee does not create a joined participant.
- Media publish permission is revoked for the removed identity (integration test against the media fake).
- The emitted count equals a recount from the database.
- Removing a non-cohost returns a typed error and changes nothing.

**Tests:** `apps/api/tests/broadcast_service.rs` — (a) active cohost demoted + evicted, (b) invited-but-absent removed without joining, (c) media permission revoked, (d) non-cohost removal rejected.

**Watch out for:** Rolling back the database when the media call fails. Local state must not depend on media success, or a blip resurrects a removed cohost.

**Depends on:** Step 53 (event vocabulary).

---

### Step 56: Make broadcast start/join retry-safe

**Phase:** B
**Audit reference:** §3 [H18] "Backend broadcast transitions are not safely retryable", Recommended fix (reject terminal rows atomically; move fallible preconditions earlier; make related DB transitions atomic; define compensation or committed-outcome recovery); evidence `apps/api/src/modules/broadcast/service.rs:440–476,728–744`, `apps/api/src/modules/broadcast/repository/pg.rs:283–286,349–354,468–474`.
**Why (plain words):** Start and join commit state before the later token and response work, so a failure after commit makes a retry conflict even though the first attempt took effect; and start accepts a previously ended inactive row without clearing its ending fields, so an ended broadcast can become active while still reporting ended. Moving preconditions before the commit and rejecting terminal rows atomically makes "error" mean "did not happen".

**Files**

| File (exact path from repo root)                  | Action | Kind        | Required? | Notes                                                         |
| ------------------------------------------------- | ------ | ----------- | --------- | ------------------------------------------------------------- |
| `apps/api/src/modules/broadcast/service.rs`       | Modify | Rust module | Required  | Preconditions first; no committed state before fallible work. |
| `apps/api/src/modules/broadcast/repository/pg.rs` | Modify | Rust module | Required  | Conditional updates that reject terminal rows.                |
| `apps/api/tests/broadcast_service.rs`             | Modify | test        | Required  | Retry and ended-row matrix.                                   |

**Pseudo-code**

```
FUNCTION start(broadcast_id, creator)
    READ broadcast
    REQUIRE state allows start AND ended_at IS NULL AND ended_reason IS NULL
        // an ended row can never be restarted; clearing ending fields is forbidden here
    REQUIRE creator identity matches
    PRE-CREATE everything fallible FIRST
        room name, media room, tokens (in memory)          // nothing persisted yet
    BEGIN transaction
        UPDATE broadcast SET state = live
            WHERE id = $1 AND state = <expected> AND ended_at IS NULL    // conditional
        REQUIRE rows_affected == 1 ELSE ROLLBACK -> typed Conflict
        INSERT the creator's participant row
    COMMIT
    RETURN tokens

FUNCTION join(broadcast_id, user)
    READ broadcast; REQUIRE it is live AND not ended        // terminal rejection BEFORE any write
    IF the SAME user is already present RETURN the existing presence   // idempotent for the same user (Step 55, D-2)
    AWAIT media token for this identity (fallible, BEFORE the DB write)
    BEGIN transaction
        INSERT participation conditionally (ON CONFLICT DO NOTHING)
        REQUIRE the row is present afterwards
    COMMIT
    // the existing idempotency middleware (Step 58) replays a stored outcome for a repeated key; nothing is added here
```

**Done when:**

- Retrying a start that failed after its media step either succeeds cleanly or reports "already live" with the same state — never a half-state.
- Starting an ended broadcast is refused and its ending fields are untouched.
- A join whose media step fails leaves no participant row.
- A repeated join by the same user returns the existing presence rather than a conflict.

**Tests:** `apps/api/tests/broadcast_service.rs` — (a) failed media step leaves no committed start, (b) ended row refused, (c) retry after commit reports already-started, (d) a repeated join by the same user is idempotent.

**Watch out for:** "compensating" by deleting rows after a failure. Conditional transitions plus ordering avoid the need; compensation that itself fails is worse than none.

**Depends on:** Step 55 (join stays idempotent). It no longer depends on Step 58, which breaks the 56 → 58 → 57 → 56 cycle (D-1).

---

### Step 57: Make the editor create → upload → start → entry one operation

**Phase:** B
**Delivery group:** G10 (Step 57 plus the mobile half of Step 58 land as one change).
**Audit reference:** §3 [H19] "Editor progress and retries can report success without completing entry", Recommended fix (one operation spanning create/upload/start/entry; distinguish remote commitment from local connection; retry the incomplete phase and synchronize metadata); evidence `apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart:35–84`, `apps/mobile/lib/features/broadcast/pages/broadcast_editor_page.dart:46–55`.
**Why (plain words):** `.started` is recorded before local entry succeeds, so a retry returns early and navigates after a no-op; busy state excludes creation and upload, so fields stay editable mid-operation; and edits made before a retry are never patched onto the created row. One operation with named phases makes progress and retry honest.

**Files**

| File (exact path from repo root)                                           | Action | Kind    | Required? | Notes                                                           |
| -------------------------------------------------------------------------- | ------ | ------- | --------- | --------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart` | Modify | manager | Required  | Phase enum, monotone progress, retry from the incomplete phase. |
| `apps/mobile/lib/features/broadcast/pages/broadcast_editor_page.dart`      | Modify | widget  | Required  | Disable fields while the operation runs; show the phase.        |
| `apps/mobile/lib/features/broadcast/model/broadcast_creation_step.dart`    | Create | enum    | Required  | `none                                                           | created | uploaded | started | entered | failed(phase)`. |
| `apps/mobile/test/features/broadcast/broadcast_editor_manager_test.dart`   | Create | test    | Required  | Retry matrix.                                                   |

**Pseudo-code**

```
enum BroadcastCreationStep { none, creating, created, uploading, uploaded, starting, started, entering, entered }
    // `started` means the SERVER committed; `entered` means the local session is live

FUNCTION publish()
    editLocked = true
    TRY
        IF step < created
            draft = AWAIT create(fields)                  // idempotency key forwarded by the same change (Step 58, mobile half)
            step = created
        IF draft changed since creation
            AWAIT patch(draft.id, latestFields)           // H19: edits before retry are not ignored
        IF step < uploaded AND image != null
            uploadedUrl = AWAIT upload(image); step = uploaded
        IF step < started
            AWAIT start(draft.id); step = started         // remote commitment
        IF step < entered
            result = AWAIT activeBroadcastManager.join(EnterRequest(draft.id, asCreator))
            IF result failed -> step = started; THROW typed LocalEntryFailed
            step = entered
    FINALLY
        editLocked = false

FUNCTION retry()
    // resumes from `step`, never from zero, and never skips a phase
    AWAIT publish()

Progress/UI
    button label and progress derive from `step` and the command's isRunning
    navigate ONLY after step == entered
```

**Done when:**

- A local entry failure leaves the step at `started` and retry performs only the entry step.
- A retry after editing the title patches the created row before entering.
- Fields are not editable while the operation runs.
- No navigation happens before `entered`.
- `isRunning` covers create, upload and start (one command, one busy state).

**Tests:** `apps/mobile/test/features/broadcast/broadcast_editor_manager_test.dart` — (a) entry failure then retry, (b) edit-before-retry patches, (c) fields locked during run, (d) navigation gated on `entered`.

**Watch out for:** Treating `started` as `entered`. Navigating on the remote commit re-creates the original bug.

**Depends on:** Steps 30, 34, 56. Step 58 (mobile half) lands in the same change; there is no cycle (D-1).

---

### Step 58: Give each logical mutation a stable idempotency key (mobile)

**Phase:** B
**Delivery group:** G10 (lands with Step 57).
**Audit reference:** §3 [H20] "Broadcast mutations do not carry stable logical-operation idempotency keys", Recommended fix (persist one key per logical mutation through ambiguous retries; do not indiscriminately retry every mutation); evidence `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart:84–98,173–195`, `apps/mobile/lib/_shared/services/http_client.dart:159–166,447–452`, `apps/api/src/middleware/idempotency.rs:305–315`.
**Why (plain words):** Create, live and join omit the backend's optional idempotency protection, so a lost response turns a successful join into a conflict and a retry can duplicate a draft. A key that belongs to the _logical_ operation — not to the attempt — survives manual retries and network retries alike. The audit describes the backend protection as existing; the defect is that the mobile client does not send keys.

_Simplified from design doc §6 (a backend middleware rewrite adding stored-outcome replay and a retention window) because the audit evidence (`idempotency.rs:305–315`) shows the protection already exists and is optional, and audit §6 notes it has finite retention; safety preserved because the client now sends one persisted key per logical mutation and reuses it on retry. This also removes the backend half that created the D-1 cycle._

**Read first (verify):** open `apps/api/src/middleware/idempotency.rs`. **Touch the backend only if it does not already replay a stored outcome for a repeated key**; if it does, this step is mobile-only. If a backend change is needed, keep it to replaying the stored outcome and documenting the finite retention, and add `apps/api/tests/idempotency.rs`.

**Files**

| File (exact path from repo root)                                           | Action          | Kind        | Required?                                                       | Notes                                                                      |
| -------------------------------------------------------------------------- | --------------- | ----------- | --------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `apps/mobile/lib/_shared/services/http_client.dart`                        | Modify          | service     | Required                                                        | Forward `Idempotency-Key` from the caller; never generate one per attempt. |
| `apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart` | Modify          | manager     | Required                                                        | One key per draft creation, persisted with the draft.                      |
| `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart`  | Modify          | service     | Required                                                        | Pass the key for create/live/join/end.                                     |
| `apps/mobile/test/features/broadcast/idempotency_key_test.dart`            | Create          | test        | Required                                                        | Same key across retries.                                                   |
| `apps/api/src/middleware/idempotency.rs`, `apps/api/tests/idempotency.rs`  | Modify / Create | Rust module | **Only if the read-first check finds no stored-outcome replay** | Replay the stored outcome; document finite retention.                      |

**Pseudo-code**

```
Mobile
    key = uuid4() generated when the LOGICAL operation begins
    PERSIST the key with the draft/local record   (survives app restart for an ambiguous create)
    SEND header 'Idempotency-Key: <key>' on create / start-present / join / end
    ON manual retry REUSE the same key             // never generate a new one "for safety"
    ON a new logical operation (a genuinely new draft) generate a NEW key
    APPLY only to operations the backend protects; do not add keys everywhere "for symmetry"
    // finite retention: document it in a client comment; do not describe it as permanent deduplication
```

**Done when:**

- Two attempts of the same create carry the same key; a second, distinct draft carries a different one.
- The draft's key survives an app restart.
- The retention window is documented in a client comment (and in the middleware if it was touched).

**Tests:** `apps/mobile/test/features/broadcast/idempotency_key_test.dart` — (a) key reused across retry, (b) new draft new key, (c) key persisted with the draft. (If the backend was touched: `apps/api/tests/idempotency.rs` — replay returns the stored outcome.)

**Watch out for:** generating the key inside the retry loop or inside an interceptor. That turns idempotency into decoration.

**Depends on:** Step 57 (same delivery group).

---

### Step 59: Back favourites with one account-level reactive source

**Phase:** B
**Audit reference:** §3 [M07] "Favourites are disconnected snapshots", Recommended fix (one account-level reactive favourites source, preferably backed by an owner-filtered DB query; reconcile authoritative stored state); evidence `apps/mobile/lib/features/broadcast/manager/favourites_manager.dart:15–20,50`, `apps/mobile/lib/features/broadcast/model/proxies/broadcast_proxy.dart:21–29`, `apps/mobile/lib/features/profile/pages/my_profile_page.dart:316–321`.
**Why (plain words):** The favourites tab watches a list that starts empty and is never fetched, and a proxy's own favourite write does not update that list or any other proxy instance. One reactive collection owned by the account manager, seeded from the owner-filtered database and updated by the desired-state setter, makes every surface agree.

**Files**

| File (exact path from repo root)                                        | Action | Kind          | Required? | Notes                                        |
| ----------------------------------------------------------------------- | ------ | ------------- | --------- | -------------------------------------------- |
| `apps/mobile/lib/features/broadcast/manager/favourites_manager.dart`    | Modify | manager       | Required  | `ListNotifier`, loaded once, mutated on set. |
| `apps/mobile/lib/features/broadcast/model/proxies/broadcast_proxy.dart` | Modify | freezed model | Required  | Favourite write delegates to the manager.    |
| `apps/mobile/lib/features/profile/pages/my_profile_page.dart`           | Modify | widget        | Required  | Renders from the manager's collection.       |
| `apps/mobile/test/features/broadcast/favourites_manager_test.dart`      | Create | test          | Required  | Cross-surface consistency.                   |

**Pseudo-code**

```
FavouritesManager (account scope, lazy)
    favourites = ListNotifier<BroadcastSummary>()          // one account-level source
    _loadedOnce = false

FUNCTION ensureLoaded()
    IF _loadedOnce RETURN
    _loadedOnce = true
    rows = AWAIT local.favouriteRows(ownerId)             // owner-filtered DB query
    favourites.replaceAll(rows.map(toSummary))
    AWAIT refreshFromServer()                             // reconcile authoritative state

FUNCTION setFavourite(summary, isFavourite)
    IF isFavourite favourites.add(summary) ELSE favourites.removeWhere(sameId)
    TRY
        AWAIT local.setFavourite(summary.id, isFavourite)  // desired state (Step 37)
        AWAIT http.favourite(summary.id, isFavourite)      // server
    CATCH
        REVERT the collection and the row                   // one place owns the truth
        RETHROW typed error for the UI

BroadcastProxy
    toggleFavourite() -> favouritesManager.setFavourite(...)
    isFavourite getter reads the collection (or its override), never a private copy
```

**Done when:**

- Saving a broadcast from a card makes it appear in the favourites tab without a reload.
- Unfavouriting elsewhere removes it from every surface.
- Starting the app loads existing favourites from the database before the server responds.
- A failed write reverts the collection and the row together.

**Tests:** `apps/mobile/test/features/broadcast/favourites_manager_test.dart` — (a) proxy save updates the list, (b) remove updates all surfaces, (c) load from DB seeds the list, (d) failure reverts both.

**Watch out for:** keeping a second list in the page. The page watches the manager's collection only.

**Depends on:** Step 37.

---

## 13. Phase F — Shared feeds

**Findings finished here:** H11, H12, M08, M09, I01.

**What you will have when this phase is done (3–5 lines):**

- A feed base where every request carries an immutable query generation, stale responses are discarded, and a refresh and a page load cannot interleave badly.
- Filtered feeds that reach existing ended broadcasts: preferably through a real backend ended filter (audit H12 lists that first), otherwise by bounded client-side paging.
- Separate initial-load and page-load error surfaces, with an explicit retry instead of incidental re-triggering.
- Subscription rebinding that cancels the old handles, and event insertion that respects the feed's query.
- A cursor-only base, with numbered-page arithmetic removed.

**How to know it's safe to ship:** the feed suite passes; typing in a search box during a load never shows the previous query's rows; scrolling a client-filtered "recently live" list reaches existing ended broadcasts; a next-page failure shows a footer with retry and does not loop.

**Order:** 60 → 61 → 62 → 63 → 64. Delivery group **G11** (Steps 60, 62 and 64) changes the same base file and tests, so they ship as one change; Step 61 is separate and is deleted if the backend already has an ended filter; Step 63 stays separate.

**What we are deliberately NOT doing in F:** no numbered-page support (nothing uses it), no new feed widgets, no event-bus integration, and no account-level feed singletons (feeds stay widget-owned, audit §5 item 3).

---

### Step 60: Capture immutable query generations and arbitrate page versus refresh

**Phase:** F
**Delivery group:** G11 (Steps 60, 62 and 64).
**Audit reference:** §3 [H11] "Refresh, pagination, query changes, and invalidations race", Recommended fix (capture immutable query generations, reject stale responses, arbitrate page versus refresh, coalesce a trailing refresh); evidence `apps/mobile/lib/_shared/manager/feed_data_source.dart:24–28,87–95,149–155`, `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:75–126,183–187`.
**Why (plain words):** Refresh and pagination can run simultaneously, responses read mutable query state, and a query change or invalidation during a request is silently dropped — so old rows and cursors overwrite fresh results. A generation counter per query plus one arbitration rule fixes all three.

**Files**

| File (exact path from repo root)                                             | Action | Kind    | Required? | Notes                                                 |
| ---------------------------------------------------------------------------- | ------ | ------- | --------- | ----------------------------------------------------- |
| `apps/mobile/lib/_shared/manager/feed_data_source.dart`                      | Modify | manager | Required  | Generation, arbitration, trailing refresh.            |
| `apps/mobile/lib/features/broadcast/manager/broadcast_feed_data_source.dart` | Move   | manager | Required  | From `_shared/manager/`; implements the new contract. |
| `apps/mobile/test/_shared/manager/feed_data_source_test.dart`                | Create | test    | Required  | Race matrix.                                          |

**Pseudo-code**

```
FeedDataSource
    _generation = 0                     // bumped on every query change
    _refreshInFlight = false
    _pageInFlight = false
    _pendingRefresh = false

FUNCTION setQuery(newQuery)             // search text, filter, creator
    _query = freeze(newQuery)           // immutable snapshot, never read live during a request
    _generation += 1
    reset()                             // clear items + cursor
    updateDataCommand.run()

FUNCTION refresh()
    IF _refreshInFlight OR _pageInFlight        // arbitration: never both
        _pendingRefresh = true; RETURN           // coalesce, do not queue N refreshes
    gen = _generation; _refreshInFlight = true
    TRY
        page = AWAIT fetchFirst(_query)
        CHECK gen == _generation ELSE RETURN      // stale: discard silently
        REPLACE items, cursor
    FINALLY
        _refreshInFlight = false
        IF _pendingRefresh SWAP false -> refresh()      // trailing refresh runs once

FUNCTION loadNextPage()
    IF _pageInFlight OR _refreshInFlight OR cursor == null RETURN
    gen = _generation; _pageInFlight = true
    TRY
        page = AWAIT fetchNext(_query, cursor)
        CHECK gen == _generation ELSE RETURN      // a query change discards the page
        APPEND (dedupe by id)
    FINALLY
        _pageInFlight = false
        IF _pendingRefresh SWAP false -> refresh()
```

**Done when:**

- Changing the query while a page is in flight discards the page (no old rows).
- A refresh during a page load waits and runs exactly once afterwards.
- Three invalidations during one refresh produce one trailing refresh.
- The query read inside a request is a snapshot, not the mutable field.

**Tests:** `apps/mobile/test/_shared/manager/feed_data_source_test.dart` — (a) query change discards in-flight page, (b) refresh waits for page, (c) coalesced invalidations, (d) stale cursor never appended.

**Watch out for:** comparing query objects by identity. Freeze them as values (or copy fields) so equality works.

**Depends on:** P1 only. (The earlier dependency on Steps 31 and 38 was not a real one, D-17.)

---

### Step 61: Keep paging through client-filtered pages

**Phase:** F
**Audit reference:** §3 [H12] "Client-filtered empty pages strand pagination", Recommended fix (add a real backend ended filter, or advance through filtered pages until visible progress/exhaustion with bounded work and cursor-progress checks); evidence `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:78–85,140–143`, `apps/mobile/lib/_shared/manager/feed_data_source.dart:149–155`, `apps/mobile/lib/_shared/widgets/feed_widget.dart:151`.
**First check (verify, A-9):** audit H12 lists "add an actual backend ended filter" first, and the broadcast list endpoint already has a status filter (audit §3 evidence). **Check whether that status filter can return ended broadcasts.** If yes: use it, delete the client-side filter, and **skip the rest of this step** (no loop, no Load more button, no `hasMoreFiltered`). Only if it cannot, implement the bounded client-side loop below.
**Why (plain words):** Recently-live filters an inactive page locally, so if nothing survives the filter the widget renders empty and never reads an item — and item-triggered pagination never fires. Existing ended broadcasts become unreachable.

**Files**

| File (exact path from repo root)                                             | Action | Kind    | Required? | Notes                                                      |
| ---------------------------------------------------------------------------- | ------ | ------- | --------- | ---------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/manager/broadcast_feed_data_source.dart` | Modify | manager | Required  | Bounded filter-aware paging.                               |
| `apps/mobile/lib/_shared/widgets/feed_widget.dart`                           | Modify | widget  | Required  | Offer "load more" when the list is empty but data remains. |
| `apps/mobile/test/features/broadcast/filtered_feed_test.dart`                | Create | test    | Required  | Empty-page progression.                                    |

**Pseudo-code**

```
FUNCTION loadNextPageWhileFilteredEmpty()
    maxPages = 5                                   // bounded work per user action
    pages = 0
    LOOP
        beforeCursor = cursor; beforeCount = items.length
        AWAIT loadOnePage()
        pages += 1
        IF items.length > beforeCount RETURN         // visible progress
        IF cursor == null RETURN                     // exhausted
        IF cursor == beforeCursor RETURN             // cursor did not advance: stop, flag a bug
        IF pages >= maxPages
            _hasMoreFiltered = true                  // UI shows an explicit "load more"
            RETURN

FeedWidget
    IF itemCount == 0 AND source.hasMoreFiltered
        SHOW an explicit "Load more" button (a user action, not a scroll trigger)
    IF itemCount == 0 AND NOT hasMoreFiltered
        SHOW the empty state
```

**Done when:**

- A "recently live" filter with three fully inactive pages still reaches the older ended broadcast.
- A feed whose filtered pages never produce results stops after the bound and offers "load more".
- A non-advancing cursor stops the loop and reports (does not spin).
- The empty state is distinguishable from "more may exist".

**Tests:** `apps/mobile/test/features/broadcast/filtered_feed_test.dart` — (a) reaches older items across filtered pages, (b) bound respected, (c) non-advancing cursor stops, (d) explicit load-more rendered.

**Watch out for:** an unbounded `while` loop inside `getItemAtIndex`. Pagination must stay bounded and user-recoverable.

**Depends on:** Step 60.

---

### Step 62: Give page-load failures their own error surface

**Phase:** F
**Delivery group:** G11.
**Audit reference:** §3 [M08] "Pagination errors are absent from the feed error surface", Recommended fix (separate observable page error/retry state; block automatic repeated pagination until explicit retry); evidence `apps/mobile/lib/_shared/manager/feed_data_source.dart:41–42,87–90,149–155`, `apps/mobile/lib/_shared/widgets/feed_widget.dart:127–147`.
**Why (plain words):** `commandErrors` exposes refresh errors only, and the pagination gate checks that same channel, so next-page failures have no footer or retry and can be re-triggered incidentally while the list rebuilds.

**Files**

| File (exact path from repo root)                         | Action | Kind    | Required? | Notes                        |
| -------------------------------------------------------- | ------ | ------- | --------- | ---------------------------- |
| `apps/mobile/lib/_shared/manager/feed_data_source.dart`  | Modify | manager | Required  | `pageError` + `retryPage()`. |
| `apps/mobile/lib/_shared/widgets/feed_widget.dart`       | Modify | widget  | Required  | Footer error with retry.     |
| `apps/mobile/test/_shared/widgets/feed_widget_test.dart` | Modify | test    | Required  | Footer states.               |

**Pseudo-code**

```
FeedDataSource
    pageError : ValueNotifier<FeedPageError?>(null)

FUNCTION loadNextPage()
    IF pageError.value != null RETURN                 // requires explicit retry
    TRY ... CATCH e
        pageError.value = classify(e)                 // retryable | terminal
FUNCTION retryPage()
    pageError.value = null
    AWAIT loadNextPage()

FeedWidget footer
    IF source.pageError.value != null
        RENDER a row with the message + "Retry" calling retryPage()
    ELSE IF isFetchingNextPage
        RENDER the progress row
    // refresh errors keep their existing top-of-list treatment
```

**Done when:**

- A failing next page shows a footer with retry and stops auto-triggering.
- Retrying after success clears the footer.
- A refresh error and a page error are both visible and distinguishable.
- Scrolling while in the error state does not re-trigger.

**Tests:** `apps/mobile/test/_shared/widgets/feed_widget_test.dart` — (a) page error footer, (b) no auto retrigger, (c) retry clears, (d) refresh error unaffected.

**Watch out for:** Clearing `pageError` inside the pagination command. Clearing it before an explicit retry re-enables the auto-trigger loop.

**Depends on:** Step 60.

---

### Step 63: Rebind subscriptions safely and respect query predicates on events

**Phase:** F
**Audit reference:** §3 [M09] "Socket rebinding cancels the wrong handles; feed events ignore filters", Recommended fix (detach old handles into locals synchronously, then cancel them; apply full query predicates or invalidate when event data is insufficient); evidence `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:119–124,172–178,190–205`.
**Why (plain words):** Cancellation is unawaited and reads subscription fields across awaits while rebinding overwrites those fields, so subscriptions leak or vanish; and event insertions ignore the feed's creator/query predicates, so a creator-specific feed acquires other creators' rows.

**Files**

| File (exact path from repo root)                                             | Action | Kind    | Required? | Notes                                                      |
| ---------------------------------------------------------------------------- | ------ | ------- | --------- | ---------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/manager/broadcast_feed_data_source.dart` | Modify | manager | Required  | Detach-then-cancel, predicate checks, invalidate fallback. |
| `apps/mobile/test/features/broadcast/feed_event_binding_test.dart`           | Create | test    | Required  | Rebind + predicate matrix.                                 |

**Pseudo-code**

```
FUNCTION bindEvents(events)
    old = _handles; _handles = []                 // synchronously detach into a local
    FOR h IN old AWAIT h.cancel()                 // cancel AFTER the swap; a new bind cannot be clobbered

    _handles = [
        events.whereType<NewBroadcast>().listen((e) {
            IF matchesQuery(e.summary) addItemAtStart(e.summary)
            ELSE /* nothing: it does not belong here */
        }),
        events.whereType<EndedSignal>().listen((e) {
            IF matchesQueryById(e.broadcastId) markEnded(e.broadcastId)
            ELSE ignore
        }),
    ]

FUNCTION matchesQuery(summary)
    CHECK creator filter (if the feed is creator-specific)
    CHECK status/state filter
    CHECK any search predicate the query carries
    IF the event lacks the data needed to decide RETURN false and schedule an invalidate
```

**Done when:**

- Rebinding twice leaves exactly one live subscription set.
- A creator-specific feed ignores another creator's new broadcast.
- An event without enough data triggers one invalidate instead of a wrong insert.
- Cancelling during an in-flight rebind never cancels the new handles.

**Tests:** `apps/mobile/test/features/broadcast/feed_event_binding_test.dart` — (a) rebind idempotent, (b) creator predicate, (c) insufficient data invalidates, (d) cancel ordering.

**Watch out for:** Cancelling before detaching into locals. That is exactly how the new handles get cancelled by the old rebind.

**Depends on:** Step 50, 60.

---

### Step 64: Reduce the feed base to the cursor contract that actually exists

**Phase:** F
**Delivery group:** G11.
**Audit reference:** §3 [I01] "The feed base mixes unused page arithmetic with cursor pagination", Recommended fix (standardize the shared base on the actual cursor contract; add numbered pagination only when an endpoint needs it); evidence `apps/mobile/lib/_shared/manager/feed_data_source.dart:100–120,171–175`, `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:55–69`.
**Why (plain words):** The base carries numbered-page state while every implemented endpoint uses opaque cursors and the concrete source overrides that state anyway — a trap for the next feature, which will copy whichever model it notices first.

**Files**

| File (exact path from repo root)                              | Action | Kind    | Required? | Notes                                                  |
| ------------------------------------------------------------- | ------ | ------- | --------- | ------------------------------------------------------ |
| `apps/mobile/lib/_shared/manager/feed_data_source.dart`       | Modify | manager | Required  | Delete page/size arithmetic; keep the cursor contract. |
| `apps/mobile/test/_shared/manager/feed_data_source_test.dart` | Modify | test    | Required  | Contract assertions.                                   |

**Pseudo-code**

```
FeedDataSource (cursor only)
    items, itemCount, updateDataCommand, isFetchingNextPage, commandErrors, pageError
    cursor : String?
    hasNextPage <=> cursor != null
    abstract fetchFirst(query) -> CursorPage
    abstract fetchNext(query, cursor) -> CursorPage
REMOVE
    pageNumber, pageSize, totalPages, offset maths and their defaults
    any `if (mode == numbered)` branch
KEEP the length-3 prefetch trigger (audit §5 item 4) and the separate refresh/pagination commands
```

**Done when:**

- `grep -rn "pageSize\|pageNumber\|offset" apps/mobile/lib/_shared/manager` finds nothing.
- Both existing feeds still paginate.
- The prefetch still triggers three items before the end.

**Tests:** `apps/mobile/test/_shared/manager/feed_data_source_test.dart` — (a) cursor drives `hasNextPage`, (b) prefetch trigger at `length - 3`, (c) no numbered-page API remains.

**Watch out for:** Removing the length-3 prefetch along with the page arithmetic. The prefetch is on the audit keep-list.

**Depends on:** Steps 60, 61, 62, 63.

---

## 14. Phase U — Backend prerequisites (remediation track)

**Findings finished here:** H09 (backend half), H22, H23, M13 (backend half). (C05 and H21, Steps 65 and 67, now sit directly after P1; the notification feature work, Steps 70, 71 and 73, is the separate **Feature track** after Phase C.)

Plain words for the Rust steps: these are all "the server must not promise something it does not do". Each step changes one decision point, adds a test that fails today, and names the mobile fallback that applies until it lands.

**What you will have when this phase is done (3–5 lines):**

- Refresh rotation with distinct, documented error codes and a 429 that never consumes the token (Step 66).
- Notes sync that resolves folder ownership after applying folder mutations and never unfiles a note because a field was omitted (Step 68); chat deletion that cannot cross rooms (Step 69).
- Field-level settings PATCH that cannot delete a key it did not send (Step 72).

**How to know it's safe to ship:** `cargo test` covers each new guard; the mobile fallbacks in each step are implemented on the client side (Steps 7, 10, 42); no feature UI claims a capability the backend does not provide.

**Order:** 66, 68, 69 and 72 are small and independent and can ship any time after P1. Steps 65 and 67 (backend revocation and reset ordering, delivery group **G12**) ship **right after P1** because audit §8 puts them second.

**What we are deliberately NOT doing in U:** no new chat/notes/discover product features, no websocket replay service standing in for durability, no push delivery redesign, no stored-outcome replay for refresh rotation (the guide's own assumption A5), and no localization of any new copy.

---

### Step 66: Make refresh rotation errors actionable and non-consuming

**Phase:** U (Backend)
**Audit reference:** §3 [H09] "Refresh failures destroy sessions, including ambiguous rotated-token outcomes", backend half; design §11 "Backend prerequisites" rows C05/H09. Evidence `apps/api/src/modules/auth/token.rs:439–486`, `apps/api/src/modules/auth/error.rs:126–137`.
**Why (plain words):** Every reuse of a refresh token is reported as the same generic `InvalidToken`, which tells the client nothing about what to do, and a rate limit could consume the token. The client now refuses to guess (Step 10); this step makes the server's answers distinguishable.

_Simplified from design doc §11 (a stored-outcome replay window so a lost response can be retried) because it contradicts assumption A5 (an uncertain refresh requires re-login, never an automatic retry), the Step 10 client design and the "NOT doing" list; safety preserved because the client treats every uncertain outcome as re-login, and the server now returns distinct codes and never consumes a token on a 429._

**Files**

| File (exact path from repo root)     | Action | Kind        | Required? | Notes                                                          |
| ------------------------------------ | ------ | ----------- | --------- | -------------------------------------------------------------- |
| `apps/api/src/modules/auth/error.rs` | Modify | Rust module | Required  | Distinct documented error categories.                          |
| `apps/api/src/modules/auth/token.rs` | Modify | Rust module | Required  | Return the distinct codes; run the rate check before rotation. |
| `apps/api/tests/auth_service.rs`     | Modify | test        | Required  | Error-code matrix.                                             |

**Pseudo-code**

```
FUNCTION refresh(presented_token)
    IF rate limited or in maintenance -> RETURN 429 BEFORE rotation          // documented as non-consuming
    record = LOAD session lineage for the presented token
    IF record is unknown or expired -> Reject(AuthError::RefreshExpired)
    IF record was already rotated   -> Reject(AuthError::RefreshConsumed)    // terminal for THIS token; no replay
    rotated = rotate(record)
    RETURN the new pair

Error categories (documented in error.rs and the API docs)
    RefreshExpired     -> never valid, or valid too long ago
    RefreshConsumed    -> valid lineage already rotated
    RefreshRateLimited -> 429, token NOT consumed
    Internal           -> uncertain for the client by definition (Step 10 treats it as Uncertain)
```

**Done when:**

- A reused token returns `RefreshConsumed`, an unknown or expired one `RefreshExpired`, and the two are distinguishable in the response.
- A 429 pre-check leaves the token usable.
- There is no stored-outcome replay and no replay window.

**Tests:** `apps/api/tests/auth_service.rs` — (a) expired lineage, (b) consumed lineage, (c) 429 does not consume, (d) internal error is not mapped to `RefreshExpired`.

**Watch out for:** adding a replay window "to be helpful". That is the mechanism this step deliberately does not build.

**Depends on:** nothing. Mobile behaviour: Step 10's `Uncertain` requires reauthentication, and the UI says so plainly.

---

### Step 68: Resolve note folder ownership after folder mutations

**Phase:** U (Backend)
**Audit reference:** §3 [H22] "Notes sync can lose folders created in the same batch", Recommended fix (resolve live folder ownership after folder mutations; add PostgreSQL regression coverage); evidence `apps/api/src/modules/notes/repository/pg.rs:867–902`, `apps/api/src/modules/notes/model.rs:181–186`.
**Second defect (A-1, audit H22 "folder/PATCH semantics"):** the notes PATCH writes the folder assignment unconditionally, so a request that merely **omits** `folder_id` can unfile a note. This step fixes both: the same-batch ordering and the PATCH omission.
**Why (plain words):** The batch collects owned folder ids _before_ applying folder mutations, which contradicts the documented folder-first semantics, so a note that references a folder created in the same batch is silently unfiled. Moving the ownership query after the folder loop fixes one ordering bug.

**Files**

| File (exact path from repo root)              | Action | Kind        | Required? | Notes                                                                  |
| --------------------------------------------- | ------ | ----------- | --------- | ---------------------------------------------------------------------- |
| `apps/api/src/modules/notes/repository/pg.rs` | Modify | Rust module | Required  | Resolve ownership after folder mutations, inside the same transaction. |
| `apps/api/src/modules/notes/model.rs`         | Modify | Rust module | Required  | Document the ordering guarantee on the batch type.                     |
| `apps/api/tests/notes_repository_pg.rs`       | Modify | test        | Required  | Same-batch regression.                                                 |

**Pseudo-code**

```
FUNCTION apply_sync_batch(creator_id, batch) -> Vec<ResolvedMutation>
    BEGIN transaction
        FOR each folder mutation
            resolve_folder(tx, creator_id, mutation)          // folders FIRST (documented)
        NOW collect note folder ids AND resolve ownership
            // moved here: a folder created above is now visible to owned_folder_ids
        FOR each note mutation
            IF the mutation OMITS folder_id -> leave the stored folder_id UNTOUCHED     // PATCH semantics: absent means unchanged
            ELSE folder_id = if owned(folder_id) { folder_id } else { unfiled }         // explicit null still unfiles on purpose
            resolve_note(tx, creator_id, mutation, folder_id)
    COMMIT
    // foreign folder ids are still silently unfiled (that behaviour is correct)
```

**Done when:**

- A batch containing "create folder F" then "note N with folder_id = F" leaves N filed in F.
- A note referencing another user's folder is still unfiled.
- The ownership query happens after the folder loop (assert by ordering in the code and by test).
- A PATCH that omits `folder_id` leaves the note's folder unchanged; an explicit `null` unfiles it.
- The test runs against PostgreSQL, not the in-memory repository.

**Tests:** `apps/api/tests/notes_repository_pg.rs` — (a) same-batch folder assignment, (b) foreign folder unfiled, (c) folder rename then note update in one batch, (d) PATCH omitting `folder_id` keeps the folder (PostgreSQL regression).

**Watch out for:** Resolving ownership once for the whole batch. It must run after the folder mutations, inside the same transaction.

**Mobile fallback until this lands:** No notes-sync work starts until this lands; the registration seam named in Step 73 stays unused.

**Depends on:** nothing.

---

### Step 69: Guard chat deletion by room as well as sender

**Phase:** U (Backend)
**Audit reference:** §3 [H23] "Chat deletion can mutate one room and publish to another", Recommended fix (include broadcast identity in the guarded delete and test cross-room requests); evidence `apps/api/src/modules/chat/service.rs:252–279`, `apps/api/src/modules/chat/repository/pg.rs:235–252`.
**Why (plain words):** The delete is guarded by message id and sender only, and the broadcast id is checked _after_ a failed delete, so an author can delete a message through another room's id and send a deletion event there. Putting the room into the guarded statement removes the possibility rather than reporting it.

**Files**

| File (exact path from repo root)             | Action | Kind        | Required? | Notes                                                                                |
| -------------------------------------------- | ------ | ----------- | --------- | ------------------------------------------------------------------------------------ |
| `apps/api/src/modules/chat/repository/pg.rs` | Modify | Rust module | Required  | `... WHERE id = $1 AND broadcast_id = $2 AND sender_id = $3 AND deleted_at IS NULL`. |
| `apps/api/src/modules/chat/service.rs`       | Modify | Rust module | Required  | Publish only after a guarded success.                                                |
| `apps/api/tests/chat_service.rs`             | Modify | test        | Required  | Cross-room matrix.                                                                   |

**Pseudo-code**

```
FUNCTION delete_message(member, message_id)
    deleted = repo.soft_delete_message(message_id, member.broadcast_id, member.user_id)
    //                                             ^ room now part of the guard
    IF NOT deleted
        message = repo.find_message(message_id)
        RETURN 404 not_found if absent OR message.broadcast_id != member.broadcast_id
        RETURN 403 not_sender otherwise                      // diagnosis only, no state change
    publisher.publish(member.broadcast_id, deleted_message(member.broadcast_id, message_id))
    RETURN ok
```

**Done when:**

- Deleting a message with another room's id returns 404 and publishes nothing.
- Deleting your own message in the correct room succeeds and publishes one event.
- Deleting someone else's message returns 403 and mutates nothing.
- Direct responses accept message id and broadcast id (or member identity that carries them) — no invented ad-hoc ids.

**Tests:** `apps/api/tests/chat_service.rs` — (a) cross-room delete refused, (b) correct delete publishes here, (c) other sender forbidden, (d) already-deleted is a 404 not a 403.

**Watch out for:** Checking the room only on the failure path. The guard belongs in the UPDATE statement itself.

**Mobile fallback until this lands:** No chat-delete UI ships until this lands; chats must not copy the current delete call.

**Depends on:** nothing.

---

### Step 72: Make settings a field-level PATCH so old clients cannot delete new keys

**Phase:** U (Backend)
**Audit reference:** §3 [M13] "Settings writes race and overwrite unknown preference keys", backend half (audit: preserve unknown keys **or** use field-level patches); design §11 "Backend prerequisites" row M13. Evidence `apps/api/src/modules/settings/repository/pg.rs:65–72`, `apps/api/src/modules/settings/dto.rs`.
**Why (plain words):** The mobile client now sends only the fields it changed (Step 42), but the server replaces the whole object, so an older client still deletes keys it does not know about. Merging only the supplied fields on the server makes that impossible.

_Simplified from design doc §11 (a `version` column, `baseVersion`, a 409 conflict and a "changed elsewhere" UI) because audit M13 asks for unknown-key preservation or field-level patches, not cross-client versioning; safety preserved because only supplied fields change and unknown members of the stored object are kept._

**Files**

| File (exact path from repo root)                 | Action | Kind        | Required? | Notes                                                       |
| ------------------------------------------------ | ------ | ----------- | --------- | ----------------------------------------------------------- |
| `apps/api/src/modules/settings/dto.rs`           | Modify | Rust module | Required  | Patch shape: only the supplied fields.                      |
| `apps/api/src/modules/settings/service.rs`       | Modify | Rust module | Required  | Deep-merge only the supplied fields into the stored object. |
| `apps/api/src/modules/settings/repository/pg.rs` | Modify | Rust module | Required  | Write the merged object. No version column.                 |
| `apps/api/tests/settings_service.rs`             | Modify | test        | Required  | Merge matrix.                                               |

**Pseudo-code**

```
PATCH /settings
    body: { <field>: value, ... }                      // only what the client changed
    MERGE the body into the stored object DEEPLY (nested objects merged, unknown members kept)
    RETURN the full canonical object (the client adopts it)
// no version column, no baseVersion, no 409
```

**Done when:**

- A patch touching `notifications.email` leaves `notifications.push` and any unknown sibling intact.
- An unknown top-level key stored by a newer client survives a patch from an older one.
- The response is the full canonical object.

**Tests:** `apps/api/tests/settings_service.rs` — (a) deep merge preserves siblings, (b) unknown members survive, (c) an empty patch changes nothing.

**Watch out for:** a handler that still replaces the whole object when the body looks complete. Always merge.

**Mobile fallback until this lands:** the client sends only changed fields (Step 42); an older server that replaces the object is the known risk, so ship Step 72 before relying on field-level safety.

**Depends on:** Step 42.

---

## 15. Phase D — Release checks and device validation

**Findings finished here:** M17, M25, plus the device evidence for C02, C03, C04, H07, H24, M05, M06, M16, M18, M24, I02.

**What you will have when this phase is done (3–5 lines):**

- One short `docs/release-checklist.md` that a single developer can run on a device before a release (Steps 74, 75, 79, 80 are rows in it, delivery group **G14**).
- A release build that signs correctly (from a local, git-ignored `key.properties`), carries real push configuration and notification resources.
- A defined background-audio story that either works on a locked screen or is honestly labelled unsupported (default: supported, Option A).
- One recorded pass of the realtime scenarios (Step 78).

**How to know it's safe to ship:** every checklist row has an artefact (log excerpt, screenshot, test name, or a signed-off "not supported" note). Audit M24/M25 are explicit that a static statement is not evidence.

**Order:** 74 (creates the checklist) → 76+77 (G13) → 78. Rows for Steps 75, 79 and 80 are filled in as their subjects land.

**What we are deliberately NOT doing in D:** no `integration_test` files for the secure store, the upgrade or ObjectBox; no power-interruption, keystore-denial or 10-account vault scenarios; no new device features; no Media3 migration; no store listing work; no feature-flag system (hide a broken button instead); and no accessibility audit beyond noting it needs device semantics/contrast/large-text testing.

---

### Step 74: Write the release checklist and prove the secure store on a device

**Phase:** D (Device)
**Delivery group:** G14 (Steps 74, 75, 79 and 80 become sections of one `docs/release-checklist.md`).
**Audit reference:** §3 [C02] "Session-store mutations are neither atomic nor identity-conditional"; §3 [C03] "Explicit logout can leave a usable session recoverable"; §3 [M24] "Critical lifecycle tests are commented out" (evidence requirement); §3 [I02] (upgrade evidence). Evidence `apps/mobile/lib/_shared/services/secure_storage.dart`.
**Why (plain words):** Every account guarantee in P1 rests on one assumption: after a restart the vault says what we last wrote. Only a device can confirm that. One short checklist is enough for one developer; it replaces a pile of `integration_test` files that would need their own maintenance.

_Simplified from design doc §10 T43/T44 and the earlier draft of this step (power-interruption, keystore-denial and 10-account vault scenarios plus an integration test) because those exercise the platform keystore, not this app's code; safety preserved because the two scenarios that matter (login survives a kill; logout survives a kill) are still run on a real device._

**Files**

| File (exact path from repo root) | Action | Kind   | Required? | Notes                                         |
| -------------------------------- | ------ | ------ | --------- | --------------------------------------------- |
| `docs/release-checklist.md`      | Create | config | Required  | The only validation document. Sections below. |

**Pseudo-code (the whole checklist; run on one Android and one iOS device)**

| Section                                                               | Scenario                                                                                   | Pass condition                                                                                                                                  |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Secure store (this step)                                              | Log in, force-kill the app, relaunch                                                       | Still signed in                                                                                                                                 |
| Secure store (this step)                                              | Log out, force-kill the app, relaunch                                                      | Signed out; no credential for that account                                                                                                      |
| Upgrade (Step 75; only if assumption A13 says installed builds exist) | Install the last released build, sign in, save a draft, then install the new build over it | Opens signed out with a greeting; no legacy auth key remains; draft, favourites and settings intact; the release note says "sign in again once" |
| Realtime (Step 78)                                                    | The six scenarios in Step 78                                                               | As in Step 78                                                                                                                                   |
| Push taps (Step 79)                                                   | See Step 79                                                                                | As in Step 79                                                                                                                                   |
| Background audio (Step 76)                                            | Locked screen 10 minutes, phone-call interruption, OS kill                                 | As in Step 76                                                                                                                                   |
| ObjectBox upgrade (Step 80; A13 only)                                 | Upgrade over a store that has a duplicate draft                                            | Exactly one row survives, no existing UID changed, store never cleared                                                                          |
| Release build (Step 77)                                               | Build release with `key.properties` missing, then present                                  | Fails loudly, then signed artefact installs and runs                                                                                            |

**Done when:**

- The first two rows are recorded with device model, OS version and result on both platforms.
- If either fails, the release is blocked and the alternative (a vetted encrypted store behind `SecureStorage`'s boundary) is written down as the next task.
- No `integration_test` file was created for the secure store.

**Tests:** none beyond `session_store_test.dart` (Step 4). The device rows above are the evidence.

**Watch out for:** accepting "it worked once". Run each scenario twice.

**Depends on:** Steps 3, 4, 7.

---

### Step 75: Validate the upgrade path from an installed build (checklist row)

**Phase:** D (Device)
**Delivery group:** G14.
**Status:** **A six-line manual check inside `docs/release-checklist.md` (Step 74), and only if assumption A13 says an installed build with real users exists.** If A13 finds only developer devices, delete this step and reinstall instead.
**Audit reference:** §3 [C03]; §3 [M18]; §3 [M05]; §3 [M06]. Evidence `apps/mobile/lib/features/auth/services/legacy_cleanup.dart`, `apps/mobile/lib/features/broadcast/services/live_session_store.dart`.

_Simplified from design doc §7 and §10 T43 (six scenarios, an `upgrade_smoke_test` integration test, kill-at-every-write-boundary and downgrade cases) because the startup cleanup (Step 5) has no migration state to interrupt; safety preserved because the one check that matters is kept: after installing over the previous build, no token-bearing key remains and drafts survive._

**Checklist lines**

1. Install the last released build; sign in; save a draft; change a setting.
2. Install the new build over it. Expect: signed out, no crash loop.
3. (Debug build) no legacy auth key and no token-bearing recovery record remains.
4. Sign in again. Expect: draft, favourites and settings still present.
5. The release note says "signing in again is required once after this update".
6. Relaunch twice; nothing is deleted or re-created a second time.

**Tests:** none (no `upgrade_smoke_test.dart`).

**Depends on:** Steps 5, 16, 74.

---

### Step 76: Define and test the background-audio story

**Phase:** D (Platform)
**Delivery group:** G13 (Steps 76 and 77 edit the same manifests).
**Audit reference:** §3 [M17] "Background-audio support is declared but not established", Recommended fix (define supported background behavior, implement the appropriate service/audio-session lifecycle, test locked screen, interruptions and OS termination on devices); evidence `apps/mobile/lib/_shared/services/permissions_service.dart:249–290`, `apps/mobile/lib/_shared/services/permission_prompts.dart:17–24`, `apps/mobile/android/app/src/main/AndroidManifest.xml:6–40`, `apps/mobile/ios/Runner/Info.plist:72–85`.
**Why (plain words):** Background execution is declared but never enabled, broadcast gating differs from `canBroadcast`, and iOS declares processing without a task. Without a decision, "broadcasting on a locked screen" is a promise nobody verified.

**Files**

| File (exact path from repo root)                            | Action | Kind    | Required? | Notes                                                               |
| ----------------------------------------------------------- | ------ | ------- | --------- | ------------------------------------------------------------------- |
| `apps/mobile/lib/_shared/services/permissions_service.dart` | Modify | service | Required  | Gate on `canBroadcast`; request background permission explicitly.   |
| `apps/mobile/android/app/src/main/AndroidManifest.xml`      | Modify | config  | Required  | Foreground-service type for audio; remove unnecessary permissions.  |
| `apps/mobile/ios/Runner/Info.plist`                         | Modify | config  | Required  | Correct background modes; remove processing mode if no task exists. |
| `docs/release-checklist.md`                                 | Modify | config  | Required  | Add the background-audio rows (no separate file).                   |

**Pseudo-code**

```
Decision to write down (pick one and honour it)
    DEFAULT = OPTION A (assumption A15: the product is live audio; confirm with the developer)
    OPTION A (supported): foreground-service audio + correct iOS audio session
        ENABLE background execution for audio ONLY while a live session is active
        START on session enter; STOP on session close; never on mere navigation
    OPTION B (unsupported): no background execution
        BLOCK entry with a clear message; do not declare the capability in manifests

Permission gating
    canBroadcast = mic permission AND background capability (per the chosen option)
    UI uses canBroadcast, not a separate local check
```

**Done when:**

- Locked-screen audio either works for the whole session, or entry is blocked with an explanation (never "silently stops").
- An interruption (phone call) resumes exactly once.
- OS-initiated termination is handled: on relaunch the recovery intent produces a rejoin prompt, not a ghost session.
- Manifest/plist declarations match the chosen option.

**Tests:** the `docs/release-checklist.md` background-audio rows (locked screen 10 minutes, interruption, Bluetooth route change, OS kill). Automated: `apps/mobile/test/_shared/services/permissions_service_test.dart` (gating uses `canBroadcast`).

**Watch out for:** adding background modes "to see if it helps". Capabilities declared but unverified are exactly what M17 reports.

**Depends on:** Step 31.

---

### Step 77: Make the release build releasable

**Phase:** D (Platform)
**Delivery group:** G13.
**Audit reference:** §3 [M25] "Build/platform configuration is not release-ready", Recommended fix (production signing/provisioning pipeline, real notification resources, permission audit, platform build/device validation); evidence `apps/mobile/android/app/build.gradle.kts:1–5,37–49`, `apps/mobile/android/gradle.properties`, `apps/mobile/android/app/src/main/AndroidManifest.xml`, `apps/mobile/ios/Runner.xcodeproj/project.pbxproj`, `apps/mobile/ios/Runner/Info.plist`.
**Why (plain words):** Release uses debug signing, Firebase configuration and iOS entitlements are absent from the inspected tree, the referenced notification icon does not exist, and Android requests unnecessary or protected permissions while requiring camera hardware. Any of these blocks a store build.

**Files**

| File (exact path from repo root)                                    | Action | Kind   | Required? | Notes                                                                                                                                                                                                                             |
| ------------------------------------------------------------------- | ------ | ------ | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/android/app/build.gradle.kts`                          | Modify | config | Required  | Release signing config read from a local, git-ignored `key.properties`; the release build **fails if it is missing** (no debug-signing fallback). If a CI system already exists (verify, A14), it may supply the same properties. |
| `apps/mobile/android/app/src/main/AndroidManifest.xml`              | Modify | config | Required  | Remove unused/protected permissions; mark camera optional.                                                                                                                                                                        |
| `apps/mobile/android/app/src/main/res/drawable/ic_notification.xml` | Create | config | Required  | The icon the notifications code references.                                                                                                                                                                                       |
| `apps/mobile/ios/Runner/Runner.entitlements`                        | Create | config | Required  | Push entitlement.                                                                                                                                                                                                                 |
| `apps/mobile/ios/Runner/GoogleService-Info.plist`                   | Create | config | Optional  | Supplied locally (git-ignored); if absent, push stays "unavailable" (Step 45 already handles that). Loses only push in local builds.                                                                                              |
| `docs/release-checklist.md`                                         | Modify | config | Required  | The release-build row (created in Step 74).                                                                                                                                                                                       |

**Pseudo-code**

```
Android
    signingConfigs.release from the local key.properties (absent -> build fails loudly, no debug fallback)
    permissions: keep INTERNET, RECORD_AUDIO, POST_NOTIFICATIONS, FOREGROUND_SERVICE(+mediaPlayback)
                 remove anything the app never uses; declare camera/audio hardware as NOT required
    notification icon present at the referenced resource name
iOS
    entitlements: aps-environment
    Info.plist: background modes matching Step 76's decision; no unused processing mode
Verification (by hand, or in CI only if one already exists)
    a release build produces a signed artefact, and a debug build still installs
```

**Done when:**

- A release build fails if signing properties are missing (no silent debug signing).
- The notification icon resource exists and is used.
- `AndroidManifest.xml` requests no permission the app does not use; camera is optional.
- A release build installs and runs on a device with push either working or explicitly unavailable.

**Tests:** documented build verification in `docs/release-checklist.md` (signed APK/AAB produced; iOS archive succeeds; install-over-previous works). No unit test can prove provisioning.

**Watch out for:** committing keystores, `key.properties` or `GoogleService-Info.plist` with real credentials. They stay local and git-ignored; the repo holds placeholders only.

**Depends on:** Step 76 (same delivery group).

---

### Step 78: Validate the realtime protocol against the real backend

**Phase:** D (Device)
**Audit reference:** §3 [C04] "Mobile and backend use incompatible realtime protocols", Recommended fix (raw-WebSocket adapter with heartbeat, authenticated connection, bounded reconnect and reconciliation — verified, not assumed); §3 [H07] "Reconnection has stale credentials, conflicting state, and ignored terminal reasons". Evidence `apps/api/src/routes/ws.rs:241–271,492–503`.
**Why (plain words):** A local test client against a mock proves framing; only the real backend proves the handshake, the heartbeat cadence, the close codes and the recovery behaviour after a real network change.

**Files**

| File (exact path from repo root) | Action | Kind   | Required? | Notes                                                                 |
| -------------------------------- | ------ | ------ | --------- | --------------------------------------------------------------------- |
| `docs/release-checklist.md`      | Modify | config | Required  | The realtime row; keep the server and client log excerpts next to it. |
| `apps/api/tests/ws_protocol.rs`  | Create | test   | Required  | Server-side framing contract (Rust).                                  |

**Pseudo-code (manual procedure — the exact tests to run)**

| #   | Scenario                                                 | Pass condition                                                                |
| --- | -------------------------------------------------------- | ----------------------------------------------------------------------------- |
| 1   | Sign in with the backend running; enter a broadcast room | Handshake succeeds; one room event arrives within 5 seconds                   |
| 2   | Airplane mode for 60 seconds, then restore               | Client shows reconnecting, then live; no duplicate session; roster reconciles |
| 3   | Host ends the broadcast while a listener is connected    | Listener moves to `ended` once, no reconnect loop                             |
| 4   | Kick a participant from the room                         | Kicked client ends immediately; no retry                                      |
| 5   | Rotate the access token (force a refresh)                | Connection continues or reconnects once; no 401 storm                         |
| 6   | Background the app for 5 minutes and resume              | Exactly one connection; no duplicate delivery                                 |

**Done when:**

- All six scenarios recorded with backend and client logs.
- Framing matches `{event, data}` exactly (server test asserts the shape).
- Production realtime is enabled only after this passes; otherwise the app ships with the visible "realtime unavailable" state from Step 44.

**Tests:** `apps/api/tests/ws_protocol.rs` — (a) frame shape on a representative event, (b) auth rejection on upgrade with a bad token, (c) close code for a kicked participant.

**Watch out for:** Concluding from a mock or simulator run. The handshake and close codes only exist on the real server.

**Depends on:** Steps 47, 49, 77. Step 78 stays a separate step.

---

### Step 79: Validate push taps (checklist row)

**Phase:** D (Device)
**Delivery group:** G14.
**Audit reference:** §3 [H24] "Push and durable notifications are not implemented end-to-end"; §3 [M16] "Push payloads, channels, and tap lifecycle disagree", Recommended fix (cold/warm handling, recipient attribution, dedupe); design §10 T46.
**Why (plain words):** Tap handling has several entry points (foreground, background tap, terminated launch, local notification) and only a device can exercise the terminated one. These lines go into `docs/release-checklist.md`; there is no separate `push.md`.

_Simplified from design doc §10 T46 because the tap queue and account-chooser were replaced by one pending slot (Step 22); safety preserved because a tap is still never executed as the wrong account: it is dropped with a snackbar._

**Checklist lines**

| #   | Scenario                                                            | Pass condition                                                             |
| --- | ------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| 1   | Terminated app, tap a notification for the signed-in account        | App launches and lands on the destination once                             |
| 2   | Terminated app, tap a notification for a _different_ stored account | Nothing navigates; a snackbar says the notification is for another account |
| 3   | Signed-out app, tap a notification                                  | Login first, then the preserved destination                                |
| 4   | Foreground delivery                                                 | Exactly one local notification (no duplicate OS + local)                   |
| 5   | Background delivery                                                 | No local notification from the app                                         |
| 6   | Cold start from a **local** notification tap                        | Same destination slot is filled; lands once                                |

**Done when:** all six recorded; no duplicate rendering; no notification navigates to a private route before account readiness.

**Tests:** the automated `apps/mobile/test/_shared/services/push_payload_test.dart` and `apps/mobile/test/_routing/pending_destination_test.dart` from Steps 45 and 22.

**Watch out for:** testing only the warm path. Terminated launch is the one that silently drops destinations.

**Depends on:** Steps 22, 45, 77. (Step 71 is Feature track and is not required.)

---

### Step 80: Validate the ObjectBox preview schema on a real upgrade (one checklist line)

**Phase:** D (Device)
**Delivery group:** G14.
**Status:** **One line in `docs/release-checklist.md`; the upgrade part applies only if assumption A13 says an installed build exists.**
**Audit reference:** §3 [M06] "ObjectBox writes can duplicate logical rows" (schema/uniqueness half); §3 [I02] "Dependency risk is explicit, not proven obsolescence", Recommended fix (require installed-schema migration evidence before release); design §7 item 5 and §10 T45.

_Simplified from design doc §10 T45 (five scenarios and `objectbox_upgrade_test.dart`) because the audit asks for evidence, not a test harness; safety preserved because the one risky case, a store that already contains duplicates, is still tried on a device._

**Checklist line:** install the previous build, create a draft twice in quick succession, install the new build over it. Expect: exactly one row per logical key survives (newest `lastModified`), no existing UID in `objectbox-model.json` changed, and the store was not cleared. If it fails, block that subsystem; never delete the store.

**Tests:** none (no integration test).

**Depends on:** Steps 37, 46, 74.

---

## 16. Phase C — Cleanup and reference quality

**Findings finished here:** M20, M22, M24, M26, L01, L03 (timer half), L04, L05, L06.

**What you will have when this phase is done (3–5 lines):**

- Navigation that uses route names that exist.
- A dependency graph with one direction, narrow barrels, and no feature code masquerading as shared infrastructure.
- Executable lifecycle tests instead of commented-out suites, and new or repaired tests using Mocktail.
- UI that does not offer actions it cannot perform, an honest localization state, a timer that does not rebuild the world, and documentation that matches the code.

**How to know it's safe to ship:** `dart analyze` clean, the full mobile suite green including the reinstated suites, `tool/check_import_direction.sh` passing (run by hand or from a pre-push hook; in CI only if one exists), and a manual pass over the profile/settings screens confirming no enabled no-op remains.

**Order:** 81 → 82 → 83 → 84 → 85 → 86 → 87. Delivery group **G15** (Steps 82 and 86, with the one-line Step 81 riding along) is one change: files are moved and then documented. Step 85's manager half was folded into Step 29.

**What we are deliberately NOT doing in C:** no i18n framework adoption (only an honest gate), no visual redesign, no new tests for features that are being hidden, and no "cleanup" refactors of code the audit lists as done well.

---

### Step 81: Navigate by routes that exist

**Phase:** C
**Delivery group:** G15 (a one-line route name; rides along with Steps 82 and 86).
**Audit reference:** §3 [M20] "Named navigation is used for a route without that name", Recommended fix (declare a distinct route name or navigate by a constructed location with query parameters); evidence `apps/mobile/lib/features/broadcast/pages/home_page.dart:150–151,192–195,236–238`, `apps/mobile/lib/_routing/router.dart:92–96`.
**Why (plain words):** Home calls `pushNamed(MRoutes.broadcasts, ...)` while the route declares a path and no matching name, so "See all" fails at runtime.

**Files**

| File (exact path from repo root)                                | Action | Kind   | Required? | Notes                                                    |
| --------------------------------------------------------------- | ------ | ------ | --------- | -------------------------------------------------------- |
| `apps/mobile/lib/_routing/router.dart`                          | Modify | config | Required  | Declare the name, or support a query-parameter location. |
| `apps/mobile/lib/features/broadcast/pages/home_page.dart`       | Modify | widget | Required  | Use the working navigation call.                         |
| `apps/mobile/test/features/broadcast/home_navigation_test.dart` | Create | test   | Required  | Tapping "See all" navigates.                             |

**Pseudo-code**

```
Option A (preferred): named route
    GoRoute(name: MRoutes.broadcasts, path: '/broadcasts', ...)
    push(MRoutes.broadcasts, queryParameters: { 'filter': 'recent' })
Option B: location
    push('/broadcasts?filter=recent')
REQUIRE every `pushNamed`/`push(name:)` target has a matching `name:` declaration
    VERIFY with a test that enumerates route names and asserts each requested name exists
```

**Done when:**

- Tapping "See all" from home opens the broadcasts list with the intended filter.
- A test enumerates the app's declared route names and fails if a requested name is missing.
- No `pushNamed` targets an undeclared name.

**Tests:** `apps/mobile/test/features/broadcast/home_navigation_test.dart` — (a) navigation succeeds, (b) filter query arrives, (c) unknown name fails the route-name test.

**Watch out for:** Adding a name to the route but leaving the failing call site alone. Both sides of a named navigation must agree.

**Depends on:** Step 21.

---

### Step 82: Enforce the import direction and narrow the barrels

**Phase:** C
**Delivery group:** G15.
**Audit reference:** §3 [M22] "Shared infrastructure imports features and broad barrels hide the dependency graph", Recommended fix (move broadcast feed/session UI into broadcast, auth persistence into auth, app shells into an app layer; use narrow imports; keep useful feature barrels); design §8 "Required boundary-only moves". Evidence `apps/mobile/lib/_shared/_shared.dart:1–23`, `apps/mobile/lib/features/auth/manager/user_manager.dart:15–17`.
**Why (plain words):** `_shared` re-exports feature code, so nobody can tell which direction a dependency points, and four more features will make it worse. Narrow barrels plus an import-direction script keep the graph one-way.

**Files**

| File (exact path from repo root)                                  | Action             | Kind    | Required?    | Notes                                                                                                                                                                                          |
| ----------------------------------------------------------------- | ------------------ | ------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/_shared/_shared.dart`                            | Modify             | config  | Required     | Export only genuinely shared files.                                                                                                                                                            |
| `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart` | Move               | manager | Required     | Already moved in Step 60; this removes the barrel export.                                                                                                                                      |
| `apps/mobile/lib/_shared/pages/live_session_init_page.dart`       | Move               | widget  | Required     | Already moved in Step 32; remove the barrel export.                                                                                                                                            |
| `apps/mobile/lib/_shared/layouts/root_shell.dart`                 | Move               | widget  | Required     | Already moved in Step 33; remove export.                                                                                                                                                       |
| `apps/mobile/lib/_shared/services/database.dart`                  | **Leave in place** | service | Required     | The audit's own reference tree keeps `database.dart` in `_shared/services` (audit §7, A-8). Do not move it. If it imports feature entities, fix that with a registration callback, not a move. |
| `apps/mobile/lib/features/features.dart`                          | Delete             | config  | **Required** | Umbrella exports are rated "No" in the audit's over-engineering ledger (audit §4). Call sites import the narrow feature file.                                                                  |
| `tool/check_import_direction.sh`                                  | Create             | config  | Required     | The import-direction guard. Run by hand before merging, or from a git pre-push hook. If a CI system already exists (verify, A14), run it there too.                                            |

**Pseudo-code**

```
LEAVE database.dart in _shared/services (audit §7 reference tree)
REMOVE feature-dependent exports from _shared barrels
    // auth, broadcast pages/layouts/feeds, anything importing features/
DELETE umbrella barrels that re-export whole features
    KEEP each feature's own small barrels (_pages, _widgets, _model, _services)

Import-direction script (fails the run it is called from)
    rg -n "import 'package:meno_mobile/(features|app)/" apps/mobile/lib/_shared -> must be empty
    rg -n "import 'package:meno_mobile/app/" apps/mobile/lib/features -> must be empty
    rg -n "^export 'package:meno_mobile/features/" apps/mobile/lib/_shared -> must be empty
```

**Done when:**

- `tool/check_import_direction.sh` passes and would fail if a shared file imported a feature.
- `_shared` contains no feature-aware page, layout or manager.
- One ObjectBox handle still exists; `database.dart` did not move.
- `dart analyze` clean after the moves.

**Tests:** the import-direction script itself plus `apps/mobile/test/locator_test.dart` (registrations still resolve). No behaviour test: this is a structural change with behaviour unchanged.

**Watch out for:** adding a shim export to "avoid touching imports". That defeats the whole step.

**Depends on:** Steps 60, 32, 33, 36.

---

### Step 83: Confirm no lifecycle test is still commented out, and fix the mocking convention

**Phase:** C
**Audit reference:** §3 [M24] "Critical lifecycle tests are commented out", Recommended fix (reinstate focused tests using controllable futures and fakes; do not merely uncomment obsolete expectations); §3 [I02] "Dependency risk is explicit, not proven obsolescence", part "standardize mocking when tests are repaired". Evidence `apps/mobile/test/features/auth/user_scope_manager_test.dart:1–187`, `apps/mobile/test/features/broadcast/live_scope_manager_test.dart:1–302`, `apps/mobile/test/features/broadcast/stream_manager_test.dart:1–97`, `apps/mobile/test/features/profile/my_profile_manager_test.dart:1–190`.
**Why (plain words):** Four complete suites were comments. Earlier steps already replaced them with real tests where the code now lives: the transition owner (Step 18), the live manager (Steps 29–30), the profile manager (Step 26). This step only proves nothing is left commented out and writes down the mocking convention.

_Simplified from design doc §10 (re-listing tests T21–T26, an exit-contract suite and a stream-manager suite) because those tests are written in the steps that own the behaviour, the exit-contract step was removed (Step 27) and `StreamManager` was deleted (Step 34); safety preserved because the grep below fails while any commented-out test remains._

**Files**

| File (exact path from repo root) | Action | Kind   | Required? | Notes                                                                                               |
| -------------------------------- | ------ | ------ | --------- | --------------------------------------------------------------------------------------------------- |
| `apps/mobile/test/README.md`     | Create | config | Required  | One paragraph: new and repaired tests use Mocktail; fakes are concrete classes in `test/_support/`. |

**Pseudo-code**

```
ASSERT no commented-out test bodies remain
    rg -n "^\s*//\s*(test|group|expect)\(" apps/mobile/test -> empty
DELETE any remaining commented-out file whose subject was removed
    (user_scope_manager_test.dart, live_scope_manager_test.dart, stream_manager_test.dart)
RECORD the executable test count (`flutter test`) in the phase exit review
```

**Done when:**

- The grep above returns nothing.
- `test/README.md` states the Mocktail convention for new and repaired tests (untouched Mockito tests are not converted).
- `flutter test` is green and the executable count is higher than before the remediation.

**Tests:** none new; the suites from Steps 18, 26, 30 and 34 are the tests.

**Watch out for:** uncommenting old expectations that assert the removed design (scope stacks, signal-ready). Rewrite them against the current contract.

**Depends on:** Steps 18, 26, 30.

---

### Step 84: Stop presenting unimplemented behaviour as working

**Phase:** C
**Audit reference:** §3 [M26] "UI presents incomplete behavior as operational", Recommended fix (disable/hide unsupported actions; implement coherent capabilities before labeling the feature complete; bound the list and use real search); §3 [L04] "Small UI/validation mismatches remain", Recommended fix (align copy, validation and metrics with actual capabilities/contracts). Evidence `apps/mobile/lib/features/broadcast/pages/broadcast_details_page.dart:117–120`, `apps/mobile/lib/features/broadcast/pages/ended_broadcast_page.dart:105–107`, `apps/mobile/lib/features/broadcast/pages/broadcast_editor_page.dart:424–432`, `apps/mobile/lib/features/broadcast/widgets/add_cohost_modal.dart:33–74`, `apps/mobile/lib/features/broadcast/widgets/broadcast_control_buttons.dart:97–99`, `apps/mobile/lib/features/broadcast/widgets/broadcast_card.dart:58–67`, `apps/mobile/lib/features/settings/pages/security_settings_page.dart:25–31`.
**Why (plain words):** Restream, listen-back, password save and deletion are enabled no-ops; cohost search is unwired with fake profiles and inconsistent limits; listener leave uses host confirmation copy; the title rule omits the backend's 3–100 policy; the UI says 10 MB against a 5 MiB limit; and "live count" shows cumulative attendance. Each one teaches a user something false.

**Files**

| File (exact path from repo root)                                            | Action | Kind    | Required? | Notes                                                                                                    |
| --------------------------------------------------------------------------- | ------ | ------- | --------- | -------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/pages/broadcast_details_page.dart`      | Modify | widget  | Required  | Hide restream/listen-back until implemented.                                                             |
| `apps/mobile/lib/features/broadcast/widgets/add_cohost_modal.dart`          | Modify | widget  | Required  | Real search, bounded list, consistent limit, **and fix the unbounded-flex layout** (audit M26 evidence). |
| `apps/mobile/lib/features/broadcast/pages/ended_broadcast_page.dart`        | Modify | widget  | Required  | Hide or disable the enabled no-op at `:105–107` (audit M26 evidence).                                    |
| `apps/mobile/lib/features/broadcast/pages/broadcast_editor_page.dart`       | Modify | widget  | Required  | Hide or disable the enabled no-op at `:424–432` (audit M26 evidence).                                    |
| `apps/mobile/lib/features/settings/pages/settings_page.dart`                | Modify | widget  | Required  | Hide or disable the enabled no-op at `:113–118` (audit M26 evidence).                                    |
| `apps/mobile/lib/features/broadcast/widgets/broadcast_control_buttons.dart` | Modify | widget  | Required  | Listener-specific leave copy.                                                                            |
| `apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart`  | Modify | manager | Required  | Title 3–100 validation; image limit 5 MiB copy.                                                          |
| `apps/mobile/lib/features/broadcast/widgets/broadcast_card.dart`            | Modify | widget  | Required  | Presence count, not cumulative attendance.                                                               |
| `apps/mobile/lib/features/settings/pages/security_settings_page.dart`       | Modify | widget  | Required  | Disable password save until wired.                                                                       |

**Pseudo-code**

```
RULE: an action that cannot succeed is either hidden or explicitly labelled "coming soon"
    restream / listen-back / password-save / delete-account / the three no-ops above -> HIDE the control (or disable it with a label)
    // no feature-flag system: hiding a broken button is a one-line change and is all the audit asks for
COHOST MODAL
    REAL search endpoint; DEBOUNCE input; BOUND the list (e.g. 20) and page it
    LIMIT consistent with the backend constant (one source)
COPY
    listener leave: "Leave broadcast?"   host leave: "End broadcast?"
TITLE
    REQUIRE 3..100 characters, matching the backend validator
IMAGE
    state 5 MiB, matching the backend limit
COUNTER
    "X listening" reads the authoritative present count (Step 54)
```

**Done when:**

- No enabled control performs nothing (click every action once on a device), including the three extra pages named in the audit evidence.
- The add-cohost modal lays out without an unbounded-flex error.
- Cohost search returns real users and never renders a fake profile.
- Title/limit copy matches the backend constants, with a test asserting the numbers.
- Listener leave copy is listener-specific.

**Tests:** `apps/mobile/test/features/broadcast/ui_copy_contract_test.dart` — (a) title policy matches the backend constant, (b) image limit string matches, (c) listener/host copy differs, (d) disabled actions are not tappable.

**Watch out for:** Hiding a broken action without telling anyone. Label first, remove later.

**Depends on:** Steps 54, 41.

---

### Step 85: Extract the elapsed label so the clock does not rebuild the tab

**Phase:** C
**Audit reference:** §3 [L01] "Rebuild granularity is too broad for a one-second clock", Recommended fix (a small watching timer widget; keep stable artwork/header; do not split every field); §3 [L03] "Timer semantics and retry timing are inaccurate", Recommended fix (monotonic elapsed anchor with explicit server reconciliation) — the manager half is done once, in Step 29 (D-19). Evidence `apps/mobile/lib/features/broadcast/widgets/broadcast_tab_widget.dart:12–26,28–78`, `apps/mobile/lib/features/broadcast/manager/broadcast_timer_manager.dart:92–94,140–142`.
**Why (plain words):** One clock change rebuilds the whole tab including artwork. The monotonic anchor and server reconciliation already landed with the timer-manager rewrite in Step 29, so the manager is not touched a second or third time. What remains is the widget: one small watching label.

**Files**

| File (exact path from repo root)                                       | Action | Kind   | Required? | Notes                                        |
| ---------------------------------------------------------------------- | ------ | ------ | --------- | -------------------------------------------- |
| `apps/mobile/lib/features/broadcast/widgets/broadcast_tab_widget.dart` | Modify | widget | Required  | Extract `_ElapsedLabel` as the only watcher. |
| `apps/mobile/test/features/broadcast/broadcast_tab_rebuild_test.dart`  | Create | test   | Required  | Rebuild scope.                               |

**Pseudo-code**

```
BroadcastTabWidget (plain StatelessWidget)
    artwork, header, controls         // not watching
    _ElapsedLabel (WatchingWidget)    // the only thing watching elapsed
```

**Done when:**

- A one-second tick does not rebuild artwork/header (widget test counting builds of the header).
- The timer manager was not modified in this step.

**Tests:** `apps/mobile/test/features/broadcast/broadcast_tab_rebuild_test.dart` — the header build count stays constant across ticks. (Anchor semantics are tested in `broadcast_timer_manager_test.dart`, Step 29.)

**Watch out for:** splitting every field into its own widget. One small label is the point.

**Depends on:** Step 29.

---

### Step 86: Remove dead paths and make the documentation true

**Phase:** C
**Delivery group:** G15.
**Audit reference:** §3 [L05] "Dead paths, fixtures, and documentation obscure the reference implementation", Recommended fix (remove or isolate unused implementations/fixtures, correct lifecycle comments, document bootstrap and decisions); evidence `apps/mobile/lib/features/broadcast/model/entities.dart:40–166`, `apps/mobile/lib/features/profile/manager/user_profile_manager.dart:3–8`, `apps/mobile/README.md`.
**Why (plain words):** Duplicate live-tab code (already removed in Step 35), mutable sample lists inside production models, redundant imports and a template README all invite a developer to copy the wrong thing. Comments that describe behaviour the code no longer has are worse than no comments.

**Files**

| File (exact path from repo root)                                           | Action | Kind    | Required? | Notes                                                             |
| -------------------------------------------------------------------------- | ------ | ------- | --------- | ----------------------------------------------------------------- |
| `apps/mobile/lib/features/broadcast/model/entities.dart`                   | Modify | model   | Required  | Move sample/mutable lists to a `fixtures/` file marked test-only. |
| `apps/mobile/lib/features/broadcast/model/fixtures/broadcast_samples.dart` | Create | test    | Required  | Fixtures live here, not in production models.                     |
| `apps/mobile/README.md`                                                    | Modify | config  | Required  | Real bootstrap, DI tiers, decisions, links to the audit/design.   |
| `apps/mobile/lib/features/profile/manager/user_profile_manager.dart`       | Modify | manager | Required  | Remove redundant imports.                                         |

**Pseudo-code**

```
DELETE or MOVE
    sample/mutable broadcast lists out of production models -> fixtures file used only by tests
    redundant imports
REWRITE comments in scope/DI/session files
    keep short invariants that the code enforces
    delete claims the code does not make
README
    bootstrap: main -> bootstrap() -> root registrations -> transition owner -> gate
    two DI tiers and what belongs in each
    how to run tests, build_runner, and docs/release-checklist.md
    links: audit-report.md, account-di-migration.md, this guide
    localization: three lines (single locale today; strings sit at the top of each screen file; add keys before expanding copy)   // replaces docs/localization-plan.md
```

**Done when:**

- No mutable sample data remains in a production model file.
- Every lifecycle comment in the DI/auth/broadcast files matches the code (spot-check with `rg -n "guarantee|always|never"` in those files).
- The README describes the actual bootstrap and DI tiers.
- `dart analyze` clean.

**Tests:** no new behavioural tests; the existing suite must stay green after the moves (`flutter test`).

**Watch out for:** Rewriting comments to describe intent while the code still does something else. Fix the code or delete the claim.

**Depends on:** Steps 35, 82.

---

### Step 87: State the localization limitation and stop growing it

**Phase:** C
**Audit reference:** §3 [L06] "Language setting is not connected to localization", Recommended fix (introduce localization before expanding UI copy; explicitly label unsupported language changes meanwhile); evidence `apps/mobile/lib/features/settings/manager/settings_manager.dart:29–38`, `apps/mobile/lib/app.dart:85–94`.
**Why (plain words):** The language preference is stored remotely but no locale or delegate is wired, so the setting appears to do something it does not.

**Conscious deviation from the audit (A-11, assumption A10):** the audit says to introduce localization _before_ expanding UI copy. **That is deferred here, deliberately**, because adding an l10n framework is a project of its own for one developer. What is kept: the unsupported language change is explicitly labelled (the audit's "meanwhile" clause). What is replaced: the unenforceable rule "no new hard-coded copy without a note" is swapped for something a reviewer can check, **each screen's strings live in one block at the top of its file**, so a later move to keys is mechanical. Steps 84 and 71 add copy under this rule.

**Files**

| File (exact path from repo root)                                  | Action | Kind    | Required? | Notes                                                                              |
| ----------------------------------------------------------------- | ------ | ------- | --------- | ---------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/settings/pages/settings_page.dart`      | Modify | widget  | Required  | Language row shows "not yet available" and is disabled.                            |
| `apps/mobile/README.md`                                           | Modify | config  | Required  | Three lines on the localization limitation (replaces `docs/localization-plan.md`). |
| `apps/mobile/lib/features/settings/manager/settings_manager.dart` | Modify | manager | Required  | Keep storing the preference; no implied effect.                                    |

**Pseudo-code**

```
UI
    language row: DISABLED with subtitle "Language support is not available yet"
    // never render a selectable option that changes nothing
README (three lines)
    1. Single locale, hard-coded copy; deferred from audit L06 on purpose (one developer).
    2. Each screen keeps its strings in one block at the top of its file.
    3. Before expanding copy further: add delegates, ARB files and a locale from settings.
```

**Done when:**

- The language setting cannot be changed to a value that has no effect.
- The README says in three lines that localization is deferred on purpose and where strings live.
- `docs/localization-plan.md` was not created.

**Tests:** `apps/mobile/test/features/settings/settings_page_test.dart` — (a) language row disabled with the explanation, (b) the stored preference is not deleted.

**Watch out for:** adding a disabled language row for later. One honest gate, not a fake selector.

**Depends on:** Step 42.

---

## 16b. Feature track — notifications and feature gates (after the remediation)

These three steps are **not** part of the remediation. Audit §6 treats notifications as upcoming-feature work gated on a backend contract, so the remediation is complete without them. Build them after Phase C. They are separate so a failed inbox design cannot block the security and lifecycle fixes. Checklist for this track: the inbox routes and table (Step 70), the account coordinator with per-account token association (Step 71), and the README link to audit §6 (Step 73). Titles and bodies are plain text (assumption A10).

---

### Step 70: Give notifications a durable inbox and a device-token policy

**Phase:** Feature track (Backend) _(moved out of the remediation track, review OE-20)_
**Audit reference:** §3 [H24] "Push and durable notifications are not implemented end-to-end", Recommended fix (define durable inbox identity/pagination/read state and device-token policy; implement the account-scoped coordinator afterwards); audit §6 "Notifications — Backend reality". Evidence `apps/api/src/modules/mod.rs:16–21,70–76`, `apps/api/src/routes/settings.rs:36–55`, `apps/api/src/infrastructure/ws/dto.rs:90–101`, `apps/api/src/infrastructure/constants.rs:75–97`, `apps/api/src/worker.rs:47–59`.
_Feature track, not remediation: audit §6 treats notifications as upcoming-feature work gated on this backend contract. It is built after Phase C, and the remediation is "done" without it._
**Why (plain words):** A single push-token field per user, socket events with no durable id or read state, a best-effort 50-event buffer and no list/read routes mean notifications are not a feature — they are adapters. A real inbox needs its own table, ids, read state and pagination, or every client will treat a socket event as durable truth.

**Files**

| File (exact path from repo root)                            | Action | Kind        | Required? | Notes                                        |
| ----------------------------------------------------------- | ------ | ----------- | --------- | -------------------------------------------- |
| `apps/api/src/modules/notification/mod.rs`                  | Create | Rust module | Required  | Module root.                                 |
| `apps/api/src/modules/notification/model.rs`                | Create | Rust module | Required  | Inbox row, read state, recipient.            |
| `apps/api/src/modules/notification/dto.rs`                  | Create | Rust module | Required  | List page, unread count, mark-read body.     |
| `apps/api/src/modules/notification/service.rs`              | Create | Rust module | Required  | Create/list/read/unread + token association. |
| `apps/api/src/modules/notification/repository/pg.rs`        | Create | Rust module | Required  | Cursor `(created_at, id)` descending.        |
| `apps/api/src/modules/notification/handlers.rs`             | Create | Rust module | Required  | HTTP surface.                                |
| `apps/api/src/routes/notifications.rs`                      | Create | Rust module | Required  | Route wiring.                                |
| `apps/api/src/routes/mod.rs`, `apps/api/src/modules/mod.rs` | Modify | Rust module | Required  | Registration.                                |
| `apps/api/tests/notification_router.rs`                     | Create | test        | Required  | Inbox contract.                              |

**Pseudo-code**

```
Table notifications
    id (uuid, durable), user_id, kind, title, body (plain text; no localisable keys, D-21 / assumption A10),
    deep_link, data_json (no secrets), created_at, read_at nullable, source_id
    INDEX (user_id, created_at DESC, id DESC)

Routes
    GET    /notifications?cursor=&limit=        -> page { items, nextCursor, unreadCount }
    POST   /notifications/{id}/read             -> 204 (idempotent)
    POST   /notifications/read-all              -> 204
    GET    /notifications/unread-count          -> { count }
    PUT    /devices/{deviceId}/token            -> associate token with THIS user
    DELETE /devices/{deviceId}/token            -> remove this user's association only

Rules
    EVERY query is scoped by user_id from the verified subject      // never a client-supplied id
    A push is a hint; the row is the truth
    Socket payload for a notification carries notificationId + unreadCount (durable, actionable)
    Token association is per user+device; deleting one user's association leaves other users' intact
    The worker drains queued notifications; a failure retries with backoff and never duplicates ids
```

**Done when:**

- Listing returns only the caller's rows, newest first, cursor-paged.
- Marking read twice is idempotent; unread count reflects it immediately.
- Two accounts on one device each keep their own token association.
- A socket notification event carries a durable id that can be fetched.
- No route accepts a user id from the request body.

**Tests:** `apps/api/tests/notification_router.rs` — (a) per-user isolation, (b) pagination, (c) read idempotency, (d) two-account token association, (e) cursor stability for equal timestamps.

**Watch out for:** treating the existing 50-event socket buffer as the inbox. It is a hint buffer with duplication risk; the table is the source of truth.

**Depends on:** Step 65 (verified subject semantics).

---

### Step 71: Make the client honest about notifications

**Phase:** Feature track (Mobile) _(moved out of the remediation track, review OE-20)_
**Audit reference:** §3 [H24] "Push and durable notifications are not implemented end-to-end", mobile half; §3 [M16] "Push payloads, channels, and tap lifecycle disagree", Recommended fix (recipient attribution, validated destinations, contract fixtures). Evidence `apps/mobile/lib/features/notification/*`, `apps/mobile/lib/main.dart:6–23`.
_Feature track, not remediation._
**Why (plain words):** With Step 70 the inbox exists; without it, the app must say so instead of showing an empty list that looks like "no notifications". This step wires the coordinator to whichever contract is live and makes the unavailable state explicit.

**Files**

| File (exact path from repo root)                                                | Action     | Kind    | Required? | Notes                                                                                                                                                                                                                                                                                                         |
| ------------------------------------------------------------------------------- | ---------- | ------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/mobile/lib/features/notification/manager/notification_coordinator.dart`   | **Create** | manager | Required  | Created **here**, after the inbox exists (A-7; audit DI map: "when backend contract exists"). Inbox state, unread count, read actions, and token association (`registerDeviceFor(accountId)` on ready, `unregisterDeviceFor(accountId)` on exit; never a device-wide clear). Registered in the account scope. |
| `apps/mobile/lib/features/notification/services/notification_http_service.dart` | Create     | service | Required  | List/read/unread/token association.                                                                                                                                                                                                                                                                           |
| `apps/mobile/lib/features/notification/pages/_pages.dart` / notification page   | Modify     | widget  | Required  | Uses the coordinator; explicit unavailable state.                                                                                                                                                                                                                                                             |
| `apps/mobile/test/features/notification/notification_coordinator_test.dart`     | Create     | test    | Required  | Read/unread + unavailable.                                                                                                                                                                                                                                                                                    |

**Pseudo-code**

```
NotificationCoordinator (account scope)
    unread : ValueNotifier<int>
    inbox  : widget-owned feed source in the page (cursor), NOT this manager
    availability : ValueNotifier<NotificationsAvailability>   // available | backendUnavailable

FUNCTION initialize()
    TRY
        count = AWAIT http.unreadCount()
        unread.value = count
        availability = available
    CATCH notImplemented/404 -> availability = backendUnavailable      // honest, not empty

FUNCTION markRead(id)
    AWAIT http.markRead(id)                      // idempotent
    unread.value = max(0, unread.value - 1)      // then reconcile from the server count

Page
    IF availability == backendUnavailable SHOW "notifications are not available yet"
    ELSE render the cursor feed; an empty inbox is then genuinely empty
```

**Done when:**

- Against a backend without the inbox, the page says "not available", not "no notifications".
- Against Step 70, marking read updates the badge and survives a restart.
- The unread count is per account (switching accounts shows the other account's count).
- No page constructs its own unread state.

**Tests:** `apps/mobile/test/features/notification/notification_coordinator_test.dart` — (a) unavailable state, (b) read decrements and reconciles, (c) per-account count, (d) idempotent read.

**Watch out for:** Rendering an empty list when the backend lacks the endpoint. Empty is a claim; say unavailable instead.

**Depends on:** Steps 45, 70. Titles and bodies are shown as plain text (D-21).

---

### Step 73: Point the feature gates at the audit's readiness section

**Phase:** Feature track (Contracts) _(moved out of the remediation track, review OE-20)_
**Audit reference:** audit §6 "Template Readiness for Upcoming Features" (all four subsections: Backend reality, Carries over, Breaks if copied, Placement, Build first). No individual finding ID — this is the contract gate the audit asks for before UI expansion.
**Why (plain words):** Each upcoming feature has a documented backend reality that differs from the assumed one (broadcast-room chat only, cursor sync with conflicts, no Discover endpoint, no durable inbox). The audit already writes that gate down. A second copy would drift, so this step adds a link, not a new document.

_Simplified from design doc §9 (a separate `docs/feature-readiness.md` with checklists and optional seed files) because the audit section is already the gate; safety preserved because the README points every feature at it and the linked remediation steps own the executable coverage._

**Files**

| File (exact path from repo root) | Action | Kind   | Required? | Notes                                                                                                              |
| -------------------------------- | ------ | ------ | --------- | ------------------------------------------------------------------------------------------------------------------ |
| `apps/mobile/README.md`          | Modify | config | Required  | One line: "Before building chats, notes, discover or notifications read audit §6 and tick its _Build first_ list." |

**Done when:**

- The README links audit §6.
- No optional seed files (`chat_http_service.dart`, `notes_sync_manager.dart`) were created.
- No new feature UI is started until the _Build first_ items for it are done.

**Tests:** none (documentation gate).

**Watch out for:** treating the link as a formality. Each unchecked _Build first_ item is a feature that will copy a current defect (Step 68 and Step 69 are prerequisites for notes and chat).

**Depends on:** nothing (Steps 65–72 are the facts it points at).

---

---

## 17. Test plan summary

One row per test file this guide creates or rewrites (trimmed per the review: no test is kept for a mechanism that was removed, and there are no `integration_test` files). "What it proves" is the single claim the file exists to defend. Columns are the audit findings the file covers.

| Test file                                                                     | What it proves                                                                                                                      | Finding IDs                                           |
| ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| `apps/mobile/test/_support/fake_clock_test.dart`                              | Injected time moves deterministically                                                                                               | I02 (tooling)                                         |
| `apps/mobile/test/_support/fake_secure_storage_test.dart`                     | Storage read/write failures are observable                                                                                          | C02                                                   |
| `apps/mobile/test/features/auth/credential_record_test.dart`                  | Vault decoding fails closed on every malformed field                                                                                | C02, C03, H10                                         |
| `apps/mobile/test/features/auth/session_store_test.dart`                      | Vault mutations are serialized and conditional; a failed write leaves memory unchanged; legacy keys are deleted at startup (A13)    | C02, C03, H10                                         |
| `apps/mobile/test/features/auth/auth_expiry_test.dart`                        | Absolute expiry never moves on read                                                                                                 | H10                                                   |
| `apps/mobile/test/features/auth/auth_manager_test.dart`                       | Logout is durable even when the removal write fails; add-account is explicit; slow logins lose                                      | C02, C03, M02                                         |
| `apps/mobile/test/features/auth/session_interceptor_test.dart`                | A request can never execute as another account or origin, and completes once                                                        | C01, M15                                              |
| `apps/mobile/test/features/auth/refresh_outcome_test.dart`                    | Four rotation outcomes are classified, `inFlight` at startup is uncertain, single-use tokens never retried                          | H09, C02                                              |
| `apps/mobile/test/_shared/services/http_client_test.dart`                     | Retries preserve identity and honour server minimums                                                                                | M15, L03                                              |
| `apps/mobile/test/_core/value_objects/validation_parity_test.dart`            | Client and server accept the same accounts                                                                                          | H25                                                   |
| `apps/mobile/test/_shared/model/dtos_test.dart`                               | Server-valid values map without throwing                                                                                            | H25, H10                                              |
| `apps/api/tests/auth_validators.rs`                                           | Server side of the same fixtures                                                                                                    | H25                                                   |
| `apps/mobile/test/_core/logging/meno_logger_test.dart`                        | Production reports failures without leaking secrets                                                                                 | M23, M15                                              |
| `apps/mobile/test/_core/config/config_test.dart`                              | Invalid production configuration fails loudly                                                                                       | M18                                                   |
| `apps/mobile/test/features/broadcast/live_exit_fence_test.dart`               | The old live tier cannot outlive a transition (skip with Step 15 if P1–P3 ship together)                                            | H02                                                   |
| `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart`       | Owner isolation, foreign rows rejected, no duplicate logical rows, queries closed                                                   | M05, M06, H14                                         |
| `apps/mobile/test/app/account_transition_owner_test.dart`                     | One FIFO owner publishes readiness, rolls back partial scopes, and `AccountContext` equality holds                                  | H01, M14                                              |
| `apps/mobile/test/app/account_gate_test.dart`                                 | Old subtree disposes before pop (one frame); new generation remounts                                                                | H01, H15                                              |
| `apps/mobile/test/app/bootstrap_retry_test.dart`                              | Retry rebuilds failed registrations instead of re-awaiting (one small test)                                                         | H01, M24                                              |
| `apps/mobile/test/_routing/router_guard_test.dart`                            | Router refreshes on identity and preserves destinations                                                                             | H01, M19                                              |
| `apps/mobile/test/_routing/pending_destination_test.dart`                     | One pending slot holds a validated destination; a tap for another account is dropped with a snackbar                                | M19, M16                                              |
| `apps/mobile/test/features/auth/email_verification_manager_test.dart`         | Only a tap sends a code; one error, one message                                                                                     | M01, L02                                              |
| `apps/mobile/test/features/profile/my_profile_manager_test.dart`              | Account managers initialize once and discard late work                                                                              | M14, H15, M12                                         |
| `apps/mobile/test/features/profile/current_user_presentation_test.dart`       | One presentation source drives chrome                                                                                               | M12, L02                                              |
| `apps/mobile/test/features/broadcast/live_snapshot_test.dart`                 | Phase + capabilities are separated and value-stable                                                                                 | H08                                                   |
| `apps/mobile/test/features/broadcast/active_broadcast_session_test.dart`      | Session resources exist from construction and close exactly once                                                                    | H03, H04                                              |
| `apps/mobile/test/features/broadcast/active_broadcast_manager_test.dart`      | Enter/replace/close/recover cannot overlap; `recover()` does not deadlock; a throwing `start()` leaks nothing; `join` delegates     | H02, M21                                              |
| `apps/mobile/test/locator_test.dart`                                          | Exactly two application scopes exist                                                                                                | M21, H02                                              |
| `apps/mobile/test/features/broadcast/live_phase_routing_test.dart`            | Live surfaces follow the phase, not registration                                                                                    | H04, M21                                              |
| `apps/mobile/test/features/broadcast/ended_page_test.dart`                    | The ended page works with no session to resolve                                                                                     | H04                                                   |
| `apps/mobile/test/features/broadcast/broadcast_timer_manager_test.dart`       | Monotonic elapsed with server reconciliation (written in Step 29)                                                                   | L03                                                   |
| `apps/mobile/test/features/broadcast/broadcast_tab_rebuild_test.dart`         | The clock rebuilds only the elapsed label                                                                                           | L01                                                   |
| `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart`       | No duplicate logical rows; queries closed                                                                                           | M06                                                   |
| `apps/mobile/test/features/profile/profile_cache_test.dart`                   | Viewer-relative state cannot leak across accounts                                                                                   | H14                                                   |
| `apps/mobile/test/features/profile/user_profile_repository_test.dart`         | Proxy identity is stable and refcounts reach zero                                                                                   | H13                                                   |
| `apps/mobile/test/features/profile/user_profile_page_test.dart`               | No DI lookup for a page-owned proxy                                                                                                 | M04                                                   |
| `apps/mobile/test/features/profile/profile_editor_manager_test.dart`          | Editor compares like types and validates bio                                                                                        | M11, H15                                              |
| `apps/mobile/test/features/settings/settings_manager_test.dart`               | Settings go through one serial queue as field-level patches; a late GET does not clobber a queued edit                              | M13                                                   |
| `apps/mobile/test/features/auth/account_connection_owner_test.dart`           | Handlers first, credential supplier read at connect, stop before dispose                                                            | H06, M03                                              |
| `apps/mobile/test/_shared/services/push_payload_test.dart`                    | Payload and channel contracts match the backend                                                                                     | M16, H24                                              |
| `apps/mobile/test/_shared/services/socket_client_test.dart`                   | Raw-WebSocket framing, heartbeat, bounded reconnect                                                                                 | C04                                                   |
| `apps/mobile/test/features/broadcast/broadcast_socket_service_test.dart`      | Install/detach is explicit and idempotent                                                                                           | H06                                                   |
| `apps/mobile/test/features/broadcast/live_reconnect_test.dart`                | Terminal reasons end; network reasons reconnect                                                                                     | H07                                                   |
| `apps/mobile/test/features/broadcast/broadcast_event_decode_test.dart`        | Strict typed decoding; ended ≠ summary                                                                                              | M10                                                   |
| `apps/mobile/test/features/broadcast/broadcast_contract_test.dart`            | End/cohost contracts match the checked-in fixture; `removeFromRoom` is required                                                     | H05                                                   |
| `apps/api/tests/broadcast_contract.rs`                                        | The backend output equals that same checked-in fixture                                                                              | H05                                                   |
| `apps/api/tests/broadcast_roster_events.rs`                                   | Hosts get deltas, listeners get none; counts are present-only                                                                       | H16                                                   |
| `apps/mobile/test/features/broadcast/participants_manager_test.dart`          | Present-only roster across cursor pages                                                                                             | H16                                                   |
| `apps/api/tests/broadcast_service.rs`                                         | Atomic cohost revocation and retry-safe transitions                                                                                 | H17, H18                                              |
| `apps/mobile/test/features/broadcast/broadcast_editor_manager_test.dart`      | One operation with honest phases and retry                                                                                          | H19                                                   |
| `apps/mobile/test/features/broadcast/idempotency_key_test.dart`               | Keys belong to logical operations                                                                                                   | H20                                                   |
| `apps/api/tests/idempotency.rs`                                               | Only if Step 58's read-first check finds no stored-outcome replay                                                                   | H20                                                   |
| `apps/mobile/test/features/broadcast/favourites_manager_test.dart`            | One reactive favourites source                                                                                                      | M07                                                   |
| `apps/mobile/test/_shared/manager/feed_data_source_test.dart`                 | Query generations and refresh/page arbitration                                                                                      | H11                                                   |
| `apps/mobile/test/features/broadcast/filtered_feed_test.dart`                 | Only if the backend has no ended filter (Step 61): filtered empty pages still make progress                                         | H12                                                   |
| `apps/mobile/test/_shared/widgets/feed_widget_test.dart`                      | Page errors have their own footer and retry                                                                                         | M08                                                   |
| `apps/mobile/test/features/broadcast/feed_event_binding_test.dart`            | Rebinding and predicates are correct                                                                                                | M09                                                   |
| `apps/api/tests/auth_router.rs`                                               | Account-wide revocation and reset ordering                                                                                          | C05, H21                                              |
| `apps/api/tests/auth_service.rs`                                              | Distinct refresh error codes; a 429 does not consume the token                                                                      | H09                                                   |
| `apps/api/tests/notes_repository_pg.rs`                                       | Same-batch folder assignment; a PATCH omitting `folder_id` keeps the folder (PostgreSQL)                                            | H22                                                   |
| `apps/api/tests/chat_service.rs`                                              | Cross-room deletion is impossible                                                                                                   | H23                                                   |
| `apps/api/tests/notification_router.rs`                                       | **Feature track:** durable inbox, isolation, token policy                                                                           | H24                                                   |
| `apps/api/tests/settings_service.rs`                                          | Field-level deep merge keeps unknown members                                                                                        | M13                                                   |
| `apps/mobile/test/features/notification/…` (page test)                        | Explicit unavailable state                                                                                                          | H24                                                   |
| `apps/mobile/test/features/broadcast/home_navigation_test.dart`               | Named navigation targets exist                                                                                                      | M20                                                   |
| `tool/check_import_direction.sh` (by hand or pre-push; CI only if one exists) | One-way dependency graph                                                                                                            | M22                                                   |
| `apps/mobile/test/features/broadcast/ui_copy_contract_test.dart`              | Copy and limits match backend constants                                                                                             | M26, L04                                              |
| `apps/mobile/test/features/settings/settings_page_test.dart`                  | The language setting is honest                                                                                                      | L06                                                   |
| `apps/api/tests/ws_protocol.rs`                                               | Server frame shape and close codes                                                                                                  | C04                                                   |
| `docs/release-checklist.md` (one file)                                        | Device evidence for the secure store, upgrade (A13), realtime, push taps, background audio, ObjectBox upgrade and the release build | C02, C03, C04, H07, H24, M05, M06, M16, M17, M18, M25 |

**Validation sequence for every phase exit:** focused unit suites → widget/lifecycle suites → `flutter test` (full mobile) → `dart analyze` (must stay clean) → `cargo test` for backend steps → `docs/release-checklist.md` for D. Do not loosen lint rules or substitute the silent transport tests for real protocol validation.

---

## 18. What we are deliberately NOT doing

Consolidated, mapped to audit §4 (Over-Engineering Ledger) and the design's non-goals. Each line names what is rejected and why.

| Not doing                                                                                                                                     | Why (audit §4 / design)                                                                                                                          |
| --------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| A `live:<id>` DI scope in any form                                                                                                            | Registration existence became a state protocol; replaced by one manager with a nullable session.                                                 |
| A private per-session `GetIt` container                                                                                                       | The audit's comparison rejects it: too few collaborators to justify the lifecycle.                                                               |
| Any third application DI scope (startup "throwable", page, notification, chat, notes, discover)                                               | Design invariant I5: root plus zero-or-one authenticated only. Retry rebuilds root instead.                                                      |
| An interface per new service                                                                                                                  | Only `RoomTransport` and `TokenBlocklist` have real second implementations. Fakes are concrete.                                                  |
| An event bus or generic session framework                                                                                                     | §1.4 "Generalization" says wait until behaviour is genuinely shared.                                                                             |
| Nested command pipelines for internal sequencing                                                                                              | §4 ledger: commands are UI adapters. Internal lifecycle uses plain async methods.                                                                |
| Command-driven disposal (`dispose` calling a command)                                                                                         | M21/H03: disposal must not depend on UI machinery.                                                                                               |
| Numbered-page pagination in the feed base                                                                                                     | I01: nothing uses it; cursor only.                                                                                                               |
| A shared empty-identity profile proxy                                                                                                         | §4 ledger: an ID-bearing loading placeholder is simpler and correct.                                                                             |
| Global registration of entity proxies                                                                                                         | M04: page-owned objects stay page-owned.                                                                                                         |
| Re-registering credentials as a mutable singleton to "refresh" the socket                                                                     | M03: a credential supplier plus a serialized handoff instead.                                                                                    |
| Automatic retry of refresh, ambiguous mutations, or consumed streams                                                                          | H09/M15/H20: uncertainty is surfaced, not retried.                                                                                               |
| Backend recovery of an uncertain rotation via token reuse or a stored-outcome replay window                                                   | Design §5.5 / assumption A5: reauthentication by default; a recovery endpoint is a separate contract. Step 66 returns distinct error codes only. |
| Keeping legacy credentials "because they look valid"                                                                                          | C03/H10: no logout tombstone exists, so nothing old is usable.                                                                                   |
| Persisting LiveKit tokens or notification payloads                                                                                            | M18 / design §5.9: minimal non-secret intent, in-memory destinations.                                                                            |
| Navigating from platform callbacks                                                                                                            | M16/M19: one validated pending destination, consumed after readiness.                                                                            |
| A fourth attempt at Socket.IO compatibility                                                                                                   | C04: the backend speaks raw WebSocket JSON.                                                                                                      |
| Emulating Socket.IO acknowledgements or inventing a replay service                                                                            | C04: neither exists server-side; missing replay is a documented limitation.                                                                      |
| Adding `creatorName` to the end response to satisfy the old client                                                                            | M12: display identity has exactly one source (the profile).                                                                                      |
| Rotating or upgrading ObjectBox off the preview in this work                                                                                  | I02: risk is documented and gated, not silently changed.                                                                                         |
| Adding an external crash/analytics SDK                                                                                                        | M23/design §5.13: sanitised platform logging until a retention decision exists.                                                                  |
| A localization framework adoption                                                                                                             | L06: deferred on purpose (a conscious deviation, Step 87); honest gate plus three README lines; the guide does not add ARB machinery.            |
| Rewriting the strict analyzer baseline                                                                                                        | Audit §5 item 10: no reason to weaken it.                                                                                                        |
| Rewriting `RoomTransport`, constructor injection, one ObjectBox handle, typed errors, or widget-owned feeds                                   | Audit §5: these are the patterns to keep.                                                                                                        |
| `Media3` or a replacement media stack                                                                                                         | M17's reference supports ownership, not a library swap.                                                                                          |
| Cross-client settings linearizability                                                                                                         | M13: field-level PATCH only; no cross-client versioning; the limitation is disclosed.                                                            |
| A persisted generation, an interrupted-transition marker or migration state in the vault                                                      | OE-1: a generation only fences late work inside one process; nothing survives a kill.                                                            |
| Read-back verification of secure-storage writes, a lying-keystore test double, or a global storage gate                                       | OE-2: the queue already writes first and updates memory only after success.                                                                      |
| A migration framework, marker key or resume logic for legacy credentials                                                                      | OE-3: a ~15-line startup deletion (or nothing, if no build with real users exists).                                                              |
| An unmount-acknowledgement hub                                                                                                                | OE-4: Flutter's frame is the signal.                                                                                                             |
| A bootstrap cleanup ledger with attempt ids                                                                                                   | OE-5: reset `get_it` with dispose and rebuild.                                                                                                   |
| A notification-tap queue, expiry, dedupe list or account-chooser dialog                                                                       | OE-6: one pending slot; a wrong-account tap is dropped with a snackbar.                                                                          |
| An exit-channel capability, a temporary live-exit adapter, or three layers of idempotent exit                                                 | OE-7: pass the captured token; memoise only in `ActiveBroadcastManager`.                                                                         |
| A second attempt-ticket counter, `UserManager` bind/unbind, or a `StreamManager` pass-through                                                 | OE-8, OE-9, OE-17.                                                                                                                               |
| A generic `AccountLocalData` layer, or a split public/viewer-relative profile cache                                                           | OE-12, OE-13.                                                                                                                                    |
| A settings version column, `baseVersion`, 409 or overlay machinery                                                                            | OE-14: field-level PATCH.                                                                                                                        |
| Restarting the socket on every token refresh, or a fan-out roster event to every listener                                                     | OE-15, OE-16.                                                                                                                                    |
| A feature-flag system, `integration_test` files for storage/upgrade/ObjectBox, or a separate localization-plan and feature-readiness document | OE-19: hide a broken button; one `release-checklist.md`; README lines.                                                                           |
| Building a CI pipeline for this work                                                                                                          | OE-23 / A14: nothing in the audit requires one.                                                                                                  |
| Store listing, marketing, accessibility certification                                                                                         | Out of scope for an implementation guide; accessibility needs device semantics/contrast/large-text testing.                                      |

---

## 19. Final Definition of Done

Each line is an acceptance check, not a claim that it is done. Tick only with a test name, a device record, or a reviewed diff behind it.

- [ ] **Traceability intact.** Every ID in the traceability table maps to a step, and every step that claims an ID has a test or device record. Backend prerequisites (C05, H17, H18, H21, H22, H23, and the backend halves of H05, H09, H24, M13) are marked done only with `cargo test` evidence. _(C01–I02)_
- [ ] **Identity invariant.** No request, retry or replay initiated as A can execute as B, verified with held completers and a switch in between. _(C01, M15)_
- [ ] **Vault invariant.** One serialized queue, one versioned envelope, conditional writes, absolute expiry, fail-closed decoding. _(C02, H10)_
- [ ] **Logout invariant.** Durable local removal regardless of server outcome; a failed removal shows one "could not sign out, retry" state and local resources still close; another account is never activated. _(C03)_
- [ ] **Rotation invariant.** Rotation outcomes are classified (four cases); a possibly-consumed token is never retried by the app. _(H09)_
- [ ] **Two tiers only.** Exactly `root` and zero-or-one `authenticated`; no live/page/startup scope, no session readiness signalling. _(H02, M21)_
- [ ] **Readiness invariant.** The first authenticated build waits for local readiness; the old subtree is gone (one frame) before pop; watches and feeds remount by generation. _(H01, H15)_
- [ ] **Live ownership invariant.** One account-owned manager, one nullable session, one idempotent close, capabilities separated from phase, end summary retained outside resources. _(H02–H04, H08, M21)_
- [ ] **Connection invariant.** Handlers installed before connect, credentials supplied (never captured), start after verified readiness, stop awaited before disposal. _(C04, H06, H07, M03)_
- [ ] **Realtime verified (or visibly unavailable).** Raw-WebSocket framing, heartbeat, bounded reconnect and terminal reasons proven against the real backend. _(C04, H07)_
- [ ] **Local data invariant.** Every private read/write/delete is owner-qualified through the owner-bound local services; keys are unique; queries always close. _(M05, M06)_
- [ ] **Cache invariant.** Viewer-relative state is keyed by viewer and subject; legacy viewer-less caches are gone. _(H14)_
- [ ] **Profile invariant.** Stable repository identity with working reference counts; the editor compares like with like; one current-user presentation source. _(H13, M11, M12, M04)_
- [ ] **Settings invariant.** Serialized edits; only changed fields are sent; unknown preference members survive end to end. _(M13)_
- [ ] **Feed invariant.** Query generations, refresh/page arbitration, filtered-page progress, separate page errors, safe rebinding, cursor-only base. _(H11, H12, M08, M09, I01)_
- [ ] **Resend invariant.** Only a tap sends a code; availability is derived; one error surface. _(M01, L02)_
- [ ] **Validation parity.** One fixture set proves client and server accept and reject the same inputs; server-valid values persist without throwing. _(H25)_
- [ ] **Notification boundary (remediation).** Root payload fix and platform adapter only; one pending destination consumed after matching readiness; honest unavailable state. The coordinator, inbox and token association are the **Feature track** checklist (Steps 70, 71, 73). _(M16, M19; H24 root part)_
- [ ] **Release gate.** Signed release build, notification resources, permission audit, and `docs/release-checklist.md` passes. _(M25, M17)_
- [ ] **Schema gate.** ObjectBox UIDs unchanged, one handle, preview risk documented (drop the dedupe check if A13 finds no installed builds). _(M06, I02)_
- [ ] **Reference quality.** No commented-out suites, new and repaired tests use Mocktail, the import-direction script passes, no enabled no-ops, honest language setting (deferred on purpose), README matches the code. _(M20, M22, M24, M26, L01, L03, L04, L05, L06)_
- [ ] **Logging.** Production warnings/errors reach a sink with allowlisted fields; sentinel-secret tests pass at every level. _(M23)_
- [ ] **Nothing from audit §5 was rewritten.** Constructor injection, `RoomTransport`, widget-owned feeds, separated refresh/pagination with length-3 prefetch, success-observation, optimistic overrides, media cancellation, typed errors, strict analyzer. _(keep-list)_

---

## Coverage Checklist

All 64 audit findings, each ticked only when its step's "Done when" and tests pass. Count first, then start.

**CRITICAL (5)**

- [ ] C01 — Requests can cross account boundaries during replay — Steps 1, 8, 9, 13
- [ ] C02 — Session-store mutations are neither atomic nor identity-conditional — Steps 2, 3, 4, 5, 10, 23, 74
- [ ] C03 — Explicit logout can leave a usable session recoverable — Steps 2, 3, 5, 7, 23, 74, 75
- [ ] C04 — Mobile and backend use incompatible realtime protocols — Steps 47, 78
- [ ] C05 — Backend access-token revocation is not enforced by the actual HTTP guard — Step 65

**HIGH (25)**

- [ ] H01 — Authenticated UI is not synchronized with account-scope readiness — Steps 1, 18, 19, 20, 21
- [ ] H02 — Live transitions can outlive account teardown or supersede newer sessions — Steps 15, 18, 30, 31
- [ ] H03 — Session disposal can access an uninitialized timer — Step 29
- [ ] H04 — Failed end/leave skips session closure; ended UI depends on disposed resources — Steps 15, 29, 32, 33
- [ ] H05 — Broadcast end and cohost-removal contracts are wrong — Steps 51, 52
- [ ] H06 — No production path starts realtime delivery — Steps 44, 48
- [ ] H07 — Reconnection has stale credentials, conflicting state, and ignored terminal reasons — Steps 49, 78
- [ ] H08 — Publishing permission is mistaken for creator authority — Step 28
- [ ] H09 — Refresh failures destroy sessions, including ambiguous rotated-token outcomes — Steps 10, 66
- [ ] H10 — Stored access-token expiry moves forward on every read — Steps 2, 3, 4, 5, 6
- [ ] H11 — Refresh, pagination, query changes, and invalidations race — Step 60
- [ ] H12 — Client-filtered empty pages strand pagination — Step 61
- [ ] H13 — Cold profile proxies share an empty identity and cannot be released correctly — Step 39
- [ ] H14 — Viewer-specific profile cache state leaks across accounts — Steps 16, 36, 38
- [ ] H15 — Async operations continue after their owner is disposed — Steps 19, 26, 34, 43
- [ ] H16 — Participant UI does not implement the backend roster contract — Steps 53, 54
- [ ] H17 — Backend cohost removal does not reliably revoke role or presence — Step 55
- [ ] H18 — Backend broadcast transitions are not safely retryable — Step 56
- [ ] H19 — Editor progress and retries can report success without completing entry — Step 57
- [ ] H20 — Broadcast mutations do not carry stable logical-operation idempotency keys — Step 58
- [ ] H21 — Password-reset requests revoke sessions before proving ownership — Step 67
- [ ] H22 — Notes sync can lose folders created in the same batch — Step 68
- [ ] H23 — Chat deletion can mutate one room and publish to another — Step 69
- [ ] H24 — Push and durable notifications are not implemented end-to-end — Steps 45, 79; Feature track: 70, 71
- [ ] H25 — Client validation rejects server-valid accounts and can break credential storage — Steps 6, 12

**MEDIUM (26)**

- [ ] M01 — Email resend availability drives an automatic resend loop — Step 25
- [ ] M02 — Add-account flow retains password-only login mode — Steps 7, 23
- [ ] M03 — Scoped credentials and socket tokens stay stale — Steps 44, 49
- [ ] M04 — Public profile page resolves an unregistered proxy — Step 40
- [ ] M05 — Local storage isolation is optional rather than enforced — Steps 16, 36, 75
- [ ] M06 — ObjectBox writes can duplicate logical rows; queries lack deterministic cleanup — Steps 36, 37, 75, 80
- [ ] M07 — Favourites are disconnected snapshots — Step 59
- [ ] M08 — Pagination errors are absent from the feed error surface — Step 62
- [ ] M09 — Socket rebinding cancels the wrong handles; feed events ignore filters — Step 63
- [ ] M10 — Invalidation parsing and event enrichment are inconsistent — Step 50
- [ ] M11 — Profile editor compares different types and ignores bio validity — Step 41
- [ ] M12 — Current-user presentation has multiple unsynchronized sources — Step 24
- [ ] M13 — Settings writes race and overwrite unknown preference keys — Steps 42, 72
- [ ] M14 — Account-scoped managers redundantly track global account selection — Steps 18, 24, 26
- [ ] M15 — HTTP failure paths can hang, misclassify, or leak credentials to external origins — Steps 8, 9, 11
- [ ] M16 — Push payloads, channels, and tap lifecycle disagree — Steps 22, 45, 79
- [ ] M17 — Background-audio support is declared but not established — Step 76
- [ ] M18 — Production can select a successful silent transport; recovery stores a bearer token — Steps 3, 14, 16, 75
- [ ] M19 — Routing discards identity and intended destinations — Steps 21, 22, 32
- [ ] M20 — Named navigation is used for a route without that name — Step 81
- [ ] M21 — Scope/connection orchestration is over-commanded — Steps 30, 31, 32, 34, 35
- [ ] M22 — Shared infrastructure imports features and broad barrels hide the dependency graph — Steps 17, 82
- [ ] M23 — Production logging suppresses every severity — Step 13
- [ ] M24 — Critical lifecycle tests are commented out — Steps 0, 20, 74, 83
- [ ] M25 — Build/platform configuration is not release-ready — Step 77
- [ ] M26 — UI presents incomplete behavior as operational — Step 84

**LOW (6)**

- [ ] L01 — Rebuild granularity is too broad for a one-second clock — Step 85
- [ ] L02 — Derived observables are recreated by getters — Steps 23, 24, 25
- [ ] L03 — Timer semantics and retry timing are inaccurate — Steps 11, 29, 85
- [ ] L04 — Small UI/validation mismatches remain — Step 84
- [ ] L05 — Dead paths, fixtures, and documentation obscure the reference implementation — Steps 35, 86
- [ ] L06 — Language setting is not connected to localization — Step 87

**INFO (2)**

- [ ] I01 — The feed base mixes unused page arithmetic with cursor pagination — Step 64
- [ ] I02 — Dependency risk is explicit, not proven obsolescence — Steps 0, 46, 80, 83

**Total: 5 + 25 + 26 + 6 + 2 = 64 findings, 64 checkboxes. 64/64 covered by at least one step.**

---

## Appendix A — Step index

Fast lookup. Phase, title, dependencies. Step numbers are unchanged from the earlier draft; removed steps stay as stubs. Steps that ship together are listed in §5.5 (Delivery groups).

| Step | Phase         | Title                                                                                   | Depends on     |
| ---- | ------------- | --------------------------------------------------------------------------------------- | -------------- |
| 0    | P0            | Tooling and safety net                                                                  | —              |
| 1    | P1            | Add the account identity value types                                                    | 0              |
| 2    | P1            | Define the vault storage shapes with fail-closed decoding                               | 1              |
| 3    | P1            | Reduce `SecureStorage` to single-key operations and add the v2 key                      | 0              |
| 4    | P1            | Rewrite `SessionStore` as one serialized, conditional vault                             | 2, 3           |
| 5    | P1            | Delete legacy auth data once at startup (decision-gated, A13)                           | 4              |
| 6    | P1            | Convert relative lifetimes to absolute expiry once, at receipt                          | 2, 4           |
| 7    | P1            | Make logout durable and separate it from deactivate                                     | 4              |
| 8    | P1            | Bind every request to the identity that started it                                      | 4, 6           |
| 9    | P1            | Complete every interceptor path exactly once                                            | 8              |
| 10   | P1            | Classify refresh outcomes and never retry a possibly-consumed token                     | 4, 9           |
| 11   | P1            | Fix retry timing and reuse the captured identity                                        | 8, 9           |
| 12   | P1            | Align client validation with the server and stop validating during serialization        | 2, 6           |
| 13   | P1            | Report production failures safely and stop the blanket sign-out                         | 1, 8           |
| 14   | P1            | Make production configuration fail loudly instead of silently                           | 13             |
| 15   | P1            | Fence the old live tier so it cannot outlive a transition (skip if P1–P3 ship together) | 7, 8           |
| 16   | P1            | Stop writing secrets to preferences and enforce owner-qualified local access            | 5 (or 4)       |
| 17   | P1            | Move the session model out of `_shared` and fix the barrels                             | 4, 8           |
| 18   | P2            | Replace `UserScopeManager` with one transition owner                                    | 4, 7, 15       |
| 19   | P2            | Gate the account subtree by generation                                                  | 18             |
| 20   | P2            | Make startup retry actually rebuild what failed                                         | 14, 18         |
| 21   | P2            | Refresh the router on identity and preserve the intended destination                    | 18, 19         |
| 22   | P2            | Keep one pending notification/deep-link destination                                     | 21             |
| 23   | P2            | Reduce `AuthManager` to a UI facade with attempt tickets                                | 18, 7          |
| 24   | P2            | Give the current user exactly one presentation source                                   | 23, 18         |
| 25   | P2            | Send verification codes only when the user taps                                         | 23             |
| 26   | P2            | Inject immutable account context into account-scoped managers                           | 18, 24         |
| 27   | —             | (Removed) Adapt the live tier to the exit contract                                      | —              |
| 28   | P3            | Replace the live state booleans with one phase and separate capabilities                | 26             |
| 29   | P3            | Create the session's resources with the session and close them once                     | 28             |
| 30   | P3            | Build `ActiveBroadcastManager` as the single live owner                                 | 29             |
| 31   | P3            | Register only `ActiveBroadcastManager` in the account scope                             | 30             |
| 32   | P3            | Port the live pages to the manager's snapshot                                           | 31             |
| 33   | P3            | Move the banner into authenticated chrome and load the ended page by id                 | 32             |
| 34   | P3            | Point the editor at the manager, delete `StreamManager`, make the editor widget-owned   | 31             |
| 35   | P3            | Delete the live scope, its readiness signalling and the duplicate paths                 | 32, 33, 34     |
| 36   | P4            | Make the local services owner-bound                                                     | 18, 26         |
| 37   | P4            | Make ObjectBox writes transactional with unique logical keys                            | 36             |
| 38   | P4            | Key the profile cache by viewer and subject                                             | 36, 26         |
| 39   | P4            | Give the profile repository a stable subject identity                                   | 38, 36         |
| 40   | P4            | Target the page's own proxy in the public profile page                                  | 39             |
| 41   | P4            | Compare and validate the profile editor correctly                                       | 12             |
| 42   | P4            | Send settings as field-level patches through one serial queue                           | 36, 26         |
| 43   | P4            | Fence profile and settings continuations against owner death                            | 39, 41, 42     |
| 44   | P4            | Add the account connection owner                                                        | 31, 10         |
| 45   | P4            | Fix the push payload and platform adapters at the root; render only in the foreground   | 22             |
| 46   | P4            | Record the ObjectBox preview risk and gate native schema changes                        | 37             |
| 47   | R             | Replace the Socket.IO client with a raw-WebSocket client                                | P1 only (4, 8) |
| 48   | R             | Make adapter install/detach explicit and idempotent                                     | 47             |
| 49   | R             | Reconcile reconnection into one phase and honour terminal reasons                       | 30, 44, 47     |
| 50   | R             | Decode event payloads strictly and keep "ended" distinct from a summary                 | 48             |
| 51   | B             | Fix the end and cohost-removal contracts on the client                                  | 29             |
| 52   | B             | Make the backend end/cohost contract explicit and fixture-tested                        | —              |
| 53   | B             | Define the roster audience and a listener refresh policy                                | —              |
| 54   | B             | Page and reconcile the roster on the client                                             | 30, 53         |
| 55   | B             | Revoke cohost role, presence and media permission atomically                            | 53             |
| 56   | B             | Make broadcast start/join retry-safe                                                    | 55             |
| 57   | B             | Make the editor create → upload → start → entry one operation                           | 30, 34, 56     |
| 58   | B             | Give each logical mutation a stable idempotency key (mobile)                            | 57             |
| 59   | B             | Back favourites with one account-level reactive source                                  | 37             |
| 60   | F             | Capture immutable query generations and arbitrate page versus refresh                   | P1 only        |
| 61   | F             | Keep paging through client-filtered pages (skipped if the backend has an ended filter)  | 60             |
| 62   | F             | Give page-load failures their own error surface                                         | 60             |
| 63   | F             | Rebind subscriptions safely and respect query predicates on events                      | 50, 60         |
| 64   | F             | Reduce the feed base to the cursor contract that actually exists                        | 60–63          |
| 65   | U             | Enforce account-wide revocation in the HTTP guard                                       | —              |
| 66   | U             | Make refresh rotation errors actionable and non-consuming                               | —              |
| 67   | U             | Revoke sessions only after reset ownership is proven                                    | 65             |
| 68   | U             | Resolve note folder ownership after folder mutations                                    | —              |
| 69   | U             | Guard chat deletion by room as well as sender                                           | —              |
| 70   | Feature track | Give notifications a durable inbox and a device-token policy                            | 65             |
| 71   | Feature track | Make the client honest about notifications (creates `NotificationCoordinator`)          | 45, 70         |
| 72   | U             | Make settings a field-level PATCH so old clients cannot delete new keys                 | 42             |
| 73   | Feature track | Point the feature gates at the audit's readiness section                                | —              |
| 74   | D             | Write the release checklist and prove the secure store on a device                      | 3, 4, 7        |
| 75   | D             | Validate the upgrade path (checklist row; A13 only)                                     | 5, 16, 74      |
| 76   | D             | Define and test the background-audio story                                              | 31             |
| 77   | D             | Make the release build releasable                                                       | 76             |
| 78   | D             | Validate the realtime protocol against the real backend                                 | 47, 49, 77     |
| 79   | D             | Validate push taps (checklist row)                                                      | 22, 45, 77     |
| 80   | D             | Validate the ObjectBox preview schema upgrade (one checklist line; A13 only)            | 37, 46, 74     |
| 81   | C             | Navigate by routes that exist                                                           | 21             |
| 82   | C             | Enforce the import direction and narrow the barrels                                     | 60, 32, 33, 36 |
| 83   | C             | Confirm no lifecycle test is still commented out; fix the mocking convention            | 18, 26, 30     |
| 84   | C             | Stop presenting unimplemented behaviour as working                                      | 54, 41         |
| 85   | C             | Extract the elapsed label so the clock does not rebuild the tab                         | 29             |
| 86   | C             | Remove dead paths and make the documentation true                                       | 35, 82         |
| 87   | C             | State the localization limitation and stop growing it                                   | 42             |

---

_End of guide. Source of truth for what changes: [docs/audit-report.md](audit-report.md). Reference design for the account/DI slice: [docs/account-di-migration.md](account-di-migration.md)._
