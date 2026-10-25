# Audit Report — Implementation Guide

**Purpose.** This guide turns [docs/audit-report.md](audit-report.md) into an ordered set of small, buildable steps for a single developer. [docs/account-di-migration.md](account-di-migration.md) is a reference design, not an authority: where the two disagree, the audit wins and the simpler option is taken.

**How to use it.** Work top to bottom. Each step names its audit source, the exact files, the shape of the code, and how to prove it worked. A step may only start when its `Depends on` steps are merged. Steps that must land in one change say so in their own text.

**Scope.** All 64 findings (C01–I02), the audit's §1 scope redesign (`ActiveBroadcastManager`), §4 over-engineering ledger, §5 keep-list, §6 feature readiness, §7 reference architecture and §8 priorities — mobile Dart (`apps/mobile`) and backend Rust (`apps/api`).

**Revision.** This is revision 2, aligned with the review in `Implementation_Guide_Review_Alignment__Drift_and_Over-Engineering.md`. 88 steps became 61: build-breaking groups are merged into single steps, two steps were deleted as unnecessary for an app with no shipped build, and roughly twenty-five mechanisms the audit never asked for were removed while keeping the guarantee each one carried. Every removal is recorded as `Simplified …` in its step.

**Status.** Instructions, not a claim of work. Nothing here has been compiled and no device test has run. Every `Done when` is a plan until a test or a device record exists.

---

## 1. Assumptions and open questions

Each item has a default already baked into the steps. Two were resolved by reading the repository rather than guessing; both are noted.

| # | Assumption | Default taken | Where it bites |
| - | ---------- | ------------- | -------------- |
| A1 | Audit paths are written as `meno/apps/mobile/...`; this repo is a monorepo whose app lives at `apps/mobile`. All paths here are repo-relative. | Repo-relative paths. | Every step |
| A2 | `freezed ^4.0.2`, `freezed_annotation ^3.1.0`, `build_runner ^2.15.1` are already dependencies (`pubspec.yaml:29,53,57`). | Use what is installed; no upgrades (I02). | Step 0 |
| A3 | Dart `3.13.5`, Flutter `3.47.6` (verified locally). Existing classes use the `new(...)` declaring-constructor shorthand; freezed does not generate it. | New freezed classes use `const factory(...)`; hand-written classes keep `new(...)`. | Step 0 |
| A4 | "One developer" means no interface per class, no event bus, no generic lifecycle framework. Fakes are concrete classes passed through constructors. | Concrete collaborators everywhere. | All steps |
| A5 | Sessions use single-use refresh tokens (audit §3 H09). | An uncertain refresh means re-authenticate; never auto-retry. | Steps 5, 12 |
| A6 | The app may ship with realtime visibly unavailable (design §5.6). | Realtime enablement is gated behind Steps 24 and 52. | Step 24 |
| A7 | Durable notifications need a backend inbox (audit H24), so the whole feature sits in the Feature track (Steps 58–60), not in the remediation. | The app shows "notifications unavailable" until then. | Steps 58–60 |
| A8 | **Corrected in this revision.** Generated files are *not* committed: `.gitignore:77–79` ignores `*.g.dart` and `*.freezed.dart`. The files exist only after a local build. | Regenerate locally; never add generated files to a commit. | Step 0 |
| A9 | **Decided from the repository.** No build has shipped: release signs with the debug key (`android/app/build.gradle.kts:38–41`) and there is no `google-services.json` or `GoogleService-Info.plist`. | No legacy-credential migration and no upgrade-with-installed-build step. If a build *is* distributed later, add the one-time legacy-key deletion described in Step 2's note before the first release. | Steps 2, 38, 53 |
| A10 | Localization stays out of scope (audit L06), and that deviation is stated in Step 57 rather than hidden. | No l10n framework; each screen keeps its strings in one place. | Step 57 |
| A11 | Two mocking libraries exist (Mockito, Mocktail). New and repaired tests use Mocktail; existing Mockito tests are not converted. | Mocktail for anything touched here. | Step 55 |
| A12 | The `live:<id>` DI tier is deleted, not refactored (audit §1.4). | Rebuilt inside one account-owned manager. | Steps 25–27 |
| A13 | **Decided from the repository.** There is no CI pipeline (`.github/workflows` and `tool/` do not exist). | Checks run by hand or from a git pre-push hook; no pipeline is built. | Steps 54, 55 |
| A14 | The audit outranks the design document. Where the design is heavier than one developer needs, the step says so and keeps the safety requirement. | Simplification with a stated reason. | All steps |
| A15 | ObjectBox is `6.0.0-preview.4` and its `ConflictStrategy.replace` support could not be verified from this shell. | Step 38 uses the unique-key upsert *if* that API exists, otherwise one lookup plus `put` inside a single write transaction. | Step 38 |

**Open questions that do not block.** Exact approved API origin list (Step 4 assumes the one origin in `Config`); whether biometric gating of the vault is required (assumed no); whether notification taps survive process death (assumed no, Step 19); whether the backend can filter ended broadcasts (Step 36 checks first). Ask the developer only if a product decision changes.

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
│ OnboardingManager · ThemeManager · GoRouter + one pending destination            │
└───────────────────────────────────────────────────────────────────────────────────┘
                    │ push exactly one `authenticated` scope when ready
                    ▼
┌──────────────── authenticated scope (one account, one generation) ───────────────┐
│ AccountContext (immutable id + generation)                                        │
│ Account-bound HttpClient facade + SessionInterceptor (identity-pinned)            │
│ ProfileCacheService · SettingsLocalService · BroadcastLocalService                │
│      (each takes ownerId in its constructor; none exposes a Box)                  │
│ BroadcastHttpService · ProfileHttpService · SettingsHttpService                   │
│ UserProfileRepository (stable subject identity, refcounted proxies)               │
│ MyProfileManager · SettingsManager (initialized once, no global selectors)        │
│ SocketClient + BroadcastSocketService + AccountConnectionOwner                    │
│ LiveSessionStore (non-secret recovery intent only)                                │
│ ActiveBroadcastManager ── owns at most one ActiveBroadcastSession                 │
│      ActiveBroadcastSession = RoomTransport + roster + media subscriptions        │
│      (the roster and the timer live here; there is no separate StreamManager)     │
│ FavouritesManager                                                                 │
└───────────────────────────────────────────────────────────────────────────────────┘
                    │ widget lifetime (never in DI)
                    ▼
┌──────────────── widget-owned objects ─────────────────────────────────────────────┐
│ BroadcastEditorManager · ProfileEditorManager · UserProfileManager (details)      │
│ EmailVerificationManager · feed data sources via createOnce · form controllers    │
└───────────────────────────────────────────────────────────────────────────────────┘
```

An arrow means "is created by". The `authenticated` scope is a **GetIt scope**: a named layer of registrations pushed when an account becomes usable and popped (disposed) when it leaves. Nothing below root is ever pushed by a page. There is no third tier, no per-session container and no live scope.

---

## 3. Glossary

| Term | One-line meaning |
| ---- | ---------------- |
| **GetIt scope** | A named layer of registrations stacked on the root ones. Lookup searches top-down; popping the layer disposes everything registered in it. |
| **`registerLazySingleton`** | Registers a factory that runs on first lookup and is reused afterwards. Nothing is built until someone asks. |
| **`registerSingleton`** | Registers an already-built object; disposal goes through its `dispose:` callback. |
| **freezed** | Code generator producing `copyWith`, `==`, `hashCode` for immutable classes. This guide uses it only where `copyWith` earns its place. |
| **sealed class** | A class whose subtypes are all known, so `switch` is exhaustive and the compiler reports a missing case. Hand-written in Dart 3. |
| **enum** | A closed set of values with no payload. Right answer for phases and steps. |
| **`ValueNotifier` / `ValueListenable`** | A box holding a value that notifies listeners when it changes; `ValueListenable` is the read-only view. |
| **FIFO queue** | Work runs one job at a time, in arrival order, and one failure does not drop the queue. |
| **compare-and-set (CAS)** | "Write only if what I expect is still there", so a late refresh cannot overwrite a newer session. |
| **idempotent** | Calling it twice equals calling it once. Every close, dispose and revoke path here is idempotent. |
| **generation** | An in-memory counter, incremented each time an account is activated. Keys the account subtree and fences late async work. It is not persisted: nothing survives a process kill, so persisting it would protect nothing. |
| **interceptor** | A hook around every HTTP request (attach token, react to 401). |
| **single-flight** | At most one in-flight operation per key; concurrent callers share the result. |
| **proxy** | A wrapper around a DTO adding commands, overrides and change notification. |
| **`createOnce`** | A `watch_it` helper that builds a widget-local object on first build and disposes it on unmount. |
| **`watchValue`** | A `watch_it` function that reads a `ValueListenable` off a DI object and rebuilds that widget when it changes. |
| **restriction (command_it)** | A `ValueListenable<bool>` where `true` means "cannot run". Not a concurrency lock. |
| **owner-fenced** | After every `await`, the code re-checks that its account, generation and owner are still live before the next side effect. |
| **transport** | The layer that physically sends an HTTP/WebSocket request. It never chooses the identity. |

---

## 4. Traceability table

One row per finding. **Step(s)** lists every step whose audit reference cites it; the first is where it is finished. Status says who does the work.

| ID | Title | Severity | Step(s) | Status |
| -- | ----- | -------- | ------- | ------ |
| C01 | Requests can cross account boundaries during replay | CRITICAL | 1, 4, 7 | Mobile |
| C02 | Session-store mutations are neither atomic nor identity-conditional | CRITICAL | 2, 5, 20, 53 | Mobile |
| C03 | Explicit logout can leave a usable session recoverable | CRITICAL | 2, 3, 20, 53 | Mobile |
| C04 | Mobile and backend use incompatible realtime protocols | CRITICAL | 24, 52 | Mobile |
| C05 | Backend access-token revocation is not enforced by the actual HTTP guard | CRITICAL | 11 | Backend-Rust |
| H01 | Authenticated UI is not synchronized with account-scope readiness | HIGH | 1, 16, 17, 18, 19 | Mobile |
| H02 | Live transitions can outlive account teardown or supersede newer sessions | HIGH | 15, 16, 26 | Mobile |
| H03 | Session disposal can access an uninitialized timer | HIGH | 25 | Mobile |
| H04 | Failed end/leave skips session closure; ended UI depends on disposed resources | HIGH | 15, 25, 27 | Mobile |
| H05 | Broadcast end and cohost-removal contracts are wrong | HIGH | 28, 29 | Both |
| H06 | No production path starts realtime delivery | HIGH | 24, 48 | Mobile |
| H07 | Reconnection has stale credentials, conflicting state, and ignored terminal reasons | HIGH | 50, 52 | Mobile |
| H08 | Publishing permission is mistaken for creator authority | HIGH | 25 | Mobile |
| H09 | Refresh failures destroy sessions, including ambiguous rotated-token outcomes | HIGH | 5, 12 | Both |
| H10 | Stored access-token expiry moves forward on every read | HIGH | 2 | Mobile |
| H11 | Refresh, pagination, query changes, and invalidations race | HIGH | 35 | Mobile |
| H12 | Client-filtered empty pages strand pagination | HIGH | 36 | Mobile |
| H13 | Cold profile proxies share an empty identity and cannot be released correctly | HIGH | 40 | Mobile |
| H14 | Viewer-specific profile cache state leaks across accounts | HIGH | 9, 39 | Mobile |
| H15 | Async operations continue after their owner is disposed | HIGH | 17, 23, 45 | Mobile |
| H16 | Participant UI does not implement the backend roster contract | HIGH | 30, 31 | Both |
| H17 | Backend cohost removal does not reliably revoke role or presence | HIGH | 32 | Backend-Rust |
| H18 | Backend broadcast transitions are not safely retryable | HIGH | 33 | Backend-Rust |
| H19 | Editor progress and retries can report success without completing entry | HIGH | 34 | Mobile |
| H20 | Broadcast mutations do not carry stable logical-operation idempotency keys | HIGH | 34 | Both |
| H21 | Password-reset requests revoke sessions before proving ownership | HIGH | 11 | Backend-Rust |
| H22 | Notes sync can lose folders created in the same batch | HIGH | 13 | Backend-Rust |
| H23 | Chat deletion can mutate one room and publish to another | HIGH | 14 | Backend-Rust |
| H24 | Push and durable notifications are not implemented end-to-end | HIGH | 49, 58, 59 | Both |
| H25 | Client validation rejects server-valid accounts and can break credential storage | HIGH | 2, 6 | Mobile |
| M01 | Email resend availability drives an automatic resend loop | MEDIUM | 22 | Mobile |
| M02 | Add-account flow retains password-only login mode | MEDIUM | 3, 20 | Mobile |
| M03 | Scoped credentials and socket tokens stay stale | MEDIUM | 48, 50 | Mobile |
| M04 | Public profile page resolves an unregistered proxy | MEDIUM | 41 | Mobile |
| M05 | Local storage isolation is optional rather than enforced | MEDIUM | 9, 38, 53 | Mobile |
| M06 | ObjectBox writes can duplicate logical rows; queries lack deterministic cleanup | MEDIUM | 38, 53 | Mobile |
| M07 | Favourites are disconnected snapshots | MEDIUM | 46 | Mobile |
| M08 | Pagination errors are absent from the feed error surface | MEDIUM | 35 | Mobile |
| M09 | Socket rebinding cancels the wrong handles; feed events ignore filters | MEDIUM | 37 | Mobile |
| M10 | Invalidation parsing and event enrichment are inconsistent | MEDIUM | 24 | Both |
| M11 | Profile editor compares different types and ignores bio validity | MEDIUM | 42 | Mobile |
| M12 | Current-user presentation has multiple unsynchronized sources | MEDIUM | 21 | Mobile |
| M13 | Settings writes race and overwrite unknown preference keys | MEDIUM | 43, 44 | Both |
| M14 | Account-scoped managers redundantly track global account selection | MEDIUM | 1, 16, 23 | Mobile |
| M15 | HTTP failure paths can hang, misclassify, or leak credentials to external origins | MEDIUM | 4 | Mobile |
| M16 | Push payloads, channels, and tap lifecycle disagree | MEDIUM | 19, 49, 53, 59 | Mobile |
| M17 | Background-audio support is declared but not established | MEDIUM | 51 | Mobile |
| M18 | Production can select a successful silent transport; recovery stores a bearer token | MEDIUM | 8, 9, 53 | Mobile |
| M19 | Routing discards identity and intended destinations | MEDIUM | 19, 27 | Mobile |
| M20 | Named navigation is used for a route without that name | MEDIUM | 54 | Mobile |
| M21 | Scope/connection orchestration is over-commanded | MEDIUM | 26, 27 | Mobile |
| M22 | Shared infrastructure imports features and broad barrels hide the dependency graph | MEDIUM | 10, 54 | Mobile |
| M23 | Production logging suppresses every severity | MEDIUM | 7 | Mobile |
| M24 | Critical lifecycle tests are commented out | MEDIUM | 0, 18, 53, 55 | Mobile |
| M25 | Build/platform configuration is not release-ready | MEDIUM | 51 | Mobile |
| M26 | UI presents incomplete behavior as operational | MEDIUM | 56 | Mobile |
| L01 | Rebuild granularity is too broad for a one-second clock | LOW | 25 | Mobile |
| L02 | Derived observables are recreated by getters | LOW | 16, 20, 21, 22 | Mobile |
| L03 | Timer semantics and retry timing are inaccurate | LOW | 4, 25 | Mobile |
| L04 | Small UI/validation mismatches remain | LOW | 56 | Mobile |
| L05 | Dead paths, fixtures, and documentation obscure the reference implementation | LOW | 27, 54 | Mobile |
| L06 | Language setting is not connected to localization | LOW | 57 | Mobile |
| I01 | The feed base mixes unused page arithmetic with cursor pagination | INFO | 35 | Mobile |
| I02 | Dependency risk is explicit, not proven obsolescence | INFO | 0, 47, 53, 55 | Mobile |

Count: 5 CRITICAL + 25 HIGH + 26 MEDIUM + 6 LOW + 2 INFO = **64 findings, all mapped.**

---

## 5. Rules of the road

### 5.1 Simplicity rules

1. **Concrete classes, constructor injection.** Collaborators arrive as constructor parameters. No `di<X>()` inside a method body outside `locator.dart`, the composition files and small view boundaries.
2. **No interface per class.** An interface exists only where a second implementation exists today: `RoomTransport` (LiveKit vs silent fake) and the backend `TokenBlocklist`. Everything else gets a concrete fake in `test/_support/`.
3. **No event bus, no generic framework, no new DI tier.** Commands are for user actions; plain `Future` methods sequence internal work.
4. **Prefer fewer files.** One file may hold two or three tightly related types. Never a file per field.
5. **Immutable data, mutable owners.** Models are `final`; managers own their `ValueNotifier`; nothing else writes it.
6. **One writer per fact.** Credentials: `SessionStore`. Account selection: `AccountTransitionOwner`. Live phase: `ActiveBroadcastManager`. Current profile: `MyProfileManager`. A second writer is a bug.
7. **Fence after every await.** Any `await` in account-scoped code is followed by a check that the generation and owner are still live before the next side effect.
8. **Dispose everything you create**, in reverse order, idempotently. `finally` closes resources; a server outcome never gates local cleanup.
9. **No `BuildContext` across an await.** Capture what you need first.
10. **Enums for payload-free sets** (`AccountPhase`, `LivePhase`, `BroadcastCreationStep`) and **hand-written sealed classes for outcomes with data** (`RefreshOutcome`, `TransitionResult`). No booleans standing in for three-way states.
11. **Keep the existing stack**: get_it, watch_it, command_it, listen_it, go_router, ObjectBox, LiveKit, fpdart. No dependency is added by this guide.
12. **Simplify openly.** Where a step is simpler than the design document, the step says `Simplified from design doc §X because …; safety preserved because …`.
13. **No mechanism without a finding.** If a class exists to defend a guarantee, its step names the finding. Machinery for a build nobody runs or a failure nobody hit is removed (OE-flags in the review).

### 5.2 freezed policy

| Use freezed? | What | Why (one line) |
| ------------ | ---- | -------------- |
| **Yes** | `CredentialRecord`, `LiveSnapshot`, `AccountSnapshot` | They need `copyWith` plus value equality, and equality is what makes `select` comparisons and generation checks trustworthy. |
| **Yes** (hand-written decode) | `CredentialRecord` | Storage must *fail closed*: a hand-written `tryDecode` checks every field and returns `null` on the first violation; generated `fromJson` is not used for secrets. |
| **No — enum** | `AccountPhase`, `LivePhase`, `BroadcastCreationStep`, `EndReason` | Closed sets with no payload. An enum is smaller and needs no code generation. |
| **No — sealed class** | `RefreshOutcome`, `TransitionResult` | Dart 3 sealed classes give exhaustive `switch` without a generator, and the cases carry data. |
| **No — plain `final` class** | `AccountContext`, `RequestIdentity`, `PendingDestination`, `RememberedIdentity`, `VaultEnvelope` | Three or four fields, no `copyWith` needed. freezed here is ceremony, and a `final` class cannot accidentally carry a mutable flag. |
| **No** | services, managers, notifiers, widgets, anything with a lifecycle | freezed generates value classes; it has no `dispose` and hides ownership. |

Combining freezed with fail-closed decoding (Step 2 does exactly this):

- The freezed class declares the shape and provides `copyWith` and equality.
- A hand-written `static CredentialRecord? tryDecode(Map<String, dynamic>?)` performs **every** check and constructs the freezed object only after all pass.
- Serialization is a hand-written `Map<String, dynamic> toJson()` on the same class.

### 5.3 Naming and folder conventions

| Kind | Where it goes | Example |
| ---- | ------------- | ------- |
| Immutable model / union | `lib/features/<feature>/model/` (or `lib/_routing/`) | `features/auth/model/account_context.dart` |
| Service (one external boundary) | `lib/features/<feature>/services/`; genuinely cross-feature services in `lib/_shared/services/` | `features/auth/services/session_store.dart` |
| Manager (semantic business logic) | `lib/features/<feature>/manager/` | `features/broadcast/manager/active_broadcast_manager.dart` |
| App composition and lifecycle owners | `lib/app/` | `app/account_transition_owner.dart` |
| Widget | `features/<feature>/pages/` (screen) or `widgets/` (part of a screen) | `features/profile/pages/user_profile_page.dart` |
| Reusable widget with no feature knowledge | `lib/_shared/widgets/` | `_shared/widgets/feed_widget.dart` |
| Test | mirror `lib` under `test/` | `test/features/auth/session_store_test.dart` |
| Test fake / clock | `test/_support/` | `test/_support/fake_secure_storage.dart` |
| Rust module | `apps/api/src/modules/<module>/`: wire types in `dto.rs`, SQL in `repository/pg.rs`, rules in `service.rs`, HTTP in `handlers.rs` | `apps/api/src/modules/broadcast/service.rs` |
| Rust test | `apps/api/tests/<area>_<layer>.rs` | `apps/api/tests/broadcast_service.rs` |

Class names say what they own (`AccountTransitionOwner`, not `AccountHelper`); no `Impl` suffix unless an interface exists; no `_v2` on classes — versions live in storage keys and schema fields.

### 5.4 Import direction

`lib/app` → `features` → `_shared` → `_core`. Never the reverse. `_shared` must not import `features` or `app` (M22). Feature-to-feature calls go through a small public file in that feature, never a broad barrel.

## 6. Phase P0 — Tooling and safety net

**Findings finished here:** none directly; it enables [M24] and [I02] coverage.

**What you will have when this phase is done:** the freezed/build_runner loop the repo already has, one fake secure store and one controllable clock, and the two conventions the rest of the guide relies on (enums vs sealed classes, and generated files staying out of git).

**How to know it's safe to ship:** `dart analyze` clean, `build_runner` idempotent, the fake store and fake clock tested. Nothing here reaches users.

**Order:** Step 0 only.

**What we are deliberately NOT doing in P0:** no dependency upgrades (audit I02 keeps versions a documented risk), no change to the strict analyzer baseline (audit §5 item 10), and no read-back secret verification or "lying keystore" double — that defends against a platform failure no finding describes (review OE-2).


### Step 0: Tooling and safety net

**Phase:** P0
**Audit reference:** none (supporting step), needed by every freezed step and by §3 [M24] "Critical lifecycle tests are commented out"; dependency baseline per [I02] "Dependency risk is explicit, not proven obsolescence", evidence `apps/mobile/pubspec.yaml:38–39,55–62`; analyzer baseline per audit §5 item 10, evidence `apps/mobile/analysis_options.yaml:76–111`.
**Why (plain words):** The guide adds small immutable types and sealed unions, and the account work is only provable with a fake secure store and a controllable clock. Neither the fakes nor the union conventions exist yet. This step makes everything after it testable without adding a dependency.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/pubspec.yaml` | Modify | config | Required | Comment block only, recording the freezed/build_runner pins. No version bumps (I02 treats versions as a documented risk, not an upgrade task). |
| `apps/mobile/test/_support/fake_clock.dart` | Create | test | Required | `FakeClock` with `nowUtc()`, `monotonic()`, `advance(Duration)`. |
| `apps/mobile/test/_support/fake_secure_storage.dart` | Create | test | Required | In-memory `SecureStorage` with injectable failures (`failOnWrite`, `failOnRead`, `writeLies`). |
| `apps/mobile/analysis_options.yaml` | Do not change | config | Required | The strict baseline stays (audit §5 item 10). |

**Pseudo-code**

```
// Conventions locked here, applied everywhere later.

Freezed usage
    USE installed freezed 4.x: @freezed on `abstract class X with _$X` (value types)
                              @freezed on `sealed class X with _$X`     (unions)
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
    write(key, value)  -> optional FAIL; optional "store a DIFFERENT value"  // proves confirmation logic works
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

**Tests:** `apps/mobile/test/_support/fake_secure_storage_test.dart` (round-trip; forced write failure; `writeLies` produces a value that differs from what was written). `apps/mobile/test/_support/fake_clock_test.dart` (advance moves `nowUtc` and `monotonic` equally).

**Watch out for:** threading `Clock` through every existing constructor at once. Add it where a step needs it; only Step 4 registers it in root composition.

**Depends on:** nothing.

---

## 7. Phase P1 — Safety foundation

**Findings finished here:** [C01], [C02], [C03], [H09], [H10], [H25], [M15], [M18] (mobile slice), [M23], [L03] (HTTP slice).

**What you will have when this phase is done:**
- One secure vault document (`auth.vault.v2`) written through a single serialized queue, with absolute expiries that never move on read.
- A logout that removes the exiting account's credentials locally even when the server is unreachable, and an explicit add-account mode.
- Every request pinned to the account and generation that started it, with a token only ever attached to the approved origin, and every interceptor path completing exactly once.
- A refresh classified into succeeded / rejected / unavailable / uncertain, never a silent sign-out.
- No bearer token outside the vault, and no viewer-blind profile cache.

**How to know it's safe to ship:** the P1 rows of §19 pass; `dart analyze` clean; a request started as account A cannot be released into B in the interceptor test; a failed credential write leaves memory unchanged; killing the app during logout leaves either "logged out" or one visible retry state — never a silently restorable session.

**Order:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10. Steps 1–2 are one chunk; 3 and 4 are one chunk (the interceptor is shared); 5 depends on 4.

**What we are deliberately NOT doing in P1:** no `AccountTransitionOwner` (P2), no live redesign (P3), no backend changes (Phase B0), no legacy-credential migration or upgrade path (A9: nothing has shipped), no read-back write verification, no global storage gate, no persisted generation.


### Step 1: Add the account identity value types

**Phase:** P1
**Audit reference:** §3 [C01] "Requests can cross account boundaries during replay", Recommended fix (capture account ID and session generation before dispatch); §3 [H01] "Authenticated UI is not synchronized with account-scope readiness", Recommended fix (publish readiness and generation); §3 [M14] "Account-scoped managers redundantly track global account selection", Recommended fix (inject immutable account context). Evidence `apps/mobile/lib/_shared/services/session_interceptor.dart:96–175`, `apps/mobile/lib/features/auth/manager/user_scope_manager.dart`.
**Why (plain words):** Every fix later needs two small facts to travel together: *which account* and *which activation of it*. Today those facts are spread across booleans and a manager that reads global state. Two plain value types plus one enum give every later step something immutable to compare, so a late callback can prove it still belongs to the live account.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/auth/model/account_context.dart` | Create | model | Required | Holds `AccountPhase`, `AccountContext`, `RequestIdentity` — three small types, one file. |
| `apps/mobile/lib/features/auth/model/_model.dart` | Modify | config | Required | Export the new file. |
| `apps/mobile/lib/features/auth/model/account_snapshot.dart` | Create later | freezed model | Optional | Not created here: it is first used by Step 17, so it is created there. Skipping it now keeps this step free of an unused class. |

**Pseudo-code**

```
Simplified from design doc §3 because a payload-free phase and two four-field values need no code
generation; safety preserved because AccountContext is still immutable and generation is still an int.

enum AccountPhase { signedOut, preparing, ready, quiescing, blocked }
    // ready covers "offline with a valid access token": requests fail with a typed error,
    // the UI is still the account's UI. There is no separate readyDegraded.

final class AccountContext
    final String accountId          // server id, never the email
    final int generation            // in-memory activation counter; not persisted (see Step 16)
    REQUIRE accountId != '' AND generation >= 0

final class RequestIdentity
    final String accountId
    final int generation
    final String sessionId          // identifies the credential record that was used
    final int revision              // the vault revision at capture time
    REQUIRE accountId != '' AND sessionId != '' AND revision >= 0
    // no isCancelled flag: cancellation belongs to the HTTP client's cancel token,
    // and requestId already exists for correlation

// Every later step builds one of these BEFORE an await and compares after it.
```

**Done when:**
- `dart analyze` is clean and `build_runner` reports nothing to generate (these types are hand-written).
- `AccountContext(accountId: 'a', generation: 1) == AccountContext(accountId: 'a', generation: 1)` is `true` (value equality from `final` fields and a hand-written `operator ==`).
- No file outside `lib/features/auth/model/` defines another identity/phase type.
- `grep -r "isCancelled" lib/features/auth` returns nothing.

**Tests:** no new test file — the equality and phase transitions are asserted through `apps/mobile/test/app/account_transition_owner_test.dart` in Step 16, which is where they can actually fail.

**Watch out for:** adding `copyWith` "for later". Neither type changes value in place; a new account means a new object, and a `copyWith` invites exactly the mutable-identity bug this step removes.

**Depends on:** Step 0.

---

### Step 2: Write credentials through one serialized, conditional vault

**Phase:** P1
**Audit reference:** §3 [C02] "Session-store mutations are neither atomic nor identity-conditional", Recommended fix (serialize mutations, check the expected account *inside* the serialized operation, conditional replace/remove, one versioned record); §3 [C03] "Explicit logout can leave a usable session recoverable"; §3 [H10] "Stored access-token expiry moves forward on every read", Recommended fix (persist an absolute `expires_at`, converted once at receipt); §3 [H25] "Client validation rejects server-valid accounts and can break credential storage", part "separate wire/storage mapping". Evidence `apps/mobile/lib/_shared/services/session_store.dart:105–156,242–267`, `apps/mobile/lib/features/auth/model/dtos.dart:38–55,79–82`.
**Why (plain words):** Credentials are written in several places, in several steps, and the stored expiry is recomputed from a relative lifetime on every read — so it drifts and can make a dead token look alive. This step makes one record, one key, one queue, and an expiry that is computed exactly once when the response arrives.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/auth/model/credential_record.dart` | Create | freezed model | Required | `CredentialRecord` (freezed) + hand-written `tryDecode`/`toJson` + `RenewalState` enum. |
| `apps/mobile/lib/features/auth/services/session_store.dart` | Create (moved from `_shared/services/session_store.dart`) | service | Required | The only writer of credentials. |
| `apps/mobile/lib/_shared/services/session_store.dart` | Delete | service | Required | Deleted in the same change; no shim (`git mv`, then edit). |
| `apps/mobile/lib/_core/keys/storage_keys.dart` | Modify | config | Required | `auth.vault.v2` and `auth.remembered.v2`; legacy key constants marked `@legacy` for the one-time delete only. |
| `apps/mobile/lib/features/auth/model/dtos.dart` | Modify | freezed model | Required | Convert `expiresIn` to an absolute instant at receipt, with the injected clock. |
| `apps/mobile/lib/_shared/services/secure_storage.dart` | Modify | service | Optional | Only if a caller needs a plain single-key wrapper; skip if `SessionStore` can use the platform interface directly. Loses nothing. |
| `apps/mobile/test/features/auth/session_store_test.dart` | Create | test | Required | Serialization, CAS, fail-closed decode, unconditional removal. |
| `apps/mobile/test/features/auth/auth_expiry_test.dart` | Create | test | Required | Absolute expiry never moves on read. |

**Pseudo-code**

```
Simplified from design doc §3/§7 because nothing has shipped (A9) and a keystore that lies is not
an audit finding; safety preserved because one serialized queue, CAS and unconditional local
removal are all still here.

Storage
    auth.vault.v2        -> one JSON object: the CredentialRecord
    auth.remembered.v2   -> non-secret greeting metadata (name, avatar, accountId)
    legacy keys          -> constants kept ONLY so the one-time delete can name them

CredentialRecord (freezed)   // freezed here: copyWith + equality are used by CAS tests
    accountId, sessionId: String
    accessToken, refreshToken: String
    accessExpiresAtUtc, refreshExpiresAtUtc: DateTime   // ABSOLUTE, UTC
    revision: int                                       // bumps on every successful write
    renewal: RenewalState { idle, inFlight, uncertain } // see Step 5

    static CredentialRecord? tryDecode(Map?)            // hand-written, fails closed
        IF json is null -> RETURN null
        REQUIRE every string non-empty, every date parseable, revision >= 0
        RETURN null on the FIRST violation; never throw, never guess a default

SessionStore (root singleton; the only writer)
    _queue: FIFO of jobs                      // jobs run one at a time, in order
    _memory: CredentialRecord?                // updated only AFTER a successful write

    writeIfMatches(expectedSessionId, next) -> Future<bool>
        ENQUEUE
            current = AWAIT _read()           // inside the job, never before it
            IF current?.sessionId != expectedSessionId -> RETURN false   // CAS lost
            next = next.copyWith(revision: current.revision + 1)
            AWAIT _write(next)                 // throws StorageFailure on platform error
            _memory = next                     // memory changes only after the write
            RETURN true
        // a StorageFailure propagates: memory keeps the OLD record, so the app and
        // the disk never disagree about which token is live

    removeFor(accountId) -> Future<void>
        ENQUEUE  IF _read()?.accountId == accountId -> AWAIT _delete(vaultKey)
                 _memory = null                 // runs even when the server logout failed

    usableCredential(now) -> CredentialRecord?
        record = _memory
        IF record == null OR record.refreshExpiresAtUtc <= now -> RETURN null
        RETURN record                          // access expiry is the caller's problem

    rememberedIdentity() / updateRememberedIdentity(...)   // non-secret, separate key

Expiry, once
    On every auth response: accessExpiresAtUtc = clock.nowUtc() + dto.expiresIn
                            refreshExpiresAtUtc = clock.nowUtc() + dto.refreshExpiresIn
    Reading the record NEVER recomputes either value.

One-time cleanup (A9: this app has no shipped build, so there is no migration to write)
    At startup, delete the six legacy auth keys if present; nothing is decoded or resumed.
```

**Done when:**
- `grep -r "session_store.dart" apps/mobile/lib` shows only the new path; the old file is gone.
- Two concurrent `writeIfMatches` calls with the same expected session id: exactly one returns `true`, the other `false`, and the stored record has one revision bump.
- A record written with a fabricated clock reads back with the same `accessExpiresAtUtc` after `fakeClock.advance(1h)`.
- A JSON object missing `refreshToken` decodes to `null` rather than throwing.
- No file except `session_store.dart` writes `auth.vault.v2`.

**Tests:** `apps/mobile/test/features/auth/session_store_test.dart` — (1) jobs run in arrival order under a held completer; (2) a stale expected session id loses the CAS and leaves the record untouched; (3) `tryDecode` returns `null` for each malformed field; (4) `removeFor` deletes even when the write that follows it fails, and memory ends `null`. `apps/mobile/test/features/auth/auth_expiry_test.dart` — (5) expiry is absolute at receipt; (6) advancing the clock past it makes `usableCredential` return `null` without rewriting the record.

**Watch out for:** computing `expiresAt` inside `CredentialRecord`'s getters "for convenience". That is exactly H10: the value must be a stored field, and the clock is only read at receipt.

**Depends on:** Step 1.

---

### Step 3: Make logout durable and separate it from deactivate

**Phase:** P1
**Audit reference:** §3 [C03] "Explicit logout can leave a usable session recoverable", Recommended fix (distinguish deactivate from logout; remove the captured account's usable credentials locally regardless of the server outcome); §3 [M02] "Add-account flow retains password-only login mode", Recommended fix (an explicit "use another account" mode independent of greeting metadata). Evidence `apps/mobile/lib/features/auth/manager/auth_manager.dart`, `apps/mobile/lib/features/auth/pages/login_page.dart`, `apps/mobile/lib/features/auth/widgets/switch_account_sheet.dart`.
**Why (plain words):** Two actions are tangled today: "switch to another account" keeps the first account signed in, and "log out" must not. Because the local removal only happens after a successful server call, a network failure leaves a usable session on the device while the UI says signed out.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/auth/model/login_form_mode.dart` | Create | enum | Required | `signIn` vs `addAccount`; no booleans. |
| `apps/mobile/lib/features/auth/manager/auth_manager.dart` | Modify | manager | Required | Logout removes locally first, then revokes; deactivate leaves credentials. |
| `apps/mobile/lib/features/auth/pages/login_page.dart` | Modify | widget | Required | Add-account mode always shows the email field. |
| `apps/mobile/lib/features/auth/widgets/switch_account_sheet.dart` | Modify | widget | Required | Two distinct actions with distinct copy. |
| `apps/mobile/test/features/auth/auth_manager_test.dart` | Modify | test | Required | Logout survives a server failure and a storage failure. |

**Pseudo-code**

```
Simplified from design doc §5.4 because a captured "exit channel" capability and a memoised exit
result re-implemented passing one string to one call; safety preserved because the token used for
the server revoke is captured before removal, and local removal still happens first and
unconditionally.

enum LoginFormMode { signIn, addAccount }

AuthManager
    logout(accountId)
        record = store.usableCredential(now)            // capture BEFORE removal
        TRY
            AWAIT store.removeFor(accountId)             // local truth first (C03)
        CATCH StorageFailure
            // one visible state, not a silent sign-out and not an app-wide gate
            state = logoutIncomplete('could not sign out, retry')
            AWAIT scopeTeardown()                        // local resources still close
            RETURN
        AWAIT owner.deactivate(LoggedOut)                // live -> connection -> scope, awaited
        IF record != null -> FIRE-AND-FORGET serverLogout(record.accessToken)
        // the server call may fail; the device is already signed out, so nothing re-appears

    deactivate(accountId){...}                           // account switch: credentials stay
        AWAIT owner.deactivate(AccountSwitched)
        // no credential removal: this is the "add another account" path

Login page
    mode == addAccount -> email field always visible, no "welcome back" greeting prefill
    mode == signIn     -> greeting from RememberedIdentity (non-secret) may prefill, never hides email

Switch account sheet
    "Add another account" -> deactivate (credentials kept)
    "Sign out"            -> logout (credentials removed)
```

**Done when:**
- Logout with the server unreachable still ends signed out on the next launch.
- Logout with a failing local delete shows exactly one retry state and still tears down the live session and scope.
- Deactivate (add account) leaves the first account's credentials intact and switchable.
- Add-account mode always shows the email field, even when a remembered identity exists.

**Tests:** `apps/mobile/test/features/auth/auth_manager_test.dart` — (1) server failure still logs out locally; (2) storage failure yields `logoutIncomplete` and closes resources; (3) deactivate keeps the credential; (4) add-account mode renders the email field.

**Watch out for:** calling the server first "so the token is revoked before the device forgets it". A failed call then leaves a usable session; revoke after removal, with the captured token.

**Depends on:** Step 2.

---

### Step 4: Bind every request to the identity that started it

**Phase:** P1
**Audit reference:** §3 [C01] "Requests can cross account boundaries during replay", Recommended fix (capture account ID and session generation before dispatch, bind refresh/retry to that identity, cancel or reject obsolete operations); §3 [M15] "HTTP failure paths can hang, misclassify, or leak credentials to external origins", Recommended fix (attach tokens only to the approved API origin). Evidence `apps/mobile/lib/_shared/services/session_interceptor.dart:96–124,140–175`, `apps/mobile/lib/_shared/services/http_client.dart:387–406`. Also: §3 [M15] "HTTP failure paths can hang, misclassify, or leak credentials to external origins", Recommended fix (complete every handler exactly once, preserve replay failure/uncertainty, share auth dispatch semantics with downloads); §3 [C01] "Requests can cross account boundaries during replay". Evidence `apps/mobile/lib/_shared/services/session_interceptor.dart:96–150,166–175`, `apps/mobile/lib/_shared/services/http_client.dart:439–490`. Also: §3 [M15] "HTTP failure paths can hang, misclassify, or leak credentials to external origins", Recommended fix (share auth dispatch semantics; retries annotated with identity); §3 [L03] "Timer semantics and retry timing are inaccurate", Recommended fix (apply jitter before enforcing the server's minimum wait; HTTP part only). Evidence `apps/mobile/lib/_shared/services/http_client.dart:439–490,589–605`.
**Why (plain words):** (**A**) The interceptor looks up "the current credential" when a 401 arrives, so a request started as A is replayed as B after a switch. It also attaches a bearer token to any URL it is handed, so an external download URL would receive the account's token. Both are fixed by deciding identity *before* the first await and by never substituting another identity later. (**B**) A storage exception can escape the 401 handler without completing it, so the caller waits forever; and a replay transport failure is reported as the original 401, so "offline" looks like "bad credentials". A single completion guard makes the outcome total: pass, reject-with-this-reason, or replay — chosen once. (**C**) Wrapper retries re-enter authentication without the originating identity, and the retry delay applies shortening jitter *after* choosing the server's `Retry-After`, so the app can retry earlier than the server allowed. This step makes retries identity-preserving, read-only by default, and never below the server minimum.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --- | --- | --- | --- | --- |
| `apps/mobile/lib/features/auth/services/session_interceptor.dart` | Create (moved from `_shared/services/session_interceptor.dart`), then Modify | service | Required | Identity-pinned policy; constructor takes `SessionStore`, `Clock`, approved origin. |
| `apps/mobile/lib/_shared/services/http_client.dart` | Modify | service | Required | Transport only: no `SessionStore` knowledge, injectable interceptor policy, explicit `authMode`. |
| `apps/mobile/lib/_core/config/config.dart` | Modify | config | Required | Expose the normalised approved API origin (scheme + host + effective port). |
| `apps/mobile/test/features/auth/session_interceptor_test.dart` | Create, then Modify | test | Required | The cross-account and cross-origin matrix. |
| `apps/mobile/test/_shared/services/http_client_test.dart` | Modify | test | Required | Retry matrix with `FakeClock`. |

**Pseudo-code**

```
// ===== Part A — Bind every request to the identity that started it =====
RequestIdentity capture(req)
    RETURN (current ready AccountContext, its generation, its token revision, new requestId)
    // captured BEFORE the first await; never re-read later

FUNCTION approvedOrigin(uri)
    n = normalise(uri.scheme, uri.host, effectivePort(uri))
    REQUIRE n == config.apiOrigin      // HTTPS required in production; loopback allowed only in dev
    RETURN true/false

FUNCTION dispatch(req, authMode)
    REQUIRE authMode is explicit: anonymous | account       // never inferred from a path string
    IF authMode == anonymous
        ATTACH nothing; if the caller supplied a bearer, strip it
        RETURN AWAIT transport.send(req)
    // account mode uses ONLY the identity captured by the caller; a logout request passes the
    // token it captured before removal (Step 3) and runs in anonymous mode
    identity = capture(req)
    IF NOT approvedOrigin(req.uri)
        REJECT typed WrongOrigin                         // M15: no credential leaves the API origin
    cred = AWAIT store.usableCredential(identity.account)   // by captured id, never by "active"
    CHECK identity still live (same account, same generation)  // an A->B switch makes this false
    IF cred is null or expired beyond recovery
        REJECT typed AuthUnavailable (reauthentication required)   // NEVER send anonymously
    ATTACH cred.accessToken
    response = AWAIT transport.send(req)
    CHECK identity still live before decode or publication
    RETURN decode(response)

Redirects
    CLIENT SETS followRedirects = false on the API client
    A 3xx in production is a server misconfiguration: surface it as an error
    // a token must never follow a redirect to a location the server chose (M15)
```

```
// ===== Part B — Complete every interceptor path exactly once =====
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

```
// ===== Part C — Align retry and download behaviour with the identity fence =====
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
    mutations: NO automatic retry unless the manager supplied a logical key and the endpoint
               documented pre-auth rejection (audit §6 backend prerequisite)
    never retry: refresh, certificate failure, cancellation, consumed stream, ambiguous mutation
```

**Done when:**
- **[A]** A request captured as account A and released after a switch to B sends no B credential and returns typed cancellation.
- **[A]** A request to `https://example.com/file` in `account` mode is rejected before any bytes leave.
- **[A]** An anonymous auth call runs with no `Authorization` header even if a session exists.
- **[A]** `http_client.dart` contains no reference to `SessionStore` (`grep -n SessionStore` finds none).
- **[B]** Each of the four failures the tests inject produces exactly one terminal outcome and no timeout.
- **[B]** A replay that hits a transport error surfaces the transport error, not `401`.
- **[B]** `grep -n "download" apps/mobile/lib/_shared/services/http_client.dart` shows the same policy call as normal dispatch.
- **[B]** No `Completer` in the interceptor can be left uncompleted (a debug assertion fires in tests if one is).
- **[C]** A `Retry-After: 30` response is honoured for at least 30s under `FakeClock`, never less.
- **[C]** Switching accounts during the delay produces no second dispatch.
- **[C]** A mutation without a logical key is never retried.
- **[C]** Bad-certificate failure is terminal and surfaces as a security failure, not a network outage.

**Tests:** **[A]** `apps/mobile/test/features/auth/session_interceptor_test.dart` — (a) A→B switch during a held request produces no B token, (b) external origin rejected in account mode and stripped in anonymous mode, (c) expired credential yields `AuthUnavailable`, not an anonymous request, (d) cross-origin redirect cannot carry `Authorization`. **[B]** `apps/mobile/test/features/auth/session_interceptor_test.dart` — (a) storage read throws → `StorageUnavailable` once, (b) replay transport failure → transport error, (c) cancelled request during refresh → `Cancelled` once, (d) download 401 replays under the captured identity only. **[C]** `apps/mobile/test/_shared/services/http_client_test.dart` — (a) `Retry-After` floor, (b) no dispatch after generation exit mid-delay, (c) mutation retry only with a logical key, (d) certificate failure not retried.

**Watch out for:** **[A]** resolving the credential by "current account" anywhere in the dispatch path. Only the captured `identity.account` may be used. **[B]** completing the handler inside a `.then` on a fire-and-forget future. The completion must be in the `finally` of the same invocation. **[C]** applying jitter to the server value (it can only add, never subtract).

**Depends on:** Steps 2, 3.

---

### Step 5: Classify refresh outcomes and never retry a possibly-consumed token

**Phase:** P1
**Audit reference:** §3 [H09] "Refresh failures destroy sessions, including ambiguous rotated-token outcomes", Recommended fix (distinguish definite rejection, temporary unavailability and uncertainty after dispatch; never blindly retry a single-use refresh token); §3 [C02] "Session-store mutations are neither atomic nor identity-conditional". Evidence `apps/mobile/lib/_shared/services/session_interceptor.dart`, `apps/mobile/lib/features/auth/services/session_store.dart`.
**Why (plain words):** A refresh token can be spent exactly once. Today any failure is treated the same way, so a network blip can either sign the user out or, worse, replay a token the server already rotated. Four outcomes and one persisted marker make the difference between "the server said no", "the server never saw it" and "I do not know" explicit.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/auth/model/refresh_outcome.dart` | Create | sealed class | Required | `RefreshOutcome` with four cases. Hand-written sealed class, not freezed (no `copyWith` needed). |
| `apps/mobile/lib/features/auth/services/session_store.dart` | Modify | service | Required | Add `markRenewalPending` / `settleRenewal`, both inside the serialized queue. |
| `apps/mobile/lib/features/auth/services/session_interceptor.dart` | Modify | service | Required | Map transport results to outcomes; no retry on uncertainty. |
| `apps/mobile/test/features/auth/refresh_outcome_test.dart` | Create | test | Required | The four branches plus crash recovery. |

**Pseudo-code**

```
Simplified from design doc §5.5 because five outcomes and a persisted attempt journal were more
states than the audit names (three categories); safety preserved because a possibly-consumed token
is still never retried, and a crash mid-refresh still forces a re-login.

sealed class RefreshOutcome
    Succeeded(record)
    Rejected(reason)                 // server refused: 401/403/invalid_grant after dispatch
    Unavailable(retryAfter)          // server definitely did not consume it
    Uncertain()                      // dispatched, no definitive answer

SessionStore.refresh(identity) -> RefreshOutcome        // entries run in the queue
    current = AWAIT _read()
    IF current == null OR current.accountId != identity.accountId -> RETURN Rejected('identity')
    IF current.renewal != idle -> RETURN Uncertain()     // already in flight or already unclear

    AWAIT _write(current.copyWith(renewal: inFlight))    // BEFORE the request goes out
    result = AWAIT _post(current.refreshToken)

    MATCH result
        200 with a new record -> AWAIT _write(next.copyWith(revision: +1, renewal: idle))
                                 RETURN Succeeded(next)
        401 / 403 / invalid_grant -> AWAIT _delete()    // the server will not accept it again
                                     RETURN Rejected(code)
        429 / 503 with Retry-After -> AWAIT _write(renewal: idle)   // definitely not consumed
                                      RETURN Unavailable(retryAfter)
        connection refused, nothing sent -> AWAIT _write(renewal: idle)
                                            RETURN Unavailable(none)
        timeout, reset, 5xx after send -> AWAIT _delete()  // it MAY have been consumed
                                          RETURN Uncertain()

Startup
    record = AWAIT store.read()
    IF record.renewal != idle -> AWAIT store.removeFor(record.accountId)  // forces a fresh login
    // an app killed mid-refresh is indistinguishable from a consumed token,
    // so the safe default is to require login rather than retry

Interceptor
    MATCH outcome
        Succeeded  -> replay the original request ONCE with the new access token
        Rejected   -> complete the request with an auth error; no sign-out side effect here
        Unavailable -> complete with a retry-later error; keep the session
        Uncertain  -> complete with "please sign in again"; never retry, never replay
```

**Done when:**
- All four outcomes are produced by a test, each from a distinct server/transport behaviour.
- A second `refresh` call while one is in flight returns `Uncertain()` without a second HTTP request.
- Killing the process between `inFlight` and the response leaves the next launch signed out.
- No code path retries a refresh whose outcome was `Uncertain`.
- The interceptor completes the original request exactly once in every branch.

**Tests:** `apps/mobile/test/features/auth/refresh_outcome_test.dart` — (1) 200 rotates and replays once; (2) 401 rejects and removes the record; (3) 429 with `Retry-After` keeps the record and reports retry-later; (4) a timeout after send removes the record and reports sign-in-again; (5) a restart with `renewal: uncertain` lands signed out.

**Watch out for:** treating a 5xx as "not consumed". The request reached the server; a 502 from a proxy can hide a completed rotation. That is `Uncertain`, and `Uncertain` means re-authenticate.

**Depends on:** Step 4.

---

### Step 6: Align client validation with the server and stop validating during serialization

**Phase:** P1
**Audit reference:** §3 [H25] "Client validation rejects server-valid accounts and can break credential storage", Recommended fix (shared accepted/rejected fixtures, aligned documented policies, separate wire/storage mapping, do not discover invalidity during serialization); evidence `apps/mobile/lib/_core/value_objects/email.dart:9–35`, `password.dart:67–84`, `text_value_objects.dart:17–18,52`, `apps/mobile/lib/_shared/model/dtos.dart:35–55`, `apps/api/src/modules/auth/validators.rs:128–270`.
**Why (plain words):** The client email regex, password length counting (code units vs Unicode scalars) and three-character name minimum disagree with the server. A server-valid account can therefore fail login, and a value that was fine on the wire can throw when it is written to the vault. One shared fixture file makes the policies provably equal, and mapping stops using form-policy constructors.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | -------- | ----- |
| `packages/account-contract-fixtures/auth-validation.json` | Create | config | Required | Language-neutral accepted/rejected cases. |
| `apps/mobile/lib/_core/value_objects/email.dart` | Modify | config | Required | Match the server's real accepted policy. |
| `apps/mobile/lib/_core/value_objects/password.dart` | Modify | config | Required | Read the limits from `apps/api/src/modules/auth/validators.rs:128–270` and match them; do not copy numbers from this guide. |
| `apps/mobile/lib/_core/value_objects/text_value_objects.dart` | Modify | config | Required | Same rule: take the minimum length from `validators.rs`, do not hard-code it here. |
| `apps/mobile/lib/_shared/model/dtos.dart` | Modify | freezed model | Required | Server→domain mapping no longer runs through throwing form constructors. |
| `apps/api/tests/auth_validators.rs` | Create | test | Required | Same fixture, Rust side. |

**Pseudo-code**

```
Fixture file (owned jointly with the API)
    { "accepted": [ {kind:"email", value:"a+b_c@example.co.uk"}, ... ],
      "rejected": [ {kind:"password", value:"short"}, ... ] }
    INCLUDE plus/underscore/hyphen emails, trailing garbage, Unicode scalar boundaries at the server's
            min and max, minimum-length names, long passwords, whitespace/normalisation cases

Mobile validation
    isValidEmail(v)  -> same rule set the server documents; no "universal" regex
    isValidPassword(v) -> length in RUNES (not .length) within the server's range; nothing else required
    isValidFullName(v) -> trimmed runes >= the server's minimum
    Registration form shows these; LOGIN password check is presence-only

Server → domain mapping
    decode wire value as a raw String
    IF it is needed for future form submission, apply policy then; the mapping itself accepts server truth
    NEVER call getOrCrash / .value on a form value object inside toCredentialRecord
```

**Done when:**
- The mobile policy test and the Rust validator test both read the same JSON and both pass.
- Changing one accepted case in the fixture makes the other language's test fail (prove it once, then revert).
- A response containing a two-character name and a password with only letters persists into a `CredentialRecord` without throwing.
- `grep -n "getOrCrash" apps/mobile/lib/_shared/model/dtos.dart` finds nothing.

**Tests:** `apps/mobile/test/_core/value_objects/validation_parity_test.dart` (fixture-driven accepted/rejected), `apps/api/tests/auth_validators.rs` (same fixture), `apps/mobile/test/_shared/model/dtos_test.dart` (server-valid values map without throwing).

**Watch out for:** tightening the password policy to "must contain a symbol" because it feels safer. The server does not, so client-only strictness breaks valid accounts.

**Depends on:** Steps 1, 2.

---

### Step 7: Report production failures safely and stop the blanket sign-out

**Phase:** P1
**Audit reference:** §3 [M23] "Production logging suppresses every severity", Recommended fix (explicit sanitised production warning/error policy and reporting sink; never log credentials or private content); design §5.13 "never sign out globally from any single 401" — the C01 fix implies this but the audit's C01 text does not name it, so it is cited to the design. Evidence `apps/mobile/lib/_core/logging/meno_logger.dart:68,100–143`, `apps/mobile/lib/_core/exceptions/global_exception_handler.dart:13–39`.
**Why (plain words):** `_on == env.isDev` gates warnings, errors and fatal events, so a production lifecycle or storage failure leaves no trace. In the other direction, any 401 reaches a global handler that signs out "the current account" — which is exactly wrong once identity is per-request. Both are about who is allowed to react to a failure, so they change together.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/_core/logging/meno_logger.dart` | Modify | service | Required | Level policy by environment; allowlisted fields. |
| `apps/mobile/lib/_core/exceptions/global_exception_handler.dart` | Modify | config | Required | Routes identity-bearing auth failures; no blanket `signOut`. |
| `apps/mobile/test/_core/logging/meno_logger_test.dart` | Modify | test | Required | Redaction with sentinel secrets. |

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
        ELSE hand an identity-specific rejection to the account owner (the transition owner, Step 16)
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

**Depends on:** Steps 1, 4.

---

### Step 8: Make production configuration fail loudly instead of silently

**Phase:** P1
**Audit reference:** §3 [M18] "Production can select a successful silent transport; recovery stores a bearer token in preferences", Recommended fix (reject invalid production configuration; allow the silent transport only through explicit test/dev configuration); evidence `apps/mobile/lib/locator.dart:348–358`, `apps/mobile/lib/_core/config/config.dart:34–38`.
**Why (plain words):** An empty LiveKit room URL currently falls back to the fake transport, so a release can show "live" with no audio and no error. A configuration mistake must be a visible startup failure, and the fake must be reachable only by an explicit test/dev switch.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/_core/config/config.dart` | Modify | config | Required | `validate()`: required URLs, scheme checks, no empty room config in production. |
| `apps/mobile/lib/locator.dart` | Modify | config | Required | Transport selection reads an explicit `useFakeRoomTransport` flag. |
| `apps/mobile/test/_core/config/config_test.dart` | Modify | test | Required | Validation matrix. |

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

**Depends on:** Step 7.

---

### Step 9: Stop keeping secrets and viewer-blind caches outside the vault

**Phase:** P1
**Audit reference:** §3 [M18] "Production can select a successful silent transport; recovery stores a bearer token in preferences", Recommended fix (persist minimal non-secret recovery intent); §3 [H14] "Viewer-specific profile cache state leaks across accounts", Recommended fix (viewer-keyed cache; no viewer-blind reads); §3 [M05] "Local storage isolation is optional rather than enforced", part "owner-qualified local access". Evidence `apps/mobile/lib/features/broadcast/services/live_session_store.dart`, `apps/mobile/lib/features/profile/services/profile_cache_service.dart`, `apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart`.
**Why (plain words):** Two smaller secrets exist outside the new vault: a live-session recovery record that carries a bearer token, and a profile cache that cannot tell which account wrote an entry. Before accounts can be switched, both must stop being able to hand account A's data to account B.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/services/live_session_store.dart` | Modify | service | Required | Store only `broadcastId` + intended phase. No token, no credential, no URL with a token. |
| `apps/mobile/lib/features/profile/services/profile_cache_service.dart` | Modify | service | Required | Every read/delete takes `viewerId`; a viewer-less read is removed, not defaulted. |
| `apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart` | Modify | service | Required | Every method takes `ownerId` (Step 38 turns this into a constructor field and removes the parameter). |
| `apps/mobile/test/features/broadcast/live_session_store_test.dart` | Create | test | Required | Proves the encoded record has no secret fields. |

**Pseudo-code**

```
Simplified from design doc §7 because there is no installed build to migrate (A9) and no tombstone
protocol is needed; safety preserved because nothing outside the vault can hold a credential or
answer a read without an account id.

LiveSessionStore (was: recovery record with a token)
    write(broadcastId, intendedPhase)     // two fields, both non-secret
    read() -> { broadcastId, intendedPhase }?
    // a recovery intent is enough to reopen the live page after a restart;
    // the room token is fetched fresh from the server, never restored
    REMOVE token, url, expiresAt, accountName fields

ProfileCacheService
    read(viewerId, subjectId)             // REQUIRE both non-empty
    write(viewerId, subjectId, snapshot)
    clear(viewerId)
    REMOVE any method that reads without viewerId
    REMOVE any "current viewer" field: the viewer is an argument, not ambient state

BroadcastLocalService
    every query/mutation gains ownerId    // Step 38 makes it a constructor field
    DELETE rows whose owner cannot be determined rather than showing them to whoever asks

One-time hygiene (A9: no shipped build, so no migration)
    At startup, delete preference keys that held a bearer token or a viewer-less profile entry.
```

**Done when:**
- `grep -rn "token" apps/mobile/lib/features/broadcast/services/live_session_store.dart` returns nothing.
- A profile read with an empty `viewerId` is impossible: the compiler rejects the call and no default exists.
- No preference key outside `auth.vault.v2` contains a value issued by the auth API.
- A cached profile written by account A is not returned for account B (asserted in Step 39's test, once the key includes the viewer).

**Tests:** `apps/mobile/test/features/broadcast/live_session_store_test.dart` — (1) the encoded map contains exactly `broadcastId` and `intendedPhase`; (2) reading a record written by another account returns `null` once Step 39 lands.

**Watch out for:** keeping a "migration" reader for old records. With no shipped build (A9) there is nothing to read; a reader that tolerates a token is a reader that can resurrect one.

**Depends on:** Step 2.

---

### Step 10: Move auth persistence out of _shared

**Phase:** P1
**Audit reference:** §3 [M22] "Shared infrastructure imports features and broad barrels hide the dependency graph", Recommended fix (move auth persistence into auth; narrow imports); design §8 "Move/modify" rows for `session_store.dart`, `session_interceptor.dart`, `session.dart`. Evidence `apps/mobile/lib/_shared/_shared.dart:1–23`, `apps/mobile/lib/_shared/services/session_store.dart:7`.
**Why (plain words):** `_shared` currently imports `features/auth` and re-exports it through a barrel, so the claimed feature-independent bottom layer is not real and no one can tell which direction a dependency points. Moving the three auth files and fixing their imports restores a one-way graph before P2 adds more app-level code.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/_shared/services/session_store.dart` | Move | service | Required | → `apps/mobile/lib/features/auth/services/session_store.dart` (created there in Step 4; this deletes the old path, after call sites move). |
| `apps/mobile/lib/_shared/services/session_interceptor.dart` | Move | service | Required | → `apps/mobile/lib/features/auth/services/session_interceptor.dart` (created there in Step 4; this deletes the old path). |
| `apps/mobile/lib/_shared/model/session.dart` | Move | freezed model | Required | → `apps/mobile/lib/features/auth/model/session.dart`. |
| `apps/mobile/lib/_shared/_shared.dart` | Modify | config | Required | Stop exporting the moved files. |
| `apps/mobile/lib/features/auth/_services.dart`, `.../model/_model.dart` | Modify | config | Required | Export from the feature's own narrow barrels. |
| `apps/mobile/test/locator_test.dart` | Modify | test | Optional | Only if it asserted the old import path; loses nothing to update it now. |

**Pseudo-code**

```
MOVE the three files; update every import
    RUN `rg -l "session_store|session_interceptor|model/session" apps/mobile/lib apps/mobile/test` and fix each
DELETE the moved originals (no re-export shim left behind)
SHARED barrel
    REMOVE the auth exports
    LEAVE genuinely shared exports alone
FEATURE barrels
    auth/_services.dart exports session_store + session_interceptor
    auth/model/_model.dart exports session + account_context + account_snapshot + credential_record + refresh_outcome
CHECK direction
    `rg -n "features/" apps/mobile/lib/_shared` -> only the known, planned exceptions still to be handled in Step 54
```

**Done when:**
- No file under `lib/_shared` imports `features/auth` for session types.
- `lib/_shared/_shared.dart` does not export anything from `features/`.
- `dart analyze` clean; the app boots and login/logout still work against a stub API.

**Tests:** run the existing `apps/mobile/test/locator_test.dart` and `apps/mobile/test/app_test.dart` unchanged; they fail if a registration or import moved incorrectly.

**Watch out for:** leaving an `export 'package:.../features/auth/...'` shim "to avoid touching call sites". That keeps the bad direction and hides it again.

**Depends on:** Steps 2, 4.

---

## 8. Phase B0 — Backend account security

**Findings finished here:** [C05], [H21], [H09] (backend half), [H22], [H23].

**What you will have when this phase is done:** the HTTP guard actually rejects tokens revoked account-wide, a password reset revokes nothing until ownership is proven, refresh rotation answers with distinct documented error codes, notes cannot lose a folder created in the same batch, and chat deletion cannot publish across rooms.

**How to know it's safe to ship:** `cargo test` green for `auth_router`, `auth_service`, `notes_repository_pg`, `chat_service`, and each fix has a regression test that fails on the old code.

**Order:** 11 → 12 → 13 → 14. This phase sits here, not at the end, because audit §8 puts backend revocation second: mobile isolation is worthless if a revoked token still works server-side.

**What we are deliberately NOT doing in B0:** no stored-outcome replay for refresh, no new recovery endpoint, no token format change, no session table, no rate-limit redesign beyond the existing 429 path.


### Step 11: Backend: enforce account-wide revocation and prove reset ownership

**Phase:** B0
**Audit reference:** §3 [C05] "Backend access-token revocation is not enforced by the actual HTTP guard", Recommended fix (validate both token-specific and user-wide revocation using the verified subject and issuance time; test through a protected router using production wiring); evidence `apps/api/src/modules/auth/token.rs:540–565,600–614`, `apps/api/src/modules/auth/cache/redis_store.rs:155–187`, `apps/api/src/middleware/auth.rs:281–290,456–459`, `apps/api/src/state.rs:314–321`. Also: §3 [H21] "Password-reset requests revoke sessions before proving ownership", Recommended fix (revoke only after successful reset-code verification and password replacement; retain enumeration-resistant responses); evidence `apps/api/src/routes/auth.rs:95–110`, `apps/api/src/modules/auth/credentials.rs:168–189`.
**Why (plain words):** (**A**) Logout-all and password reset write a user-wide invalidation marker, but the installed guard only asks "is this JTI revoked?", so existing access tokens keep working for the rest of their lifetime. The blocklist implementation already knows how to answer the user-wide question — the guard never asks it. (**B**) The public forgot-password endpoint revokes the target user's sessions before any code is checked, so knowing an email address is enough to disrupt that account — and Step 4 makes the disruption immediate. Moving revocation after verification removes the lever while keeping the response identical for unknown addresses.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --- | --- | --- | --- | --- |
| `apps/api/src/middleware/auth.rs` | Modify | Rust module | Required | Add the trait method; call it in `authenticate`. |
| `apps/api/src/modules/auth/cache/redis_store.rs` | Modify | Rust module | Required | Delegate to the existing `is_user_tokens_blocked`. |
| `apps/api/src/modules/auth/cache/memory_store.rs` | Modify | Rust module | Required | Same behaviour for in-memory wiring and tests. |
| `apps/api/tests/auth_router.rs` | Modify | test | Required | Production-wiring guard test. |
| `apps/api/src/routes/auth.rs` | Modify | Rust module | Required | Forgot-password no longer revokes. |
| `apps/api/src/modules/auth/credentials.rs` | Modify | Rust module | Required | Reset path revokes after the password is replaced. |

**Pseudo-code**

```
// ===== Part A — Enforce account-wide revocation in the HTTP guard =====
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

```
// ===== Part B — Revoke sessions only after reset ownership is proven =====
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
- **[A]** `POST /auth/logout-all` followed by a request with the old access token returns `401 InvalidToken`.
- **[A]** A token issued *after* logout-all still works (the marker comparison is `<=`).
- **[A]** A Redis/in-memory backend failure returns an infrastructure error, not a 401.
- **[A]** The test drives the production guarded router, not a hand-built one.
- **[B]** Calling forgot-password twice leaves existing access tokens valid.
- **[B]** A successful reset invalidates them (immediately; the client's local removal is Step 3).
- **[B]** An unknown email and a known email produce identical responses.
- **[B]** A failed code attempt leaves sessions untouched.

**Tests:** **[A]** `apps/api/tests/auth_router.rs` — (a) logout-all invalidates existing access tokens, (b) a fresh login survives, (c) blocklist failure is not an auth failure, (d) JTI-level revocation still works. **[B]** `apps/api/tests/auth_router.rs` — (a) request does not revoke, (b) successful reset revokes, (c) identical responses for unknown email, (d) wrong code does not revoke.

**Watch out for:** **[A]** checking the user-wide marker before signature verification. That makes the blocklist a probe for arbitrary tokens.

**Mobile fallback until this lands:** Local logout remains authoritative and is described as "remote revocation unconfirmed"; the client never claims account-wide invalidation already took effect. **[B]** Revoking on request to be safe. That is the vulnerability; revocation belongs after verification.

**Mobile fallback until this lands:** Requesting a reset is never treated as ownership proof, and the client does not log the current account out on a reset request.

**Depends on:** nothing (backend repo).

---

### Step 12: Backend: keep refresh rotation outcome-safe without stored replays

**Phase:** B0
**Audit reference:** §3 [H09] "Refresh failures destroy sessions, including ambiguous rotated-token outcomes", backend half — the server must tell "expired" apart from "already used", and must not spend a token on a request it is going to refuse. Evidence `apps/api/src/modules/auth/token.rs:439–486`, `apps/api/src/modules/auth/error.rs:126–137`.
**Why (plain words):** The mobile side can only classify an outcome if the server gives it a distinguishable answer. Today a reused token and an expired token look alike, and a rate-limited request can still rotate the token before refusing — which is how a client ends up locked out after a harmless retry.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/api/src/modules/auth/token.rs` | Modify | Rust module | Required | Check rate limit and reuse *before* rotation; rotate and mark consumed in one step. |
| `apps/api/src/modules/auth/error.rs` | Modify | Rust module | Required | Add `RefreshTokenExpired` and `RefreshTokenReused` to the documented error codes. |
| `apps/api/tests/auth_service.rs` | Modify | test | Required | Three regression cases. |

**Pseudo-code**

```
Simplified from design doc §11 because audit H09 names three categories and the middleware already
has an error surface; safety preserved because a reused token is still refused and a rate-limited
request still spends nothing. No stored-outcome replay: the review showed it contradicts the
"re-authenticate on uncertainty" contract in Step 5 and needs a table nobody asked for.

refresh(refresh_token)
    claims = VERIFY signature and expiry
    IF expired -> RETURN RefreshTokenExpired           // distinct code, same 401 status
    IF rate_limited(account_id) BEFORE any mutation
        RETURN 429 + Retry-After                        // token NOT consumed
    BEGIN transaction
        row = SELECT ... FOR UPDATE WHERE token_hash = hash(refresh_token)
        IF row.consumed_at IS NOT NULL -> ROLLBACK; RETURN RefreshTokenReused
        INSERT new token row
        UPDATE row SET consumed_at = now()
    COMMIT
    RETURN new access + refresh pair

// The client maps Expired and Reused to Rejected (Step 5): both mean "sign in again".
// 429 with Retry-After maps to Unavailable, which keeps the session.
```

**Done when:**
- Presenting an already-rotated token returns `RefreshTokenReused`, not a generic 401 body.
- Presenting an expired token returns `RefreshTokenExpired`.
- A request refused by the rate limiter leaves the presented token usable on the next attempt (asserted by reusing it successfully).
- Two concurrent refreshes with the same token: exactly one succeeds, the other returns `RefreshTokenReused`.
- No new table and no outcome cache were added.

**Tests:** `apps/api/tests/auth_service.rs` — (1) reuse after rotation is rejected with the distinct code; (2) expiry is rejected with its own code; (3) a rate-limited attempt does not consume the token; (4) concurrent rotation has exactly one winner.

**Watch out for:** consuming the token inside the rate-limit branch "to be safe". That turns a throttled retry into a forced sign-out and is the failure H09 describes.

**Depends on:** Step 11.

---

### Step 13: Backend: resolve note folder ownership after folder mutations

**Phase:** B0
**Audit reference:** §3 [H22] "Notes sync can lose folders created in the same batch", Recommended fix (resolve live folder ownership after folder mutations; add PostgreSQL regression coverage), and the same finding's "folder/PATCH semantics" half — a note PATCH that omits `folder_id` must not unfile the note. Evidence `apps/api/src/modules/notes/repository/pg.rs:867–902`, `apps/api/src/modules/notes/model.rs:181–186`.
**Why (plain words):** Two ways a note loses its folder. In a sync batch that creates a folder and files a note into it, ownership is resolved against state read before the insert, so the note keeps a folder id nobody can resolve. And a PATCH that simply does not mention `folder_id` currently writes a `NULL`, which unfiles the note even though the client never asked for that.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/api/src/modules/notes/repository/pg.rs` | Modify | Rust module | Required | Resolve folder ownership after mutations; PATCH writes `folder_id` only when supplied. |
| `apps/api/src/modules/notes/model.rs` | Modify | Rust module | Required | Make the folder field an `Option<Option<Uuid>>` (absent vs explicit null) or an explicit `FolderChange` enum. |
| `apps/api/tests/notes_repository_pg.rs` | Modify | test | Required | Same-batch case plus the PATCH-omission case. |

**Pseudo-code**

```
update_note(request)
    MATCH request.folder_change                       // NOT a plain Option<Uuid>
        Unchanged            -> do not touch the column
        Set(folder_id)       -> verify the folder belongs to the same owner, then write it
        Clear                -> write NULL             // only when the client explicitly clears

same_batch_sync(items)
    FOR each item
        upsert folder (if present)
        upsert note
    AFTER all upserts, in the same transaction
        resolve every note's folder id against the rows that now exist
        IF a folder id still cannot be resolved -> RETURN a validation error naming the note
    // resolving before the inserts is what loses folders created in the same batch
```

**Done when:**
- A PATCH that omits the folder field leaves `folder_id` byte-identical.
- A PATCH that sends `folder_id: null` clears it.
- A batch that creates a folder and files a note into it in one request returns a note whose folder exists.
- A batch that references a folder it never creates fails with a validation error rather than silently nulling.

**Tests:** `apps/api/tests/notes_repository_pg.rs` — (1) same-batch folder create + assignment resolves; (2) PATCH without `folder_id` preserves the existing folder; (3) PATCH with explicit null clears it; (4) unresolvable folder reference is rejected.

**Watch out for:** deriving `Unchanged` from `None` while the DTO also uses `None` for "clear". Two meanings on one value is exactly how the wrong branch gets taken; use three distinct states.

**Depends on:** nothing (backend repo).

---

### Step 14: Backend: guard chat deletion by room as well as sender

**Phase:** B0
**Audit reference:** §3 [H23] "Chat deletion can mutate one room and publish to another", Recommended fix (include broadcast identity in the guarded delete and test cross-room requests); evidence `apps/api/src/modules/chat/service.rs:252–279`, `apps/api/src/modules/chat/repository/pg.rs:235–252`.
**Why (plain words):** The delete is guarded by message id and sender only, and the broadcast id is checked *after* a failed delete, so an author can delete a message through another room's id and send a deletion event there. Putting the room into the guarded statement removes the possibility rather than reporting it.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/api/src/modules/chat/repository/pg.rs` | Modify | Rust module | Required | `... WHERE id = $1 AND broadcast_id = $2 AND sender_id = $3 AND deleted_at IS NULL`. |
| `apps/api/src/modules/chat/service.rs` | Modify | Rust module | Required | Publish only after a guarded success. |
| `apps/api/tests/chat_service.rs` | Modify | test | Required | Cross-room matrix. |

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

**Depends on:** nothing (backend repo).

---

## 9. Phase P2 — Authoritative account lifecycle

**Findings finished here:** [H01], [H02] (admission), [M14], [M19], [M16] (destination half), [M01], [M02], [L02].

**What you will have when this phase is done:**
- One `AccountTransitionOwner` that serializes activation and deactivation and publishes `AccountPhase`.
- A generation-keyed gate so the old account subtree is disposed before the scope is popped.
- A startup retry that re-runs registration instead of re-awaiting a failed future.
- A router that refreshes on identity, with one validated pending destination that cannot cross accounts.
- `AuthManager` reduced to UI commands; one current-user presentation source; codes sent only on tap.

**How to know it's safe to ship:** the transition suite passes; switching A→B never mounts B while A's widgets still exist; a tap that arrives before readiness is consumed once and dropped if it belongs to another account; logging out during a slow login leaves the app signed out.

**Order:** 15 → 16 → 17 → 18 → 19 → 20 → 21 → 22 → 23. Step 16 needs 15 (the fence it calls). Land 17 with 16 if you prefer one change; the gate is useless without the owner.

**What we are deliberately NOT doing in P2:** no unmount hub or acknowledgement protocol (the frame boundary is enough), no attempt-id ledger at bootstrap, no notification-tap queue with an account chooser, no persisted transition intent, no second attempt-ticket counter in `AuthManager`.


### Step 15: Fence the old live tier so it cannot outlive a transition

**Phase:** P2
**Audit reference:** §3 [H02] "Live transitions can outlive account teardown or supersede newer sessions", Recommended fix (quiesce before disposing resources; interim step until one owner exists); §3 [H04] "Failed end/leave skips session closure", part "account-exit closure". Evidence `apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart:155–223,289–340`.
**Why (plain words):** Until Step 27 replaces the live tier, an account can be torn down while a broadcast is still opening. This is a twenty-line brake, not a design: stop accepting entries, wait for the open that is already running, then close. Step 27 replaces the single call with the real manager.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart` | Modify | manager | Required | Add the flag, the in-flight handle and `closeForAccountExit()`. |
| `apps/mobile/test/features/broadcast/live_exit_fence_test.dart` | Create | test | Required | Proves the brake holds. |

**Pseudo-code**

```
Simplified from design doc §4.3 because an "exit channel" capability, a memoised result and an
adapter class all re-implemented one idempotent close; safety preserved because the open is still
awaited, new entries are still refused, and the caller still cannot proceed without the close.

LiveScopeManager
    _acceptingEntries = true
    _inFlightOpen: Future<void>?

    open(...)
        IF NOT _acceptingEntries -> THROW TransitionInProgress
        _inFlightOpen = _doOpen(...)
        AWAIT _inFlightOpen

    closeForAccountExit() -> Future<void>
        _acceptingEntries = false                 // 1. no new entries, synchronously
        AWAIT _inFlightOpen?.catchError(ignore)    // 2. let the open finish or fail
        IF liveId != null -> AWAIT _endOrLeave(liveId)   // 3. one close, runs in finally
        liveId = null

// Step 16's owner calls this before popping the account scope.
// Step 27 deletes this class and calls ActiveBroadcastManager.closeForAccountExit() instead.
```

**Done when:**
- An `open()` that starts after `closeForAccountExit()` begins throws `TransitionInProgress` and creates no transport.
- `closeForAccountExit()` returns only after the in-flight open has completed or failed.
- Calling it twice closes once (idempotent).
- Exactly one call site exists: the transition owner.

**Tests:** `apps/mobile/test/features/broadcast/live_exit_fence_test.dart` — (1) a held open completer, then `closeForAccountExit()`; assert the close waits and the transport is closed after it releases; (2) a second entry attempt during close is refused; (3) double close performs one end/leave.

**Watch out for:** awaiting the in-flight open *after* clearing `liveId`. The open assigns it when it completes, so the close would miss the thing it was told to close.

**Depends on:** Steps 3, 4.

---

### Step 16: Replace UserScopeManager with one transition owner

**Phase:** P2
**Audit reference:** §3 [H01] "Authenticated UI is not synchronized with account-scope readiness", Recommended fix (publish account transition/readiness and generation; gate authenticated routes); §3 [M14] "Account-scoped managers redundantly track global account selection", Recommended fix (inject immutable account context; initialize once); §3 [H02] "Live transitions can outlive account teardown or supersede newer sessions"; §3 [L02] "Derived observables are recreated by getters". Evidence `apps/mobile/lib/features/auth/manager/user_scope_manager.dart`, `apps/mobile/lib/app.dart`.
**Why (plain words):** `UserScopeManager` both decides when an account becomes usable and is read by pages as a flag, so "did this finish?" has several answers and two activations can overlap. One owner that runs jobs in order, publishes a phase, and increments a generation answers it once.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/app/account_transition_owner.dart` | Create (moved from `features/auth/manager/user_scope_manager.dart`) | service | Required | The only thing that pushes or pops the account scope. |
| `apps/mobile/lib/features/auth/manager/user_scope_manager.dart` | Delete | manager | Required | Deleted in the same change; no shim. |
| `apps/mobile/lib/app/composition/account_registrations.dart` | Create | config | Required | The body of the scope: one function, no logic beyond registrations. |
| `apps/mobile/test/app/account_transition_owner_test.dart` | Create (moved from `test/features/auth/user_scope_manager_test.dart`) | test | Required | Serialization, supersession, rollback, exit ordering. |

**Pseudo-code**

```
Simplified from design doc §5.2 because persisting a generation and a transition marker protects
nothing (nothing survives a process kill, and the vault then holds one valid account anyway);
safety preserved because generation is still strictly increasing while the process lives, and two
activations still cannot overlap.

AccountTransitionOwner (root singleton)
    _phase: ValueNotifier<AccountPhase>(signedOut)     // published, not polled
    _current: AccountContext?
    _generation = 0                                    // in memory only
    _queue: FIFO                                        // one job at a time, arrival order
    _intent = 0                                        // the ONE attempt counter (Step 20 uses it)

    beginAttempt() -> int
        RETURN ++_intent                                // a newer login supersedes an older one

    activate(credential, attempt) -> Future<TransitionResult>
        ENQUEUE
            IF attempt != _intent -> RETURN Superseded()   // a newer intent already won
            _phase.value = preparing
            AWAIT _previous?.closeForAccountExit()         // Step 15's brake (Step 27 swaps it)
            IF get_it.hasScope(authenticated) -> AWAIT popScope()
            _generation += 1
            context = AccountContext(credential.accountId, _generation)
            PUSH scope(authenticated)
            TRY   registerAccountScope(context, credential)   // composition only
                  AWAIT get_it.allReady()
            CATCH error
                  AWAIT popScope()                        // roll back a half-built scope
                  _phase.value = signedOut
                  RETURN Failed(error)
            _current = context
            _phase.value = ready
            RETURN Activated(context)

    deactivate(reason) -> Future<void>
        ENQUEUE
            IF _current == null -> RETURN
            _phase.value = quiescing
            AWAIT live.closeForAccountExit()              // live first (H02)
            AWAIT connection.stop()                       // then transport
            AWAIT popScope()                              // disposal is awaited, not scheduled
            _current = null
            _phase.value = signedOut
            // _generation is NOT reset: a later activation must still be strictly newer,
            // so late callbacks from the previous account can never match

    phase  -> ValueListenable<AccountPhase>
    context -> AccountContext?       // null unless ready

sealed class TransitionResult { Activated(context) | Superseded() | Failed(error) }

Facts kept separate (Step 25 uses them):
    creatorId     who owns the broadcast
    canPublish    microphone/stream permission
    canEnd        creator authority
    // one boolean must never answer two of these
```

**Done when:**
- Two `activate` calls: the first returns `Superseded()`, the second `Activated`, and the scope is pushed once.
- A failing registration leaves `phase == signedOut` and no scope pushed (asserted with a throwing fake registration).
- `deactivate` awaits scope disposal before it returns (assert with a completer inside a registered `dispose`).
- `generation` increases across two activations and is never written to storage.
- `UserScopeManager` no longer exists anywhere in `lib/`.

**Tests:** `apps/mobile/test/app/account_transition_owner_test.dart` — (1) FIFO ordering under held completers; (2) supersession by a newer attempt ticket; (3) rollback of a partially built scope; (4) exit order is live → connection → scope.

**Watch out for:** resetting `_generation` on deactivate. That re-opens the exact race the counter exists to close: a callback from account A would satisfy a `generation == 1` check made by account B.

**Depends on:** Steps 2, 3, 15.

---

### Step 17: Gate the account subtree by generation

**Phase:** P2
**Audit reference:** §3 [H01] "Authenticated UI is not synchronized with account-scope readiness", Recommended fix (gate authenticated routes and remount the account subtree by generation); §3 [H15] "Async operations continue after their owner is disposed", Recommended fix (check the owner/generation after every await; invalidate continuations on disposal). Evidence `apps/mobile/lib/app.dart:106–109`, `apps/mobile/lib/locator.dart`.
**Why (plain words):** The router can show an authenticated page before the account's registrations exist, and when an account leaves, the old widgets can still be mounted while the scope that fed them is disposed. A gate keyed by generation makes the widget tree and the DI scope change together, one frame apart.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/app/account_gate.dart` | Create | widget | Required | Chooses the subtree from the published phase and keys it by generation. |
| `apps/mobile/lib/features/auth/model/account_snapshot.dart` | Create | freezed model | Required | `AccountSnapshot` — freezed, because the gate and the router compare snapshots by value. |
| `apps/mobile/lib/app.dart` | Modify | widget | Required | Wrap the authenticated branch in the gate. |
| `apps/mobile/lib/app/composition/root_registrations.dart` | Create | config | Required | Root registrations in one function (used by Step 18). |
| `apps/mobile/test/app/account_gate_test.dart` | Create | test | Required | Old subtree disposes before pop; new generation remounts. |

**Pseudo-code**

```
Simplified from design doc §4.4 because a registration hub with per-generation acknowledgements and
a timeout re-derived what the frame boundary already guarantees; safety preserved because the owner
still waits for the old subtree to be gone (one frame) before it closes resources or pops the scope.

AccountSnapshot (freezed)
    phase: AccountPhase
    context: AccountContext?      // null unless ready
    failure: String?              // set only when blocked

AccountGate (StatefulWidget, inside MaterialApp)
    phase   = watchValue(owner.phase)
    context = watchValue(owner.context)

    MATCH phase
        ready    -> IF context != null
                        RETURN KeyedSubtree(key: ValueKey(context.generation), child: subtree)
                    ELSE RETURN Progress()
        quiescing-> RETURN NeutralTransitionPage()      // replaces the old subtree THIS build
        blocked  -> RETURN BlockedPage(failure)          // explicit, not a silent spinner
        signedOut-> RETURN SignedOutShell()
        preparing-> RETURN Progress()

    // Remount by generation is the whole protocol: the old subtree is gone because this
    // build no longer returns it, and the key guarantees no state is reused across accounts.

Owner (Step 16) deactivate()
    _phase.value = quiescing
    AWAIT SchedulerBinding.instance.endOfFrame        // the frame that removed the subtree
    AWAIT live.closeForAccountExit()
    AWAIT connection.stop()
    AWAIT popScope()

// If the app is backgrounded and no frame is produced, endOfFrame still completes when the
// next one is; disposal is never skipped, only delayed.
```

**Done when:**
- Switching A→B: A's subtree `dispose` runs before the scope is popped (assert with a flag written in both).
- The first authenticated build never happens while `phase != ready`.
- B mounts with a new `ValueKey` and does not inherit A's widget state.
- `phase == blocked` shows an explicit page, not an endless progress indicator.

**Tests:** `apps/mobile/test/app/account_gate_test.dart` — (1) `preparing` renders progress and no subtree; (2) A→B disposes A before pop; (3) B gets a distinct key; (4) `blocked` renders the failure text.

**Watch out for:** keying by `accountId` only. Two activations of the *same* account must also remount, otherwise the second one reuses the first one's disposed closures — that is the H15 case.

**Depends on:** Step 16.

---

### Step 18: Make startup retry rebuild what failed

**Phase:** P2
**Audit reference:** §3 [H01] "Authenticated UI is not synchronized with account-scope readiness", Recommended fix (readiness observable, gate before mounting; retry must rebuild); §3 [M24] "Critical lifecycle tests are commented out", Recommended fix (real retry coverage). Evidence `apps/mobile/lib/main.dart:6–23`, `apps/mobile/lib/app.dart:56–114`.
**Why (plain words):** Today the startup error's Retry re-awaits the same failed future, so it can only fail again. Registration has to be re-run, not re-read — and the retry button must not be tappable twice while it runs.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/app/bootstrap.dart` | Create | service | Required | One function: reset registrations if any exist, register root, await ready. |
| `apps/mobile/lib/main.dart` | Modify | config | Required | Call it; show the error app on failure. |
| `apps/mobile/lib/app.dart` | Modify | widget | Required | Error screen's Retry calls the same function. |
| `apps/mobile/test/app/bootstrap_retry_test.dart` | Create | test | Required | Retry rebuilds; a second tap during retry is ignored. |

**Pseudo-code**

```
Simplified from design doc §5.10 because attempt ids and a cleanup ledger tracked native resources
for one failure mode; safety preserved because a retry still disposes what exists, re-registers and
awaits readiness, and concurrent taps still cannot start two bootstraps.

bootstrap() -> Future<void>
    IF get_it.hasAnyRegistration -> AWAIT get_it.reset(dispose: true)   // dispose before re-register
    registerRoot()                                                     // root_registrations.dart
    AWAIT get_it.allReady()

main()
    TRY
        AWAIT bootstrap()
        runApp(MenoApp())
    CATCH error
        runApp(StartupErrorApp(error, onRetry: retry))

retry() -> Future<void>
    IF _running -> RETURN          // one bool; ignores double taps
    _running = true
    TRY   AWAIT bootstrap(); runApp(MenoApp())
    CATCH error -> show the same error screen, updated
    FINALLY _running = false

    // If disposing the database handle itself throws, do not pretend: tell the user to
    // restart the app. A half-closed ObjectBox handle cannot be re-opened in-process.
```

**Done when:**
- The first startup failure shows the error app; tapping Retry registers again and reaches the main app when the second attempt succeeds.
- A second tap while the first retry runs starts nothing (assert one `registerRoot` call).
- Retry disposes a partially registered service before re-registering it.
- A dispose failure shows the restart instruction rather than looping.

**Tests:** `apps/mobile/test/app/bootstrap_retry_test.dart` — (1) fail then succeed on retry; (2) double tap registers once; (3) a throwing `dispose` surfaces the restart message.

**Watch out for:** caching the failed `Future` anywhere (a static, a lazy singleton). Re-awaiting a completed-with-error future is exactly the bug this step removes.

**Depends on:** Steps 8, 16.

---

### Step 19: Refresh the router on identity and keep one pending destination

**Phase:** P2
**Audit reference:** §3 [M19] "Routing discards identity and intended destinations", Recommended fix (preserve validated pending destinations; add platform link registration); §3 [H01] "Authenticated UI is not synchronized with account-scope readiness", part "router refresh observes booleans, not identity"; §3 [M16] "Push payloads, channels, and tap lifecycle disagree", Recommended fix (validated destinations, pending intent, cold/warm handling). Evidence `apps/mobile/lib/_routing/router.dart:364–389`, `apps/mobile/lib/_routing/router_guard.dart`, `apps/mobile/lib/_shared/services/local_notifications_service.dart:22–34,85`.
**Why (plain words):** A notification tap or a cold-start link can arrive before the account is ready, and today it either navigates into a tree that does not exist yet or is dropped. Worse, a tap that belongs to account A can open while B is active. One validated slot that waits for readiness and refuses a foreign recipient fixes both without a queue or a chooser dialog.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/_routing/pending_destination.dart` | Create | model | Required | `PendingDestination` + `PendingDestinations` (one slot) + the path allowlist. |
| `apps/mobile/lib/_routing/router.dart` | Modify | config | Required | Refresh on identity; consume the slot in `redirect`. |
| `apps/mobile/lib/_routing/router_guard.dart` | Modify | config | Required | Gate protected routes on `phase == ready`. |
| `apps/mobile/lib/_shared/services/push_notifications_service.dart` | Modify | service | Required | Hand a validated destination to the slot; never navigate. |
| `apps/mobile/lib/_shared/services/local_notifications_service.dart` | Modify | service | Required | Same contract for a local tap. |
| `apps/mobile/test/_routing/pending_destination_test.dart` | Create | test | Required | Newest wins, waits for readiness, refuses a foreign account. |
| `apps/mobile/test/_routing/router_guard_test.dart` | Modify | test | Required | Redirect consumes once. |

**Pseudo-code**

```
Simplified from design doc §5.9 because a 20-item queue with expiry, a dedupe list, an injected
clock and an account chooser is a product feature, not a routing fix; safety preserved because a
destination is still validated against an allowlist, still waits for readiness, and still cannot
open for an account it does not belong to.

PendingDestination (plain final class)
    path: String                  // already validated
    recipientAccountId: String?   // null = "whoever is active"

PendingDestinations (root singleton, one slot)
    _slot: PendingDestination?

    put(rawPath, recipient) -> bool
        normalized = validate(rawPath)             // allowlist: /broadcast/<id>, /profile/<id>, /chat/<id>
        IF normalized == null -> RETURN false      // unknown or hostile path: dropped, logged by kind only
        _slot = PendingDestination(normalized, recipient)   // newest wins; no queue
        RETURN true

    consumeFor(phase, activeAccountId) -> { path }? | Dropped(reason)
        IF _slot == null -> RETURN null
        IF phase != ready -> RETURN null            // keep it; the router refreshes again when ready
        candidate = _slot
        _slot = null                                // consume-once, even if it is then dropped
        IF candidate.recipientAccountId != null
           AND candidate.recipientAccountId != activeAccountId
            RETURN Dropped('another account')       // caller shows one snackbar, no navigation
        RETURN candidate.path

Router
    refreshListenable = Listenable.merge([owner.phase, onboardingDone])   // built ONCE
    redirect(state)
        IF protected(state) AND owner.phase != ready -> RETURN '/'
        target = destinations.consumeFor(owner.phase, owner.context?.accountId)
        IF target is path -> RETURN target
        IF target is Dropped -> show snackbar after the frame; RETURN null
        RETURN null

Platform links
    DEFERRED: App Links / Universal Links are not registered in this work. When they are,
    they call PendingDestinations.put with the same allowlist — recorded as a decision,
    not an omission.
```

**Done when:**
- A tap that arrives while `phase == preparing` navigates after readiness, once.
- A tap whose recipient is another account shows a snackbar and does not navigate.
- Two taps in a row: only the second destination is opened.
- `refreshListenable` is created once (a debug counter stays at 1 across rebuilds).
- A path outside the allowlist is stored nowhere and navigates nowhere.

**Tests:** `apps/mobile/test/_routing/pending_destination_test.dart` — (1) put/consume returns the path once; (2) a second consume returns null; (3) a foreign recipient is dropped; (4) an unknown path is refused. `apps/mobile/test/_routing/router_guard_test.dart` — (5) redirect before readiness stays home; (6) redirect after readiness lands on the destination exactly once.

**Watch out for:** consuming in `build`. `redirect` can run several times per navigation; the slot must clear on read whether or not the navigation succeeds, or the app will bounce between pages.

**Depends on:** Steps 16, 17.

---

### Step 20: Reduce AuthManager to a UI facade with attempt tickets

**Phase:** P2
**Audit reference:** §3 [C02] "Session-store mutations are neither atomic nor identity-conditional", part "AuthManager must not write credentials separately"; §3 [C03] "Explicit logout can leave a usable session recoverable"; §3 [M02] "Add-account flow retains password-only login mode"; §3 [L02] "Derived observables are recreated by getters", Recommended fix (cache derived observables in fields); design §5.3. Evidence `apps/mobile/lib/features/auth/manager/auth_manager.dart:69–73,182–190,202–205,249–257,318–348`.
**Why (plain words):** `AuthManager` currently saves a response, adopts it, emits a credential stream and lets a second listener adopt again — three writers for one fact, and a slow login can land after the user already switched or logged out. Making it a command facade with a ticket, and moving all writing to the vault and the transition owner, removes the double-adoption and the race.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/auth/manager/auth_manager.dart` | Modify | manager | Required | Submit commands, attempt ticket, form mode, cached projections. |
| `apps/mobile/lib/features/auth/manager/user_manager.dart` | Modify | manager | Optional | Only to delete its dependency on `activeCredential` shapes; the projection rewrite is Step 21. Loses nothing except a duplicate write path. |
| `apps/mobile/test/features/auth/auth_manager_test.dart` | Modify | test | Required | Attempt-race suite. |

**Pseudo-code**

```
AuthManager (root)
    loginCommand / registerCommand / googleCommand / logoutCommand / switchCommand
    formMode : ValueNotifier<LoginFormMode>             // fullEmail | prefillSelectedIdentity
    isAuthenticated / isVerified : cached derived observables created ONCE
    _attemptTicket : int

FUNCTION submitLogin(values)
    REQUIRE form valid AND no transition is running (command restriction; UI hint only)
    ticket = ++_attemptTicket
    response = AWAIT authService.login(values)           // anonymous transport, no interceptor state
    IF ticket != _attemptTicket RETURN cancelled         // a switch/logout/add-account superseded us
    result = AWAIT transitionOwner.adoptCandidate(response, ticket)
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

**Depends on:** Steps 16, 3.

---

### Step 21: Give the current user exactly one presentation source

**Phase:** P2
**Audit reference:** §3 [M12] "Current-user presentation has multiple unsynchronized sources", Recommended fix (one presentation source for current-user name/avatar; synchronize credential metadata explicitly); §3 [L02] "Derived observables are recreated by getters". Evidence `apps/mobile/lib/_shared/widgets/user_avatar_widget.dart:14–20`, `apps/mobile/lib/features/broadcast/pages/home_page.dart:61–63`, `apps/mobile/lib/features/profile/manager/my_profile_manager.dart:82–85`.
**Why (plain words):** The avatar and greeting read the auth-stored user, while edits go to `MyProfileManager`, so a name change updates one screen and not the chrome. Binding the chrome to the ready account's profile presentation — and keeping the stored user for greeting metadata only — makes edits visible everywhere without turning the token store into a profile store.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/app/widgets/current_user_avatar.dart` | Create | widget | Required | Composition connector: watches the ready account's profile, passes plain data down. |
| `apps/mobile/lib/_shared/widgets/user_avatar_widget.dart` | Modify | widget | Required | Becomes presentational: takes an image URL/name, resolves nothing. |
| `apps/mobile/lib/features/broadcast/pages/home_page.dart` | Modify | widget | Required | Greeting/avatar use the connector. |
| `apps/mobile/lib/features/auth/manager/user_manager.dart` | Modify | manager | Required | Read-only projection: ready-account presentation, else remembered identity for signed-out greeting. |
| `apps/mobile/test/features/profile/current_user_presentation_test.dart` | Create | test | Required | Single-source assertions. |

**Pseudo-code**

```
UserManager (root, read-only)
    readyProfile  : ValueListenable<CurrentUserPresentation?>   // bound by composition when a scope is ready
    signedOutGreeting : ValueListenable<RememberedIdentity?>     // metadata only
    FUNCTION bind(context, MyProfileManager profile)
        REQUIRE context.generation == live generation
        _binding = profile.presentation.listen(...)              // cancel on unbind
    FUNCTION unbind()
        CANCEL binding BEFORE scope pop
    // rejects a binding whose generation differs from the published ready snapshot

CurrentUserAvatar (WatchingWidget)
    presentation = watchValue((UserManager u) => u.readyProfile)
    RETURN UserAvatarWidget(name: presentation?.name, imageUrl: presentation?.avatarUrl)
    // never reads tokens, never resolves a manager itself

UserAvatarWidget (presentational, no DI)
    REQUIRED name; OPTIONAL imageUrl
    RENDERS fallback initial when imageUrl is null
```

**Done when:**
- Editing the display name updates the chrome without a restart.
- `grep -rn "UserCredential\|accessToken" apps/mobile/lib/_shared/widgets/user_avatar_widget.dart` finds nothing.
- Signed out shows a remembered greeting, never a "logged in" chrome.
- Unbinding happens before scope pop (test asserts order).

**Tests:** `apps/mobile/test/features/profile/current_user_presentation_test.dart` — (a) profile edit reaches chrome, (b) unbind before pop, (c) signed-out greeting uses metadata, (d) a stale-generation binding is rejected.

**Watch out for:** registering `CurrentUserPresentation` as DI state in the account scope *and* caching it in `UserManager`. One source: the profile manager; `UserManager` only forwards.

**Depends on:** Steps 20, 16.

---

### Step 22: Send verification codes only when the user taps

**Phase:** P2
**Audit reference:** §3 [M01] "Email resend availability drives an automatic resend loop", Recommended fix (send from the tap command; derive availability from cooldown and running state; keep one error handler); §3 [L02] "Derived observables are recreated by getters", Recommended fix (retain the derived availability). Evidence `apps/mobile/lib/features/auth/pages/email_verification_page.dart:42–56,64–67,92–97,182–184`, `apps/mobile/lib/features/auth/manager/email_verification_manager.dart:36–69`.
**Why (plain words):** `canResend` changing starts a cooldown and sends an OTP, so the countdown finishing sends *another* code with no user action. Availability is a derived value, not a trigger; making the tap the only sender and caching the derived notifier fixes the loop and the duplicate error handlers at once.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/auth/manager/email_verification_manager.dart` | Modify | manager | Required | Explicit `resend()`; cached `canResend`; injected clock. |
| `apps/mobile/lib/features/auth/pages/email_verification_page.dart` | Modify | widget | Required | Button calls `resend()`; one error handler. |
| `apps/mobile/test/features/auth/email_verification_manager_test.dart` | Create | test | Required | Send-count assertions. |

**Pseudo-code**

```
EmailVerificationManager (widget-owned, created with createOnce)
    _cooldownDeadline : DateTime?
    _ticker : Timer?                     // updates remaining time only
    resendCommand : Command.createAsyncNoParamNoResult(resend)
    late final canResend = <derived ONCE>(deadline, tick, running, pageAlive)
        // built in the constructor and retained; NEVER inside build()

FUNCTION resend()
    REQUIRE clock.nowUtc() >= deadline AND owner still the captured account generation
    AWAIT authService.resendVerification(capturedEmail)
    CHECK ticket; ON success SET deadline = now + documentedCooldown
    ON documented throttling SET deadline from Retry-After (never shorter)
    ON uncertain outcome SET conservative cooldown + "try again later"
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

**Tests:** `apps/mobile/test/features/auth/email_verification_manager_test.dart` — (a) availability becoming true sends nothing, (b) one tap sends once, (c) `Retry-After` lengthens the cooldown, (d) one error handler fires.

**Watch out for:** `registerHandler` on `canResend` with `executeImmediately: true`. That is the automatic send in a new costume.

**Depends on:** Step 20.

---

### Step 23: Inject immutable account context into account-scoped managers

**Phase:** P2
**Audit reference:** §3 [M14] "Account-scoped managers redundantly track global account selection", Recommended fix (inject immutable account context; initialize once; use generation/cancellation for pending work); §3 [H15] "Async operations continue after their owner is disposed". Evidence `apps/mobile/lib/features/profile/manager/my_profile_manager.dart:70–74,147–160`, `apps/mobile/lib/features/settings/manager/settings_manager.dart:208–215,347–362`, `apps/mobile/lib/locator.dart:250–281`.
**Why (plain words):** Managers that already live in the account scope subscribe to the global account selector to handle a lifecycle their scope owns, so a disposed manager can begin loading the *next* account's data. Deleting those subscriptions and injecting the context removes the duplicate ownership; the existing disposal guards stay.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/profile/manager/my_profile_manager.dart` | Modify | manager | Required | Take `AccountContext`; delete `_onAccountChanged` and the `UserManager` dependency. |
| `apps/mobile/lib/features/settings/manager/settings_manager.dart` | Modify | manager | Required | Same deletion; keep legitimate account-data subscriptions. |
| `apps/mobile/lib/app/composition/account_registrations.dart` | Modify | config | Required | Pass the context; await local initialisation before publishing ready. |
| `apps/mobile/test/features/profile/my_profile_manager_test.dart` | Modify | test | Required | Rewrite: no global-following expectations. |

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
    REQUEST safe remembered-metadata update (Step 20)              // never tokens

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

**Watch out for:** removing the *disposal* guards along with the selector. Those guards are the pattern the audit §5 item 7 calls out as good.

**Depends on:** Steps 16, 21.

---

## 10. Phase R — Realtime transport

**Findings finished here:** [C04], [H06] (adapter half), [M10], [M03] (handshake half).

**What you will have when this phase is done:** a raw-WebSocket client that speaks the backend's JSON frames, sends a heartbeat, reconnects with bounded backoff, decodes events strictly, and installs/detaches handlers idempotently.

**How to know it's safe to ship:** the framing tests pass against recorded server frames; the adapter test proves attach/detach is idempotent; no Socket.IO dependency remains.

**Order:** Step 24 only. It needs only P1, so it goes here: audit §8 puts transport repair before the live teardown, and the live work should not be built on a client that cannot talk to the server.

**What we are deliberately NOT doing in R:** no Socket.IO compatibility shim, no acknowledgement emulation, no replay service, no `IWebSocket` interface (there is one implementation: `dart:io`'s `WebSocket`).


### Step 24: Replace the Socket.IO client with a raw-WebSocket client

**Phase:** R
**Audit reference:** §3 [C04] "Mobile and backend use incompatible realtime protocols", Recommended fix (replace the Socket.IO implementation with a raw-WebSocket adapter matching `/ws`; implement heartbeat, connection authentication, bounded reconnect and reconciliation; do not invent Socket.IO acknowledgements); evidence `apps/mobile/lib/_shared/services/socket_client.dart:69–74,153–165`, `apps/api/src/routes/ws.rs:241–271,492–503`. Also: §3 [H06] "No production path starts realtime delivery", Recommended fix (an explicit account connection owner initializes handlers before connecting); evidence `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart:72–129`. Also: §3 [M10] "Invalidation parsing and event enrichment are inconsistent", Recommended fix (explicit RFC3339 typed payload; guarded decoding; keep end signals distinct from rich REST summaries and enrich/refetch); evidence `apps/mobile/lib/features/broadcast/model/dtos.dart:113–119`, `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart:120–126,167–189`, `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:202–205`, `apps/api/src/infrastructure/ws/dto.rs:169–173,234–238`.
**Why (plain words):** (**A**) The app speaks Socket.IO framing and handshake while the backend accepts raw WebSocket JSON frames, so realtime cannot work at all. Selecting Socket.IO's "websocket transport" does not change the framing. A direct `WebSocket` client with an explicit handshake and heartbeat is the only compatible implementation. (**B**) The adapter only installs handlers inside `listen()`, and subscribing to its stream does not call it — so even a working transport delivered nothing. Making install/detach explicit, idempotent and composition-callable means the connection owner decides when delivery is active. (**C**) The backend serialises a bare `OffsetDateTime` (which is not RFC3339 text), invalidation payloads bypass the adapter's guarded decode, and an end event carrying only an id and reason is turned into a full card with empty fields. Explicit typed payloads, one guarded decode, and a refetch for anything richer fix all three.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --- | --- | --- | --- | --- |
| `apps/mobile/lib/_shared/services/socket_client.dart` | Modify | service | Required | Rewritten as a raw-WebSocket client; same class name so feature adapters keep working. |
| `apps/mobile/lib/_shared/model/socket.dart` | Modify | freezed model | Required | Frame envelope + connection phase types. |
| `apps/mobile/pubspec.yaml` | Modify | config | Required | Remove `socket_io_client`; no replacement package (uses `dart:io`). |
| `apps/mobile/test/_shared/services/socket_client_test.dart` | Create | test | Required | Frame/heartbeat/reconnect tests against a local server. |
| `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart` | Modify | service | Required | `install()`/`detach()` replace `listen()`-implies-install; guarded decode; no side-effectful getters. |
| `apps/mobile/lib/features/broadcast/services/broadcast_events.dart` | Create, then Modify | freezed model | Required | Typed event union the adapter emits. |
| `apps/mobile/test/features/broadcast/broadcast_socket_service_test.dart` | Create | test | Required | Install/detach matrix. |
| `apps/mobile/lib/features/broadcast/model/dtos.dart` | Modify | freezed model | Required | Explicit `@JsonKey(fromJson:)` RFC3339 parsing for event timestamps. |
| `apps/mobile/test/features/broadcast/broadcast_event_decode_test.dart` | Create | test | Required | Payload fixtures from the backend. |

**Pseudo-code**

```
// ===== Part A — Replace the Socket.IO client with a raw-WebSocket client =====
SocketClient (transport only; no account logic)
    new(uri, credentialSupplier, logger, clock)
    WebSocket? _socket          // dart:io's WebSocket; no interface for one implementation
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

```
// ===== Part B — Make adapter install/detach explicit and idempotent =====
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

```
// ===== Part C — Decode event payloads strictly and keep "ended" distinct from a summary =====
Timestamps
    REQUIRE a string timestamp matching RFC3339; parse with DateTime.parse
    IF the value arrives as anything else (array/object/int) -> decode FAILS        // visible, not silent

BroadcastEvent (sealed)
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
- **[A]** Against a locally running `apps/api`, the client completes the handshake and receives a known event.
- **[A]** A heartbeat miss triggers exactly one reconnect attempt chain, bounded, with visible backoff.
- **[A]** A close code the backend uses for auth rejection is reported as terminal and not retried.
- **[A]** `socket_io_client` is no longer a dependency (`grep -n socket_io apps/mobile/pubspec.yaml`).
- **[B]** `install()` twice creates one set of handles; `detach()` twice cancels once.
- **[B]** A malformed frame emits nothing and does not throw.
- **[B]** The connection owner calls `install()` before `socket.start()` (asserted by call order).
- **[B]** Subscribing to the stream without `install()` receives nothing (proving install is required and explicit).
- **[C]** A backend-shaped payload with a real RFC3339 string decodes; an `OffsetDateTime`-as-array payload fails loudly in a test fixture.
- **[C]** An ended signal never produces a card with empty title/creator.
- **[C]** Invalidation decoding shares the adapter's guarded path (no second parser).
- **[C]** Malformed events are dropped with one sanitised log line.

**Tests:** **[A]** `apps/mobile/test/_shared/services/socket_client_test.dart` — (a) frame encode/decode round trip, (b) heartbeat miss reconnects, (c) terminal close code stops retrying, (d) `send` before open throws typed error. **[B]** `apps/mobile/test/features/broadcast/broadcast_socket_service_test.dart` — (a) idempotent install, (b) detach cancels all handles, (c) malformed frame dropped, (d) install-before-start ordering. **[C]** `apps/mobile/test/features/broadcast/broadcast_event_decode_test.dart` — (a) all event fixtures decode, (b) non-RFC3339 timestamp rejected, (c) ended signal triggers a refetch instead of a synthetic card, (d) guarded decode of an unknown event emits nothing.

**Watch out for:** **[A]** keeping the Socket.IO `path`/`transports` semantics in the settings. The backend has one endpoint with one framing. **[B]** cancelling handles across an `await` while a new install overwrites the list. Copy to locals first (the same bug shape as M09). **[C]** `DateTime.parse` on a numeric or list value with a `try/catch` that silently defaults to `now`. Defaulting to "now" re-creates the H10 expiry problem in the feed.

**Depends on:** Step 4.

---

## 11. Phase P3 — Account-owned `ActiveBroadcastManager`

**Findings finished here:** [H02], [H03], [H04], [H08], [M21], [L01], [L03] (timer), [L05] (duplicate live paths).

**What you will have when this phase is done:**
- One account-scoped `ActiveBroadcastManager` owning at most one `ActiveBroadcastSession` — no `live:<id>` scope, no registration-as-state.
- One `LiveSnapshot` (phase + broadcast + failure + end reason) read by the banner, the live page and the ended page.
- A single idempotent close in `finally`, with the end summary kept outside the disposed resources.
- Creator authority, publish capability and end authority as three separate facts.
- One timer, created with the session, monotonic, reconciled with the server.

**How to know it's safe to ship:** opening then logging out leaves no transport, timer or live registration; a failed REST end still closes locally and still shows the ended page; a cohost sees "leave", never "end"; navigating away from the live page keeps audio playing.

**Order:** 25 → 26 → 27. Step 27 must land in one change: it removes registrations while callers still use them.

**What we are deliberately NOT doing in P3:** no per-session `GetIt` container, no new session framework, no `StreamManager` pass-through, no separate timer step (the timer belongs to the session), no roster pagination (Phase B).


### Step 25: Create the session's resources with the session and close them once

**Phase:** P3
**Audit reference:** §1.4 "State machine" (one authoritative phase, capabilities separate); §3 [H03] "Session disposal can access an uninitialized timer", Recommended fix (create the timer with the session, never invoke UI commands from disposal); §3 [H04] "Failed end/leave skips session closure; ended UI depends on disposed resources", Recommended fix (one idempotent close in `finally`, retain the ending result outside the resources); §3 [H08] "Publishing permission is mistaken for creator authority", Recommended fix (separate creator identity, publish capability and end authority); §3 [L01] "Rebuild granularity is too broad for a one-second clock"; §3 [L03] "Timer semantics and retry timing are inaccurate", Recommended fix (monotonic elapsed anchor with server reconciliation). Evidence `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart:127–130,203–205`, `apps/mobile/lib/features/broadcast/model/entities.dart:207–208`, `apps/mobile/lib/features/broadcast/widgets/broadcast_tab_widget.dart:12–26`.
**Why (plain words):** The session's timer is created lazily and disposed through a UI command, so a fast close touches an uninitialized field; the same object mixes four booleans that disagree; and `isHost` means "may publish", which hands a cohost the creator-only end call. One phase, three explicit capabilities, resources built in the constructor and one close path fix all of it.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/model/live_snapshot.dart` | Create | freezed model | Required | `LivePhase` (enum), `LiveFailure`, `LiveSnapshot` (freezed), `LiveCapabilities`. |
| `apps/mobile/lib/features/broadcast/model/entities.dart` | Modify | freezed model | Required | Replace role booleans with capability values; keep `EndReason`. |
| `apps/mobile/lib/features/broadcast/session/active_broadcast_session.dart` | Create | service | Required | Owns transport, roster, media subscriptions, timer. |
| `apps/mobile/lib/features/broadcast/manager/broadcast_timer_manager.dart` | Modify | manager | Required | Monotonic anchor + server reconciliation; no commands in `dispose`. |
| `apps/mobile/lib/features/broadcast/widgets/broadcast_control_buttons.dart` | Modify | widget | Required | Gate mic / stop / leave independently. |
| `apps/mobile/lib/features/broadcast/widgets/broadcast_tab_widget.dart` | Modify | widget | Required | Only the elapsed label watches the ticker. |
| `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart` | Leave alone | manager | Required | Not deleted here: callers still use it until Step 27 removes them together (review D-6). |
| `apps/mobile/test/features/broadcast/active_broadcast_session_test.dart` | Create | test | Required | Resources exist from construction; one close; end summary survives. |
| `apps/mobile/test/features/broadcast/broadcast_timer_manager_test.dart` | Modify | test | Required | Monotonic elapsed, server reconciliation, one-second rebuild scope. |

**Pseudo-code**

```
Simplified from design doc §1.4's union sketch because live phases carry no payload; safety
preserved because the phase is still the single authority and the three capabilities are still
separate facts.

enum LivePhase { idle, connecting, live, reconnecting, ending, ended, failed }

LiveCapabilities (plain final class, computed once from broadcast + my account id)
    isCreator      // broadcast.creatorId == myAccountId
    canPublish     // host or cohost with a media grant
    canEnd         // isCreator only
    canModerate    // creator or cohost
    // never derive canEnd from canPublish: that is H08

LiveSnapshot (freezed)
    phase: LivePhase
    broadcast: Broadcast?
    failure: LiveFailure?          // connecting/ended reason surfaced to the UI
    elapsed: Duration              // convenience copy for the page, not the source of truth

ActiveBroadcastSession
    CONSTRUCTOR(broadcast, transport, roster, media, clock, myAccountId)
        timer   = BroadcastTimerManager(clock, broadcast.startedAt)   // created HERE
        subs    = []
        caps    = LiveCapabilities.from(broadcast, myAccountId)
        // nothing is created later, so nothing can be missing when close runs

    start()
        phase = connecting
        AWAIT transport.connect()
        subs  = AWAIT roster.bind() + media.bind()
        phase = live

    closeOnce(reason) -> Future<LiveEndSummary>
        IF _closing != null -> RETURN _closing           // idempotent: second caller awaits the first
        _closing = _close(reason)
        RETURN _closing

    _close(reason) async
        phase = ending
        TRY
            FOR sub IN subs -> AWAIT sub.cancel()          // local resources first
            timer.dispose()                                // no command, no context
            AWAIT transport.leaveOrEnd(reason)             // server call last, result ignored on failure
        FINALLY
            AWAIT transport.close()                        // always closes, even when the server call threw
            _summary = LiveEndSummary(broadcast, reason, elapsed: timer.elapsed)
            phase = ended
        RETURN _summary
        // _summary lives on the session object, which the manager keeps until it replaces it,
        // so the ended page can read it after the transport is gone

BroadcastTimerManager
    _anchorMono, _anchorElapsed
    elapsed = _anchorElapsed + (clock.monotonic() - _anchorMono)
    reconcile(serverElapsed) -> re-anchor without jumping backwards
    // one ValueNotifier<Duration>, ticked by a single periodic timer

BroadcastTabWidget
    ValueListenableBuilder(timer.elapsed) around the label ONLY
    // the artwork, title and buttons do not rebuild once per second
```

**Done when:**
- Constructing a session and calling `closeOnce` immediately does not throw and does not touch a null timer.
- `closeOnce` called twice returns the same summary and performs one transport close.
- A transport `leave` that throws still results in `phase == ended` and one summary.
- A cohost's capabilities read `canPublish: true, canEnd: false`.
- Only the elapsed label rebuilds on a tick (assert with a `build` counter in the test widget).

**Tests:** `apps/mobile/test/features/broadcast/active_broadcast_session_test.dart` — (1) close-before-start is safe; (2) double close is idempotent; (3) a throwing leave still ends and summarises; (4) capabilities separate publish from end. `apps/mobile/test/features/broadcast/broadcast_timer_manager_test.dart` — (5) elapsed is monotonic across a clock jump backwards; (6) reconciliation never decreases the display.

**Watch out for:** disposing the timer through a `command_it` command. Disposal must not depend on UI machinery, and `command_it` is UI machinery (M21/H03).

**Depends on:** Step 23.

---

### Step 26: Build ActiveBroadcastManager as the single live owner

**Phase:** P3
**Audit reference:** §1.3 "Account manager with nullable active session" (the recommended option); §1.4 "Recommended approach", "Ownership and lifecycle"; §3 [H02] "Live transitions can outlive account teardown or supersede newer sessions", Recommended fix (one account-owned transition owner, generation checks after awaits, quiesce before disposing); §3 [M21] "Scope/connection orchestration is over-commanded", Recommended fix (plain async entry/close; commands only at the UI boundary). Evidence `apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart`, `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart`.
**Why (plain words):** Today the *existence of a DI registration* is used as "am I live?", so entry, replacement and teardown are spread across a scope, a page and a manager that can each start one. One manager that owns at most one session, runs its operations in order and closes idempotently makes "one live broadcast per account" a fact of the code rather than a convention.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/manager/active_broadcast_manager.dart` | Create | manager | Required | The single live owner. |
| `apps/mobile/lib/features/broadcast/services/live_session_store.dart` | Modify | service | Required | Persist only broadcast id + intended phase for recovery. |
| `apps/mobile/test/features/broadcast/active_broadcast_manager_test.dart` | Create | test | Required | Ordering, idempotence, recovery, exit. |

**Pseudo-code**

```
Simplified from design doc §1.4 because a generation check per await is enough for one owner that
serializes every entry; safety preserved because operations still cannot overlap, close still runs
in finally, and exit still awaits the close. Two defects found in review are fixed here: recover()
must not re-enter through the public queued method, and the session must be stored before start().

ActiveBroadcastManager (account-scoped singleton)
    _queue: FIFO                                  // every public operation runs here
    _session: ActiveBroadcastSession?
    _snapshot: ValueNotifier<LiveSnapshot>(idle)
    _lastSummary: LiveEndSummary?                 // survives session disposal

    enter(broadcastId)      -> ENQUEUE _enter(id)
    replace(broadcastId)    -> ENQUEUE { AWAIT _closeIfAny(Replaced); _enter(id) }
    recover()               -> ENQUEUE { intent = AWAIT store.read()
                                         IF intent == null -> RETURN
                                         AWAIT _enter(intent.broadcastId, recovered: true) }
    leaveOrEnd(reason)      -> ENQUEUE { IF _session == null -> RETURN   // null-guarded (D-5)
                                         TRY AWAIT _session.leaveOrEnd(reason)
                                         CATCH log-and-continue        // H04: local close still happens
                                         FINALLY _lastSummary = AWAIT _session.close(reason)
                                                 _session = null; _publish(ended, summary) }
    closeForAccountExit()   -> ENQUEUE { AWAIT _closeIfAny(AccountExit) }   // awaited, idempotent

    _enter(id, {recovered = false})
        AWAIT _closeIfAny(Superseded)
        broadcast = AWAIT http.fetch(id)
        session = ActiveBroadcastSession(broadcast, transport, roster, media, clock, myAccountId)
        _session = session                        // STORE BEFORE START (D-5)
        _publish(connecting, broadcast)
        TRY
            AWAIT session.start()
            AWAIT store.write(id, live)           // recovery intent, non-secret
            _publish(live, broadcast)
        CATCH error
            AWAIT session.closeOnce(Failed(error))   // closes the timer and transport that exist
            _session = null
            _publish(failed, broadcast, error)
            RETHROW

    _closeIfAny(reason)
        IF _session == null -> RETURN
        _lastSummary = AWAIT _session.closeOnce(reason)
        _session = null
        AWAIT store.clear()
        _publish(ended, _lastSummary.broadcast, summary: _lastSummary)

// Legal transitions: idle->connecting, connecting->live|failed, live->reconnecting->live,
// live->ending->ended, any->ending->ended. Everything else is a bug and is asserted in the test.
// There is no second entry while one is in flight: the queue is the lock.
```

**Done when:**
- `enter` twice in a row: the first session is closed before the second connects (assert ordering with a log list).
- `recover()` inside the queue does not deadlock (the test times out on the old code that re-enters `enter`).
- `start()` throwing leaves no transport, no timer and `phase == failed`.
- `leaveOrEnd` with no session is a no-op rather than a null dereference.
- `closeForAccountExit()` returns only after the transport is closed.

**Tests:** `apps/mobile/test/features/broadcast/active_broadcast_manager_test.dart` — (1) enter/enter closes then opens; (2) recover does not deadlock; (3) a throwing start cleans up; (4) leave with no session is safe; (5) exit close is awaited and idempotent.

**Watch out for:** calling `enter()` from `recover()`. The queue is FIFO, so the nested call waits for the job that is waiting for it: a permanent hang (the review's D-4).

**Depends on:** Step 25.

---

### Step 27: Switch the app to the manager and delete the live tier

**Phase:** P3
**Audit reference:** §1.4 "Migration steps" items 6–7 and "DI registration map" (register only `ActiveBroadcastManager`; the session is an ordinary owned object; remove `LiveScopeManager`, live scope names, scope polling and readiness signalling); §1.2 "Broadcast scope" failures table (registration existence used as connection state; the transition page polls and self-disposes) and the "early-registration/banner assessment" (the banner already avoids the historical first-build bug — keep that); §3 [H04] "Failed end/leave skips session closure; ended UI depends on disposed resources", Recommended fix (ended route loads by its broadcast id); §3 [M21] "Scope/connection orchestration is over-commanded"; §3 [L05] "Dead paths, fixtures, and documentation obscure the reference implementation"; §3 [M19] "Routing discards identity and intended destinations", part "live entry consults session phase". Evidence `apps/mobile/lib/app/composition/account_registrations.dart`, `apps/mobile/lib/locator.dart`, `apps/mobile/lib/features/broadcast/pages/live_session_init_page.dart`, `apps/mobile/lib/features/broadcast/manager/stream_manager.dart`, `apps/mobile/lib/features/broadcast/pages/pre_stream_modal.dart:16–19`.
**Why (plain words):** The manager from Step 26 is useless while five other paths can still start, stop or observe a live session through a DI scope. This step moves every caller onto the snapshot, then deletes the scope, the polling page, the duplicate tab, the pass-through stream manager and the old session manager. It must be one change: removing a registration that callers still look up does not compile.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/app/composition/account_registrations.dart` | Modify | config | Required | **[A]** Register `ActiveBroadcastManager` and the session's collaborators; no live scope. |
| `apps/mobile/lib/locator.dart` | Modify | config | Required | **[A]** Remove live-scope creation and readiness signalling. |
| `apps/mobile/lib/app/account_transition_owner.dart` | Modify | service | Required | **[D]** Call `manager.closeForAccountExit()` instead of the Step 15 fence. |
| `apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart` | Delete | manager | Required | **[D]** The tier being removed. |
| `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart` | Delete | manager | Required | **[D]** Callers moved in [B]; deletion happens here, not in Step 25. |
| `apps/mobile/lib/features/broadcast/manager/stream_manager.dart` | Delete | manager | Required | **[C]** Its only job was forwarding `join()`; callers use the manager. |
| `apps/mobile/lib/features/broadcast/widgets/live_broadcast_tab.dart` | Delete | widget | Required | **[D]** Duplicate live entry path. |
| `apps/mobile/lib/features/broadcast/pages/live_session_init_page.dart` | Move | widget | Required | **[B]** Into `features/broadcast/pages/`; becomes a thin phase switch. |
| `apps/mobile/lib/features/broadcast/layouts/live_session_shell.dart` | Move | widget | Required | **[B]** Into `features/broadcast/layouts/`. |
| `apps/mobile/lib/features/broadcast/pages/live_broadcast_page.dart` | Create | widget | Required | **[B]** Watches the snapshot; no polling, no self-dispose. |
| `apps/mobile/lib/features/broadcast/widgets/live_session_banner.dart` | Modify | widget | Required | **[B]** Reads phase + summary from the manager. |
| `apps/mobile/lib/app/layouts/authenticated_shell.dart` | Create | widget | Required | **[B]** Hosts the banner inside authenticated chrome. |
| `apps/mobile/lib/_shared/layouts/root_shell.dart` | Move | widget | Required | **[B]** Into `app/layouts/` (finishes the M22 move for this file). |
| `apps/mobile/lib/features/broadcast/pages/ended_broadcast_page.dart` | Modify | widget | Required | **[B]** Loads by id from the route; works with no session. |
| `apps/mobile/lib/features/broadcast/pages/pre_stream_modal.dart` | Modify | widget | Required | **[B]** Entry consults `snapshot.phase`, not a registration lookup (M19). |
| `apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart` | Modify | manager | Required | **[C]** Owns its state; entry goes through the manager. |
| `apps/mobile/lib/features/broadcast/pages/broadcast_editor_page.dart` | Modify | widget | Required | **[C]** Editor manager becomes widget-owned (`createOnce`). |
| `apps/mobile/test/locator_test.dart` | Modify | test | Required | **[A]** Exactly two scopes exist; no `live:` name. |
| `apps/mobile/test/features/broadcast/live_phase_routing_test.dart` | Create | test | Required | **[B]** Surfaces follow the phase. |
| `apps/mobile/test/features/broadcast/ended_page_test.dart` | Create | test | Required | **[B]** Ended page with no session. |
| `apps/mobile/test/features/broadcast/live_scope_manager_test.dart` | Delete | test | Required | **[D]** Its subject is gone. |
| `apps/mobile/test/features/broadcast/stream_manager_test.dart` | Delete | test | Required | **[C]** Its subject is gone; the manager test already covers the call. |

**Pseudo-code**

```
[B] Every live surface reads the same snapshot
    phase = watchValue(manager.snapshot).phase
    MATCH phase
        idle        -> leave the page / show the editor
        connecting  -> spinner with a cancel that calls leaveOrEnd
        live        -> room + controls, gated by capabilities (Step 25)
        reconnecting-> keep the room visible, show the retry state (Step 50 owns the policy)
        ending      -> neutral, controls disabled
        ended       -> navigate to /broadcast/<id>/ended, which loads by ID
        failed      -> error page with "try again" -> manager.enter(id)
    // no page calls get_it<...>(instanceName: 'live:$id'); nothing polls; nothing self-disposes

[B] pre_stream_modal
    IF manager.snapshot.phase != idle -> navigate to the live page instead of starting another
    // previously checked whether a live registration existed

[C] Editor
    BroadcastEditorManager is created by the editor page with createOnce
    its "go live" step calls manager.enter(broadcastId) and then awaits the snapshot
    StreamManager removed: callers use ActiveBroadcastManager.enter / leaveOrEnd

[D] Transition owner
    deactivate(): AWAIT manager.closeForAccountExit()   // replaces liveScope.closeForAccountExit()
    // Step 26 guarantees this is queued, idempotent and awaited

[D] Deletions, in this order inside one change
    1. move the pages onto the snapshot
    2. point the editor and the modal at the manager
    3. delete StreamManager, LiveSessionManager, LiveScopeManager, LiveBroadcastTab
    4. remove the live scope registration and its readiness signalling
    5. delete the two orphaned tests
```

**Done when:**
- **[A]** `locator_test` proves exactly two application scopes, and no registration name starts with `live:`.
- **[B]** Opening a broadcast and navigating away keeps audio playing; the page can be popped without closing the session.
- **[B]** The ended page renders from a route id with no session and no DI lookup.
- **[B]** `pre_stream_modal` refuses to start a second broadcast while any non-idle phase exists.
- **[C]** The editor still works when opened twice in a row (its manager is widget-owned and disposed with the page).
- **[D]** `grep -r "LiveScopeManager\|StreamManager\|live:" apps/mobile/lib` returns nothing.
- **[D]** Logging out while live closes the session before the scope pops (the Step 16 test still passes with the new call).

**Tests:** `apps/mobile/test/locator_test.dart` — (1) two scopes only; (2) no `live:` registrations. `apps/mobile/test/features/broadcast/live_phase_routing_test.dart` — (3) each phase renders its surface; (4) `ended` navigates to the id route. `apps/mobile/test/features/broadcast/ended_page_test.dart` — (5) renders with an empty DI scope.

**Watch out for:** deleting the registrations first "to see what breaks". The compiler will list the callers, which is useful, but do not commit an intermediate state where they are removed and the pages still look them up: that is a broken build, not a step.

**Depends on:** Step 26.

---

## 12. Phase B — Broadcast and backend contracts

**Findings finished here:** [H05], [H16], [H17], [H18], [H19], [H20].

**What you will have when this phase is done:** end and cohost-removal contracts that match on both sides, a roster contract with a sane audience policy, atomic backend cohost revocation, retry-safe transitions, and one editor operation that cannot report success without entering.

**How to know it's safe to ship:** the shared contract fixture passes on both sides; a cohost removal leaves no role, presence or media grant; retrying a start returns the same broadcast; the editor's retry resumes only the incomplete phase.

**Order:** 28 → 29 → 30 → 31 → 32 → 33 → 34. Step 30 (roster audience) is a backend prerequisite for 31. Step 34 needs 26, 27 and 33.

**What we are deliberately NOT doing in B:** no per-token event fan-out to every listener (hosts get deltas, listeners poll), no backend idempotency rewrite (the middleware already stores outcomes; only the client was missing keys), no hidden "retried" media revoke.


### Step 28: Fix the end and cohost-removal contracts on the client

**Phase:** B
**Audit reference:** §3 [H05] "Broadcast end and cohost-removal contracts are wrong", Recommended fix (align end DTO/enrichment; make cohost removal return void and send the actual JSON body; add backend-produced fixtures); evidence `apps/mobile/lib/features/broadcast/model/dtos.dart:124–136`, `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart:181–186,244–255`, `apps/api/src/modules/broadcast/dto.rs:185–193,537–557`.
**Why (plain words):** The client requires `creatorName` in the end response, which the backend does not send, so a successful end becomes a client error; and cohost removal sends a query parameter instead of the JSON body `removeFromRoom` while expecting a DTO from an endpoint that returns no data. Both turn success into failure.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/model/dtos.dart` | Modify | freezed model | Required | End response matches the backend's fields exactly. |
| `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart` | Modify | service | Required | `removeCohost` returns `Future<void>` and sends a JSON body. |
| `apps/mobile/test/features/broadcast/broadcast_contract_test.dart` | Create | test | Required | Fixtures captured from the backend. |
| `apps/mobile/test/features/broadcast/broadcast_dto_test.dart` | Modify | test | Required | Update end-response expectations. |

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

FUNCTION removeCohost(broadcastId, cohostId, removeFromRoom) -> void   // REQUIRED, no default
    AWAIT delete('/broadcasts/$id/cohosts/$cohostId', body: { removeFromRoom: removeFromRoom })
    // parse NOTHING; the backend returns no body
    // default true: leaving them in a room they have no rights in is the failure to avoid
```

**Done when:**
- An end response without `creatorName` decodes and shows a summary.
- `removeCohost` requires `removeFromRoom` and sends it in the body; the modal passes the user's explicit choice, so nothing in code defaults "keep listening" to eviction.
- A 204/empty body from cohost removal does not throw.
- Contract fixtures come from the backend's own serialization (see Step 29).

**Tests:** `apps/mobile/test/features/broadcast/broadcast_contract_test.dart` — (a) end fixture decodes, (b) empty cohost body accepted, (c) request body shape asserted, (d) missing name renders a fallback.

**Watch out for:** making `creatorName` optional and *still* sending a `?removeFromRoom=` query. Both halves of the contract must match.

**Depends on:** Step 25.

---

### Step 29: Make the backend end/cohost contract explicit and fixture-tested

**Phase:** B
**Audit reference:** §3 [H05] "Broadcast end and cohost-removal contracts are wrong" (backend half); evidence `apps/api/src/modules/broadcast/dto.rs:185–193,537–557`, `apps/api/src/modules/broadcast/handlers.rs:327–340`.
**Why (plain words):** The client cannot be correct if the server's contract drifts. This step pins the response shapes with Rust serialization tests and makes the cohost-removal body explicit, so a fixture can be generated from the server and checked into the mobile test suite.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/api/src/modules/broadcast/dto.rs` | Modify | Rust module | Required | Document the contract; keep `EndBroadcastResponse` fields as-is (they are sufficient). |
| `apps/api/src/modules/broadcast/handlers.rs` | Modify | Rust module | Required | Cohost removal returns `204`/empty explicitly. |
| `apps/api/tests/broadcast_contract.rs` | Create | test | Required | Serialization shape tests against the checked-in fixture. |
| `apps/api/tests/fixtures/broadcast-end.json` | Create | test | Required | Checked in once by hand; both suites assert against it. |

**Pseudo-code**

```
EndBroadcastResponse
    KEEP existing fields (broadcastId, broadcastTitle, broadcastImageUrl, creatorId,
                          endedReason, endedAt, durationSecs, totalParticipants, recordingEnabled)
    DO NOT add creatorName; display enrichment belongs to the client's profile source
    ADD a doc line: "consumed by apps/mobile broadcast_contract_test fixtures"

RemoveCohost handler
    READ body RemoveCohostRequest { removeFromRoom: bool }   // required in the body
    IGNORE a query parameter of the same name: the body is the contract (no 400 rule to maintain)
    RETURN 204 No Content on success

Test (apps/api/tests/broadcast_contract.rs)
    SERIALIZE a representative EndBroadcastResponse; assert the exact JSON keys
    ASSERT cohost removal returns no body
    READ apps/api/tests/fixtures/broadcast-end.json and assert the handler matches it
        // a CHECKED-IN fixture: neither side writes it, so drift is detectable. The Dart test
        // reads the same file, so the contract has one source of truth
```

**Done when:**
- `cargo test -p api --test broadcast_contract` passes.
- The handler's output is byte-identical to the checked-in fixture.
- A query-only `?removeFromRoom=true` is ignored; the body decides.
- Cohost removal returns an empty body with status 204.

**Tests:** `apps/api/tests/broadcast_contract.rs` — (a) end response key set, (b) 204 on removal, (c) query-only request ignored, (d) handler output matches the checked-in fixture.

**Watch out for:** "helpfully" adding `creatorName` to the response to satisfy the old client. That would create a second source for display identity (M12).

**Depends on:** nothing (backend repo).

---

### Step 30: Publish roster events to the people who need them

**Phase:** B
**Audit reference:** §3 [H16] "Participant UI does not implement the backend roster contract", Recommended fix (define present-only versus attendance history, correct the event audience or listener refresh policy); evidence `apps/api/src/modules/broadcast/service.rs:931–967,1073–1108`.
**Why (plain words):** Roster events currently target hosts and cohosts, so ordinary listeners can never keep an accurate list even with a working transport. Either the audience changes or listeners must refresh deliberately; this step decides and documents one, and makes the presence count authoritative.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/api/src/modules/broadcast/events.rs` | Modify | Rust module | Required | Audience rule per event, as code with a comment. |
| `apps/api/src/modules/broadcast/service.rs` | Modify | Rust module | Required | Emit overall presence count with join/leave. |
| `apps/api/tests/broadcast_roster_events.rs` | Create | test | Required | Audience matrix. |

**Pseudo-code**

```
Audience policy (decide once, document once)
    participantJoined / participantLeft -> host + cohosts only. Every listener is O(n) messages
                                           per join, so O(n^2) churn in a full room
    presenceCountChanged                -> broadcast to ALL participants in the room
    roleChanged                         -> targeted: the affected user + host/cohosts

Listener refresh policy (the alternative the audit allows)
    listeners PULL the roster when they open it
    listeners refresh the room's count every ~30 s and on every reconnect

Event payload
    INCLUDE broadcastId, the affected userId where relevant, and the authoritative
            presentCount (joined AND active), never cumulative attendance
```

**Done when:**
- A listener socket receives only the authoritative count change; the roster appears when the listener pulls it.
- A host receives full join/leave detail.
- The count reflects present participants, not cumulative joins.
- The policy is written in a comment above the emitter, not only in the test.

**Tests:** `apps/api/tests/broadcast_roster_events.rs` — (a) listener receives count change only, (b) host receives detail, (c) count excludes departed users, (d) role change is targeted.

**Watch out for:** sending personal profile data to listeners in the "nudge". The nudge carries ids and counts only.


**Depends on:** nothing (backend repo).

---

### Step 31: Page and reconcile the roster on the client

**Phase:** B
**Audit reference:** §3 [H16] "Participant UI does not implement the backend roster contract", Recommended fix (traverse cursor pages, use authoritative presence counts, reconcile deltas over snapshots); evidence `apps/mobile/lib/features/broadcast/manager/participants_manager.dart:27–43,53–66,212–244`, `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart:47–61`.
**Why (plain words):** The client treats one historical page as the complete current roster, ignores `isJoined`, discards cursors and counts loaded rows, so departed users appear present and rooms over 50 are incomplete. Cursor pages plus delta reconciliation over a snapshot give an accurate list.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/manager/participants_manager.dart` | Modify | manager | Required | Cursor paging, present-only view, delta apply. |
| `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart` | Modify | service | Required | Expose cursor + `isJoined`. |
| `apps/mobile/lib/features/broadcast/widgets/participant_list_header_widget.dart` | Modify | widget | Required | Show the authoritative count. |
| `apps/mobile/test/features/broadcast/participants_manager_test.dart` | Create | test | Required | Paging + delta tests. |

**Pseudo-code**

```
ParticipantsManager (owned by ActiveBroadcastSession)
    _present : Map<Id, Participant>          // only isJoined == true
    _attendanceCursor : String?
    _snapshotGeneration = 0

FUNCTION loadFirstPage()
    gen = ++_snapshotGeneration
    page = AWAIT http.participants(broadcastId, cursor: null)
    CHECK session live AND gen == _snapshotGeneration
    _present = { for p in page.items where p.isJoined }        // history rows are not "present"
    _attendanceCursor = page.nextCursor
    // The first page is the newest, so it holds most present users. If the authoritative count
    // says some are missing, keep paging (bounded at 3 more pages) until the present set
    // matches the count or the cursor ends. If the backend can filter present-only, use that
    // instead and delete this follow-up loop.
    FOLLOW up to 3 more pages while _present.count < _authoritativeCount AND cursor != null

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
- A room with 60 participants shows every present user: the first page plus up to three follow-up pages until the present set matches the authoritative count, or the cursor ends.
- The header shows the authoritative count, not the loaded-row count.
- A first page arriving after a delta does not resurrect a departed user.

**Tests:** `apps/mobile/test/features/broadcast/participants_manager_test.dart` — (a) departed removed, (b) multi-page load reaches the authoritative count, (c) count authority, (d) stale page discarded.

**Watch out for:** Showing `_present.length` as the room count. That is the loaded page, not the room; display the authoritative count.

**Depends on:** Steps 26, 30.

---

### Step 32: Backend: revoke cohost role, presence and media permission atomically

**Phase:** B
**Audit reference:** §3 [H17] "Backend cohost removal does not reliably revoke role or presence", Recommended fix (dedicated atomic role/presence operations, connected media permission updates, matching events); evidence `apps/api/src/modules/broadcast/service.rs:896–912`, `apps/api/src/modules/broadcast/repository/pg.rs:427–447`, `apps/api/src/modules/broadcast/media.rs:41–95`.
**Why (plain words):** Removal calls `join` to demote, which returns `AlreadyPresent` for an active row (no role change) and turns an absent invitee into a joined participant; eviction never closes the participant row, and keeping the participant never revokes their existing publish permission. A dedicated revocation path that changes role, presence and media authorisation together is the only shape that cannot leave a removed cohost with rights.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/api/src/modules/broadcast/repository/mod.rs` | Modify | Rust module | Required | Add `revoke_cohost` / `close_presence` to the repository trait. |
| `apps/api/src/modules/broadcast/repository/pg.rs` | Modify | Rust module | Required | One transaction: role + presence + count. |
| `apps/api/src/modules/broadcast/service.rs` | Modify | Rust module | Required | Orchestrate DB + media + events; no `join` for demotion. |
| `apps/api/src/modules/broadcast/media.rs` | Modify | Rust module | Required | `update_permissions(room, identity, canPublish: false)`. |
| `apps/api/tests/broadcast_service.rs` | Modify | test | Required | Removal matrix. |

**Pseudo-code**

```
FUNCTION remove_cohost(broadcast_id, actor, cohost_id, remove_from_room)
    REQUIRE actor is the creator (forbidden otherwise)
    BEGIN transaction
        REQUIRE broadcast is live and cohost_id is a cohost          // not "joined as someone else"
        SET role = listener (or delete the invite row if it was pending)
        IF remove_from_room SET left_at = now() AND present = false
        RECOMPUTE present count from rows, not from a counter delta
    COMMIT
    IF remove_from_room
        AWAIT media.update_permissions(room, cohost_id, can_publish=false)
        // media failure does NOT roll back the DB: the removal succeeds and the error is returned
        // to the host, who can retry. No hidden retry loop lives here (review OE-22)
    EMIT cohost_removed(broadcast_id, cohost_id) + presence_count_changed(n)

FUNCTION join(broadcast_id, user)                                 // unchanged for new joins
    REQUIRE user is not already present                              // AlreadyPresent is an ERROR, never a demotion
    ... existing insert path ...
```

**Done when:**
- Removing an active cohost changes the role in the same transaction that flips presence.
- Removing a pending invitee does not create a joined participant.
- Media publish permission is revoked for the removed identity (integration test against the media fake).
- The emitted count equals a recount from the database.
- Removing a non-cohost returns a typed error and changes nothing.

**Tests:** `apps/api/tests/broadcast_service.rs` — (a) active cohost demoted + evicted, (b) invited-but-absent removed without joining, (c) media permission revoked, (d) non-cohost removal rejected.

**Watch out for:** Rolling back the database when the media call fails. Local state must not depend on media success, or a blip resurrects a removed cohost.

**Depends on:** Step 30.

---

### Step 33: Backend: make broadcast start/join retry-safe

**Phase:** B
**Audit reference:** §3 [H18] "Backend broadcast transitions are not safely retryable", Recommended fix (reject terminal rows atomically; move fallible preconditions earlier; make related DB transitions atomic; define compensation or committed-outcome recovery); evidence `apps/api/src/modules/broadcast/service.rs:440–476,728–744`, `apps/api/src/modules/broadcast/repository/pg.rs:283–286,349–354,468–474`.
**Why (plain words):** Start and join commit state before the later token and response work, so a failure after commit makes a retry conflict even though the first attempt took effect; and start accepts a previously ended inactive row without clearing its ending fields, so an ended broadcast can become active while still reporting ended. Moving preconditions before the commit and rejecting terminal rows atomically makes "error" mean "did not happen".

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/api/src/modules/broadcast/service.rs` | Modify | Rust module | Required | Preconditions first; no committed state before fallible work. |
| `apps/api/src/modules/broadcast/repository/pg.rs` | Modify | Rust module | Required | Conditional updates that reject terminal rows. |
| `apps/api/tests/broadcast_service.rs` | Modify | test | Required | Retry and ended-row matrix. |

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

FUNCTION join(broadcast_id, user, idempotency_key?)
    READ broadcast; REQUIRE it is live AND not ended        // terminal rejection BEFORE any write
    IF an idempotency record exists for the key RETURN its stored outcome   // the middleware
        // already does this; Step 34 is what finally sends the key
    AWAIT media token for this identity (fallible, BEFORE the DB write)
    BEGIN transaction
        INSERT participation conditionally (ON CONFLICT DO NOTHING)
        REQUIRE the row is present afterwards
    COMMIT
    STORE the outcome under the idempotency key
```

**Done when:**
- Retrying a start that failed after its media step either succeeds cleanly or reports "already live" with the same state — never a half-state.
- Starting an ended broadcast is refused and its ending fields are untouched.
- A join whose media step fails leaves no participant row.
- A repeated join with the same key returns the original outcome rather than a conflict.

**Tests:** `apps/api/tests/broadcast_service.rs` — (a) failed media step leaves no committed start, (b) ended row refused, (c) retry after commit reports already-started, (d) same-key join returns stored outcome.

**Watch out for:** Deleting rows as compensation after a failure. Ordering plus conditional updates is safer than a compensation that can itself fail.

**Depends on:** Step 29.

---

### Step 34: Make the editor create → upload → start → entry one operation

**Phase:** B
**Audit reference:** §3 [H19] "Editor progress and retries can report success without completing entry", Recommended fix (one operation spanning create → upload → start → entry; distinguish remote commitment from local connection; retry only the incomplete phase); §3 [H20] "Broadcast mutations do not carry stable logical-operation idempotency keys", Recommended fix (one key per logical mutation, persisted across ambiguous retries; do not retry every mutation). Evidence `apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart`, `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart:84`, `apps/api/src/middleware/idempotency.rs:305–315`.
**Why (plain words):** The editor reports "live" when the server created the broadcast, even if the upload or the room entry then failed, and its retry creates a *second* broadcast because nothing identifies the first attempt. One operation with named steps plus a key that survives retries makes "try again" resume instead of duplicate.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/model/broadcast_creation_step.dart` | Create | enum | Required | The step names the UI shows (creating/uploading/starting/entering/failed) plus `idle` and `done`. |
| `apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart` | Modify | manager | Required | One operation; retry resumes at the failed step. |
| `apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart` | Modify | service | Required | Accept and forward `Idempotency-Key` on create/start. |
| `apps/mobile/lib/_shared/services/http_client.dart` | Modify | service | Required | Forward the header when the caller supplies one; add nothing else. |
| `apps/mobile/lib/features/broadcast/pages/broadcast_editor_page.dart` | Modify | widget | Required | Show the step name; retry button only when a step failed. |
| `apps/mobile/test/features/broadcast/broadcast_editor_manager_test.dart` | Create | test | Required | Resume, no duplicate, honest phases. |
| `apps/mobile/test/features/broadcast/idempotency_key_test.dart` | Create | test | Required | One key per logical mutation, reused on retry. |

**Pseudo-code**

```
Simplified from design doc §5.8 and review OE-25 because the backend middleware already stores an
outcome per key; only the client was missing keys. Safety preserved because a retried create can
no longer produce a second broadcast. The backend is untouched unless a read of idempotency.rs shows
it does not replay a stored outcome for a repeated key.

enum BroadcastCreationStep { idle, creating, uploading, starting, entering, done, failed }

BroadcastEditorManager
    _key: String?                       // one key for the whole logical publish
    _step, _broadcastId

    publish(draft)
        _key ??= newUuid()
        AWAIT draftStore.saveKey(draft.localId, _key)     // survives a restart
        TRY
            step = creating
            broadcast = AWAIT http.create(draft, key: _key)
            _broadcastId = broadcast.id
            step = uploading
            FOR asset IN draft.assets -> AWAIT http.upload(broadcast.id, asset)
            step = starting
            AWAIT http.start(broadcast.id, key: _key)      // same key: the server can dedupe
            step = entering
            AWAIT manager.enter(broadcast.id)              // success only when this returns
            step = done
        CATCH error
            step = failed
            // _broadcastId is kept: the broadcast exists on the server even if upload failed
            RETHROW as EditorFailure(step, error)

    retry()
        IF _step == failed
            IF _broadcastId != null -> jump to the step AFTER the one that failed
            ELSE                    -> restart at creating with the SAME _key
        // never a fresh key: a fresh key is a second broadcast

    cancel()
        IF _broadcastId != null -> AWAIT http.end(_broadcastId)   // do not leave an orphan
        AWAIT draftStore.clearKey(draft.localId)

// UI consequence: "live" appears only after manager.enter returns, never after create.
```

**Done when:**
- Network failure during `uploading` then retry: one broadcast exists on the server, and the retry starts at `uploading`.
- Failure during `starting` then retry: the same broadcast id is used and the key is unchanged.
- `entering` failing does not set `done`.
- Restarting the app between failure and retry reuses the persisted key.
- The page never shows a live state while `step != done`.

**Tests:** `apps/mobile/test/features/broadcast/broadcast_editor_manager_test.dart` — (1) failure at each step resumes at that step; (2) create is called once across a retry; (3) `done` only after `enter` resolves. `apps/mobile/test/features/broadcast/idempotency_key_test.dart` — (4) one key per publish; (5) the key is persisted and reused after a simulated restart.

**Watch out for:** generating the key inside `create()`'s call site. A key created per attempt is a new broadcast per attempt, which is the defect H20 describes.

**Depends on:** Steps 26, 27, 33.

---

## 13. Phase F — Shared feeds

**Findings finished here:** [H11], [H12], [M08], [M09], [I01].

**What you will have when this phase is done:** a cursor-only feed base with immutable query generations, refresh/page arbitration, its own page-error surface, safe event rebinding — and, only if the backend truly cannot filter ended broadcasts, a bounded paging loop.

**How to know it's safe to ship:** the feed suite passes, including a filtered feed whose visible pages stay empty while the cursor advances and then stops at exhaustion.

**Order:** 35 → 36 → 37. Step 36 may be deleted entirely (see its check).

**What we are deliberately NOT doing in F:** no numbered-page arithmetic (nothing uses it), no client-side filter if the backend filter works, no new feed framework.


### Step 35: Capture immutable query generations and give page failures their own surface

**Phase:** F
**Audit reference:** §3 [H11] "Refresh, pagination, query changes, and invalidations race", Recommended fix (capture immutable query generations, reject stale responses, arbitrate page versus refresh, coalesce a trailing refresh); evidence `apps/mobile/lib/_shared/manager/feed_data_source.dart:24–28,87–95,149–155`, `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:75–126,183–187`. Also: §3 [M08] "Pagination errors are absent from the feed error surface", Recommended fix (separate observable page error/retry state; block automatic repeated pagination until explicit retry); evidence `apps/mobile/lib/_shared/manager/feed_data_source.dart:41–42,87–90,149–155`, `apps/mobile/lib/_shared/widgets/feed_widget.dart:127–147`. Also: §3 [I01] "The feed base mixes unused page arithmetic with cursor pagination", Recommended fix (standardize the shared base on the actual cursor contract; add numbered pagination only when an endpoint needs it); evidence `apps/mobile/lib/_shared/manager/feed_data_source.dart:100–120,171–175`, `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:55–69`.
**Why (plain words):** (**A**) Refresh and pagination can run simultaneously, responses read mutable query state, and a query change or invalidation during a request is silently dropped — so old rows and cursors overwrite fresh results. A generation counter per query plus one arbitration rule fixes all three. (**B**) `commandErrors` exposes refresh errors only, and the pagination gate checks that same channel, so next-page failures have no footer or retry and can be re-triggered incidentally while the list rebuilds. (**C**) The base carries numbered-page state while every implemented endpoint uses opaque cursors and the concrete source overrides that state anyway — a trap for the next feature, which will copy whichever model it notices first.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --- | --- | --- | --- | --- |
| `apps/mobile/lib/_shared/manager/feed_data_source.dart` | Modify | manager | Required | Generation, arbitration, trailing refresh. |
| `apps/mobile/lib/features/broadcast/manager/broadcast_feed_data_source.dart` | Move | manager | Required | From `_shared/manager/`; implements the new contract. |
| `apps/mobile/test/_shared/manager/feed_data_source_test.dart` | Create, then Modify | test | Required | Race matrix. |
| `apps/mobile/lib/_shared/widgets/feed_widget.dart` | Modify | widget | Required | Footer error with retry. |
| `apps/mobile/test/_shared/widgets/feed_widget_test.dart` | Modify | test | Required | Footer states. |

**Pseudo-code**

```
// ===== Part A — Capture immutable query generations and arbitrate page versus refresh =====
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

```
// ===== Part B — Give page-load failures their own error surface =====
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

```
// ===== Part C — Reduce the feed base to the cursor contract that actually exists =====
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
- **[A]** Changing the query while a page is in flight discards the page (no old rows).
- **[A]** A refresh during a page load waits and runs exactly once afterwards.
- **[A]** Three invalidations during one refresh produce one trailing refresh.
- **[A]** The query read inside a request is a snapshot, not the mutable field.
- **[B]** A failing next page shows a footer with retry and stops auto-triggering.
- **[B]** Retrying after success clears the footer.
- **[B]** A refresh error and a page error are both visible and distinguishable.
- **[B]** Scrolling while in the error state does not re-trigger.
- **[C]** `grep -rn "pageSize\|pageNumber\|offset" apps/mobile/lib/_shared/manager` finds nothing.
- **[C]** Both existing feeds still paginate.
- **[C]** The prefetch still triggers three items before the end.

**Tests:** **[A]** `apps/mobile/test/_shared/manager/feed_data_source_test.dart` — (a) query change discards in-flight page, (b) refresh waits for page, (c) coalesced invalidations, (d) stale cursor never appended. **[B]** `apps/mobile/test/_shared/widgets/feed_widget_test.dart` — (a) page error footer, (b) no auto retrigger, (c) retry clears, (d) refresh error unaffected. **[C]** `apps/mobile/test/_shared/manager/feed_data_source_test.dart` — (a) cursor drives `hasNextPage`, (b) prefetch trigger at `length - 3`, (c) no numbered-page API remains.

**Watch out for:** **[A]** comparing query objects by identity. Freeze them as values (or copy fields) so equality works. **[B]** Clearing `pageError` inside the pagination command. Clearing it before an explicit retry re-enables the auto-trigger loop. **[C]** Removing the length-3 prefetch along with the page arithmetic. The prefetch is on the audit keep-list.

**Depends on:** Step 4.

---

### Step 36: Keep paging through client-filtered pages only if the backend cannot filter

**Phase:** F
**Audit reference:** §3 [H12] "Client-filtered empty pages strand pagination", Recommended fix (add a real backend ended filter **or** advance through filtered pages until visible progress or exhaustion, with bounded work and a cursor-progress check). Evidence `apps/mobile/lib/features/broadcast/manager/broadcast_feed_data_source.dart:78`, audit §3 H12 mentions the broadcast list endpoint already has a status filter.
**Why (plain words):** The ended-broadcasts view filters pages after they arrive. When a whole page is filtered out, the list shows nothing and stops loading, so the user sees an empty screen that will never fill. Either the server does the filtering, or the client keeps asking — with a stop condition so it cannot loop forever.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/manager/broadcast_feed_data_source.dart` | Modify | manager | Required | Either drop the client filter or add the bounded loop. |
| `apps/mobile/lib/_shared/widgets/feed_widget.dart` | Modify | widget | Optional | Only if the loop surfaces a "load more" footer; skip if the backend filter is used. |
| `apps/mobile/test/features/broadcast/filtered_feed_test.dart` | Create | test | Required | Empty visible pages still advance and stop. |

**Pseudo-code**

```
STEP 0 (do this first, it may delete the rest of the step)
    READ the broadcast list endpoint: does it accept a status filter that returns ended broadcasts?
    IF yes
        use it in the query; DELETE the client-side filter
        // the whole step is then ~10 lines and the loop below does not exist
        DONE
    IF no
        continue

Bounded advance (only when the client must filter)
    MAX_EMPTY_PAGES = 3                      // bounded work, not a while(true)
    loadPage()
        emptyRun = 0
        LOOP
            page = AWAIT fetch(cursor)
            visible = page.items.where(matchesFilter)
            IF visible is not empty -> append; emptyRun = 0; RETURN
            IF page.nextCursor == null -> RETURN exhausted
            IF page.nextCursor == cursor -> RETURN exhausted      // cursor-progress check
            cursor = page.nextCursor
            emptyRun += 1
            IF emptyRun >= MAX_EMPTY_PAGES -> RETURN needsManualContinue
        // auto-pagination at length-3 still applies to VISIBLE items, not raw pages

    UI
        IF needsManualContinue -> show "Load more" which resets emptyRun and calls loadPage()
        // without the button the user is stuck at an empty list with no action
```

**Done when:**
- With a filter that hides the first two pages, the third page's items appear without a tap.
- Three consecutive fully-filtered pages produce one "Load more" action, not an endless request loop.
- A server that returns the same cursor twice stops instead of spinning.
- If the backend filter exists, the client filter is gone and this test asserts the request carries the status parameter.

**Tests:** `apps/mobile/test/features/broadcast/filtered_feed_test.dart` — (1) hidden pages advance to the first visible item; (2) zero visible items produce `needsManualContinue` after the bound; (3) a repeated cursor terminates; (4) the backend-filter variant sends the status parameter and performs no client filtering.

**Watch out for:** counting *pages* toward the length-3 prefetch instead of *visible items*. That makes the feed load three empty pages at the bottom of a long list, which is the same bug with extra requests.

**Depends on:** Step 35.

---

### Step 37: Rebind subscriptions safely and respect query predicates on events

**Phase:** F
**Audit reference:** §3 [M09] "Socket rebinding cancels the wrong handles; feed events ignore filters", Recommended fix (detach old handles into locals synchronously, then cancel them; apply full query predicates or invalidate when event data is insufficient); evidence `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:119–124,172–178,190–205`.
**Why (plain words):** Cancellation is unawaited and reads subscription fields across awaits while rebinding overwrites those fields, so subscriptions leak or vanish; and event insertions ignore the feed's creator/query predicates, so a creator-specific feed acquires other creators' rows.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/manager/broadcast_feed_data_source.dart` | Modify | manager | Required | Detach-then-cancel, predicate checks, invalidate fallback. |
| `apps/mobile/test/features/broadcast/feed_event_binding_test.dart` | Create | test | Required | Rebind + predicate matrix. |

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

**Depends on:** Steps 24, 35.

---

## 14. Phase P4 — Owner-bound persistence, profile and settings

**Findings finished here:** [M05], [M06], [H14], [H13], [M04], [M11], [M13], [H15] (persistence half), [M07], [I02] (schema risk).

**What you will have when this phase is done:** local services that take the owner at construction and never expose a `Box`; deterministic upserts; a profile cache keyed by viewer and subject with one key per pair; a repository with stable proxy identity; a settings path where only changed fields are sent and unknown keys survive.

**How to know it's safe to ship:** two accounts on one device cannot read each other's cached rows; a repeated upsert leaves one row; a settings save from an old client cannot delete a key it never sent.

**Order:** 38 → 39 → 40 → 41 → 42 → 43 → 44 → 45 → 46 → 47. Step 43 (backend) and 44 (client) are one contract; land them together.

**What we are deliberately NOT doing in P4:** no generic `AccountLocalData` layer over typed entities, no public/relationship cache split, no `unknown` tri-state for the follow button, no settings version column or 409, no overlay with per-field revisions.


### Step 38: Bind local storage to the account at construction

**Phase:** P4
**Audit reference:** §3 [M05] "Local storage isolation is optional rather than enforced", Recommended fix (owner-bound facade; owner-qualified reads, deletes and upserts; foreign-object rejection); §3 [M06] "ObjectBox writes can duplicate logical rows; queries lack deterministic cleanup", Recommended fix (transactional upserts, unique logical keys, idempotent desired-state setters, `try/finally` query closure). Evidence `apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart`, `apps/mobile/lib/features/profile/services/profile_cache_service.dart`, `apps/mobile/lib/objectbox-model.json`.
**Why (plain words):** Local services take an owner id on each call and can be used without one, so a forgotten argument is a silent cross-account read. Upserts look up then insert, so two writes can create two rows for one logical item. Both become impossible if the owner is a constructor field and the logical key is unique in the schema.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart` | Modify | service | Required | `ownerId` in the constructor; no `Box` getter. |
| `apps/mobile/lib/features/broadcast/services/broadcast_rows.dart` | Modify | service | Required | Add the logical key field used by the unique index. |
| `apps/mobile/lib/features/profile/services/profile_cache_service.dart` | Modify | service | Required | Same owner-bound constructor. |
| `apps/mobile/lib/_shared/services/local_storage.dart` | Modify | service | Required | Owner-qualified helpers; every query closed in `finally`. |
| `apps/mobile/lib/objectbox-model.json` | Modify | config | Required | Unique index on the logical key. |
| `apps/mobile/lib/objectbox.g.dart` | Modify (generated) | config | Required | Regenerated locally; not committed (A8). |
| `apps/mobile/lib/app/composition/account_registrations.dart` | Modify | config | Required | Construct each service with `context.accountId`. |
| `apps/mobile/lib/_shared/services/account_local_data.dart` | Do not create | service | Required | The review (OE-12) rejects a generic layer over typed entities; naming it here prevents it being added "for symmetry". |
| `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart` | Create | test | Required | Owner isolation and one-row upserts. |

**Pseudo-code**

```
Simplified from design doc §7 because a generic owner-bound data layer duplicates what three typed
services already do, and quiesce() duplicates scope disposal; safety preserved because the owner is
now a construction-time fact and the logical key is unique in the schema itself.

BroadcastLocalService(ownerId, box)          // account scope; created with the account
    REQUIRE ownerId != ''
    // no method returns the Box; no method accepts an owner other than this.ownerId

    upsert(row)
        REQUIRE row.ownerId == ownerId        // foreign object -> throw, never "fix" it
        tx = box.writeTx()
        TRY
            IF ConflictStrategy.replace is available        // verify against the pinned preview
                box.put(row, mode: replaceOnConflict)        // one statement, one row
            ELSE
                existing = box.query(logicalKey.equals(row.logicalKey)).build().findFirst()
                box.put(row.copyWith(id: existing?.id ?? 0))
        FINALLY tx.commit() / tx.abort()

    delete(id)   REQUIRE row?.ownerId == ownerId
    all()        try/finally around every query; close the Query in finally

Schema
    @Entity BroadcastRow
        @Unique logicalKey = '<ownerId>:<broadcastId>'    // uniqueness is enforced by the DB
    // a duplicate insert is now an error, not a second row

Composition
    registerLazySingleton((ctx) => BroadcastLocalService(ctx.account.accountId, db.box<BroadcastRow>()))
```

**Done when:**
- `BroadcastLocalService('a', ...)` throws on a row whose `ownerId` is `'b'`.
- Upserting the same logical key twice leaves exactly one row (count asserted).
- Concurrent upserts of the same key from two futures leave one row and one id.
- No `Box` escapes the service (`grep` shows no `box<` outside the service and `local_storage.dart`).
- Every query is closed: a test that iterates 100 pages does not leak a query handle (asserted by the fake's open-query counter).

**Tests:** `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart` — (1) foreign row rejected; (2) repeated upsert keeps one row and one id; (3) two concurrent upserts converge; (4) queries are closed after a throw.

**Watch out for:** "repairing" a foreign row by rewriting its owner. That is a silent cross-account write; throw instead and let the caller be fixed.

**Depends on:** Steps 16, 23.

---

### Step 39: Key the profile cache by viewer and subject

**Phase:** P4
**Audit reference:** §3 [H14] "Viewer-specific profile cache state leaks across accounts", Recommended fix (key viewer-relative state by `(viewerId, subjectId)` **or** split public and relationship records — one of the two, not both). Evidence `apps/mobile/lib/features/profile/services/profile_cache_service.dart:43–51`, `apps/mobile/lib/features/profile/model/dtos.dart`.
**Why (plain words):** The cache stores a profile and "am I following this person" together, under a key that does not name the viewer. Account B can therefore be shown account A's follow state. Putting the viewer in the key fixes it with one key per pair and no extra types.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/profile/services/profile_cache_service.dart` | Modify | service | Required | One key per `(viewer, subject)` holding the whole response. |
| `apps/mobile/lib/features/profile/model/dtos.dart` | Modify | freezed model | Required | Remove the split snapshot/relation types. |
| `apps/mobile/lib/features/profile/services/profile_http_service.dart` | Modify | service | Required | Cache reads/writes take both ids; no "unknown" follow state. |
| `apps/mobile/test/features/profile/profile_cache_test.dart` | Create | test | Required | A's relation is invisible to B. |

**Pseudo-code**

```
Simplified from design doc §5.12 because H14 offers "key by viewer" OR "split the records", and the
guide previously did both; safety preserved because the viewer is still in the key, so no
viewer-relative field can be read by another account.

ProfileCacheService(ownerId)                 // owner from Step 38's constructor rule

    key(viewerId, subjectId) = 'profile.v2/' + viewerId + '/' + subjectId

    read(viewerId, subjectId) -> CachedProfile?
        REQUIRE viewerId == ownerId          // a viewer other than the active account is a bug
        decode the whole response, nothing partial

    write(viewerId, subjectId, profile)
    clearFor(viewerId)

CachedProfile (freezed)
    profile: PublicProfile
    relation: FollowRelation?                // null = the server returned none; UI shows "follow"

    // DELETE PublicProfileSnapshot, ViewerRelation and the 'unknown' follow state:
    // 'unknown' disabled the follow button and needed reconcile code in the http service.
    // With one record per viewer, a miss is simply "no cached answer"; fetch, then render.

ProfileHttpService
    fetch(subjectId) -> IF cached(viewer, subject) RETURN it
                        ELSE GET, write(viewer, subject, result), RETURN
    // the cache never answers for a viewer other than the active account
```

**Done when:**
- A profile cached by account A is a miss for account B (fetch happens).
- Following someone as A does not change the follow button for B on the same device.
- A profile with no relation decodes with `relation == null` and the button is enabled.
- Only one cache key shape exists: `profile.v2/<viewer>/<subject>`.

**Tests:** `apps/mobile/test/features/profile/profile_cache_test.dart` — (1) A's entry is invisible to B; (2) a relation change for A does not leak; (3) a missing relation renders as "follow".

**Watch out for:** leaving a legacy reader for the old viewer-less key "so old caches still work". There is no shipped build (A9); that reader is a cross-account read waiting to happen.

**Depends on:** Step 38.

---

### Step 40: Give the profile repository a stable subject identity

**Phase:** P4
**Audit reference:** §3 [H13] "Cold profile proxies share an empty identity and cannot be released correctly", Recommended fix (acquire using the immutable requested identity, use an ID-bearing loading placeholder, forbid target-ID changes, release against stable ownership); audit §4 "Shared placeholder profile proxy" (simpler alternative: ID-bearing loading state); design §5.7. Evidence `apps/mobile/lib/features/profile/manager/user_profile_manager.dart:44,77–82`, `apps/mobile/lib/features/profile/model/proxies/user_profile_proxy.dart:173–198`.
**Why (plain words):** Every uncached profile acquires the same `Id.empty` proxy and fetching then mutates that identity, so two concurrent cold profiles overwrite each other and the repository's release lookup misses the original key and leaks the entry. Keying the repository by the *requested* subject and forbidding identity changes fixes both.
*Simplified from design doc §5.7 because only one repository and one proxy type exist; safety preserved because the acquisition key is immutable and release uses it, not the fetched id.*

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/profile/model/proxies/user_profile_proxy.dart` | Modify | freezed model | Required | `subjectId` final; loading placeholder carries it. |
| `apps/mobile/lib/features/profile/manager/user_profile_manager.dart` | Modify | manager | Required | Acquire/release by requested id. |
| `apps/mobile/lib/features/profile/services/user_profile_repository.dart` | Create | service | Required | Small map + refcount; deliberately not an interface. |
| `apps/mobile/test/features/profile/user_profile_repository_test.dart` | Create | test | Required | Concurrent cold-profile matrix. |

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

**Depends on:** Steps 39, 38.

---

### Step 41: Target the page's own proxy in the public profile page

**Phase:** P4
**Audit reference:** §3 [M04] "Public profile page resolves an unregistered proxy", Recommended fix (target `manager.profile`; do not register every entity proxy globally); evidence `apps/mobile/lib/features/profile/pages/user_profile_page.dart:30–40,58–61`.
**Why (plain words):** The follow-error handler asks DI for a `UserProfileProxy`, but the proxy belongs to a locally created manager, so the page throws when the handler runs. Pointing the handler at the local object removes a latent crash and avoids the temptation to register entity proxies globally.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/profile/pages/user_profile_page.dart` | Modify | widget | Required | Handler selects the local proxy via `target:`. |
| `apps/mobile/test/features/profile/user_profile_page_test.dart` | Create | test | Required | Error-handler test with no DI registration. |

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

**Depends on:** Step 40.

---

### Step 42: Compare and validate the profile editor correctly

**Phase:** P4
**Audit reference:** §3 [M11] "Profile editor compares different types and ignores bio validity", Recommended fix (compare unwrapped values consistently, include bio validity, align edit validation with the backend, correct misleading comments); evidence `apps/mobile/lib/features/profile/manager/profile_editor_manager.dart:34–39,54–69,123–129`.
**Why (plain words):** Bio strings are compared with a `MultiLineString` value object, so an unchanged bio looks changed; validity only checks the name, so an invalid bio can be saved as a deletion; and comments claim a relaxed name type that the field does not use. All three are one comparison-and-validity change.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/profile/manager/profile_editor_manager.dart` | Modify | manager | Required | Unwrapped comparisons, bio validity, honest comments. |
| `apps/mobile/lib/features/profile/widgets/profile_editor_modal.dart` | Modify | widget | Required | Disable save on invalid bio. |
| `apps/mobile/test/features/profile/profile_editor_manager_test.dart` | Create | test | Required | Diff matrix. |

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

**Depends on:** Step 6.

---

### Step 43: Backend: patch only the settings fields the client sends

**Phase:** P4
**Audit reference:** §3 [M13] "Settings writes race and overwrite unknown preference keys", backend half — either preserve unknown keys or accept field-level patches. Evidence `apps/api/src/modules/settings/repository/pg.rs:65–72`, `apps/api/src/modules/settings/dto.rs`.
**Why (plain words):** The settings endpoint replaces the whole document. A client that has never heard of a key deletes it simply by saving, and two clients saving at once clobber each other. Accepting a patch that changes only what it names removes both problems without a version column.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/api/src/modules/settings/dto.rs` | Modify | Rust module | Required | Patch type where absence is distinct from an explicit null. |
| `apps/api/src/modules/settings/model.rs` | Modify | Rust module | Required | Merge semantics documented on the type. |
| `apps/api/src/modules/settings/repository/pg.rs` | Modify | Rust module | Required | Read-modify-write inside one transaction. |
| `apps/api/src/modules/settings/service.rs` | Modify | Rust module | Required | Deep-merge only supplied leaves; return the merged object. |
| `apps/api/tests/settings_service.rs` | Modify | test | Required | Unknown keys survive; concurrent patches merge. |

**Pseudo-code**

```
Simplified from design doc §11 because a base_version column and 409 conflict protocol is a
cross-client linearizability guarantee nobody asked for; safety preserved because a write can no
longer delete a key it never mentioned, which is the actual defect.

PATCH /settings
    body = partial tree (only the leaves the client changed)

service.update(user_id, patch)
    BEGIN transaction
        current = SELECT document FROM settings WHERE user_id = $1 FOR UPDATE
        merged  = deep_merge(current, patch)
        UPDATE settings SET document = merged WHERE user_id = $1
    COMMIT
    RETURN merged

deep_merge(base, patch)
    FOR (key, value) IN patch
        IF value is object AND base[key] is object -> recurse
        ELSE IF value is null                       -> REMOVE key      // explicit removal
        ELSE                                        -> base[key] = value
    RETURN base
    // absent key: untouched. No version field, no 409, no retry loop on the server.

// Reading stays a plain GET returning the merged document.
```

**Done when:**
- A patch containing one field leaves every other field, including keys the server does not know, byte-identical.
- Two patches touching different fields, applied in sequence, both survive.
- An explicit null removes a key; an absent key does not.
- No `version` column, `If-Match` header or 409 response was added.

**Tests:** `apps/api/tests/settings_service.rs` — (1) unknown keys survive a patch; (2) absent vs explicit null behave differently; (3) two sequential patches on different fields both persist; (4) a concurrent pair of patches does not lose either field.

**Watch out for:** implementing `deep_merge` with a JSON merge-patch library that treats null as "set null" for map values. The client needs removal to be expressible, so decide and document which null means what — here: null removes.

**Depends on:** nothing (backend repo). Land it in the same change as Step 44: the client that sends patches and the endpoint that merges them are one contract.

---

### Step 44: Serialize settings edits and send only what changed

**Phase:** P4
**Audit reference:** §3 [M13] "Settings writes race and overwrite unknown preference keys", Recommended fix (serialize and coalesce mutations; send only what changed or preserve unknown keys; reconcile the response against newer edits). Evidence `apps/mobile/lib/features/settings/manager/settings_manager.dart`, `apps/mobile/lib/features/settings/model/dtos.dart`.
**Why (plain words):** Each save sends the entire settings object as the client understands it, so a slow save can drop a key another device added, and two quick edits can arrive out of order. Sending only the fields the user actually changed, one request at a time, makes both impossible — and the server (Step 43) only touches what it is told.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/settings/manager/settings_manager.dart` | Modify | manager | Required | Diff, one serial queue, adopt the server's merged answer. |
| `apps/mobile/lib/features/settings/model/dtos.dart` | Modify | freezed model | Required | `SettingsPatch` carrying only changed leaves. |
| `apps/mobile/lib/features/settings/model/entities.dart` | Modify | freezed model | Required | Remove the client revision fields. |
| `apps/mobile/test/features/settings/settings_manager_test.dart` | Modify | test | Required | Only changed fields are sent; unknown keys survive. |

**Pseudo-code**

```
Simplified from design doc §5.7 because a client overlay with per-field revisions and a flush loop
with four failure branches duplicated what the server's merge already guarantees; safety preserved
because a save still cannot delete a key the user did not touch, and edits still serialize.

SettingsManager
    _lastKnown: Settings                 // what the server last returned
    _pending: Map<path, value>           // only edits not yet acknowledged
    _queue: FIFO                          // one request at a time

    edit(path, value)
        _lastKnown = _lastKnown.withValue(path, value)     // optimistic, local only
        _pending[path] = value
        AWAIT _flush()

    _flush()  // runs in the queue
        WHILE _pending is not empty
            patch = SettingsPatch.from(_pending)     // ONLY changed leaves
            _pending.clear()
            merged = AWAIT http.patch(patch)          // server deep-merges (Step 43)
            _lastKnown = merged                       // adopt server truth, including unknown keys
            // edits made during the request are already in _pending and go in the next patch

    refresh()
        IF _queue has work -> SKIP this refresh        // a late GET must not overwrite pending edits
        _lastKnown = AWAIT http.get()

    // no revision map, no version column, no conflict UI: two clients touching DIFFERENT keys
    // both survive, which is the case the audit names.
```

**Done when:**
- Changing one setting sends a patch containing exactly that field.
- Two rapid edits produce at most two requests, the second containing only its own change.
- A key present on the server but unknown to the client is still there after a save (asserted against a stub that echoes the merged document).
- A GET that lands while an edit is queued does not overwrite the edited value.
- No `version`, `revision` or `409` handling exists in the client.

**Tests:** `apps/mobile/test/features/settings/settings_manager_test.dart` — (1) patch contains only the changed leaf; (2) two edits serialize; (3) unknown keys survive a save; (4) a stale GET is ignored while an edit is queued.

**Watch out for:** diffing against the *optimistic* local copy instead of `_lastKnown`. That produces an empty patch and the save silently does nothing.

**Depends on:** Steps 23, 38. Land it with Step 43.

---

### Step 45: Fence profile and settings continuations against owner death

**Phase:** P4
**Audit reference:** §3 [H15] "Async operations continue after their owner is disposed", Recommended fix (cancel requests where supported; check owner/session generation after every await; invalidate mutation and undo continuations on disposal); evidence `apps/mobile/lib/features/profile/manager/profile_editor_manager.dart:109–137`, `apps/mobile/lib/features/settings/manager/settings_manager.dart:246–251`, `apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:75–107,220–224`.
**Why (plain words):** A profile upload followed by a patch can cross an account transition, and disposed notifiers can still be written. Disposing a command is not cancelling its body, so each continuation needs an explicit liveness check — the pattern that makes upload-then-patch safe and that notes autosave will depend on later.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/profile/manager/profile_editor_manager.dart` | Modify | manager | Required | Ticket before upload, check before PATCH. |
| `apps/mobile/lib/features/settings/manager/settings_manager.dart` | Modify | manager | Required | Owner checks after each await. |
| `apps/mobile/lib/features/profile/manager/user_profile_manager.dart` | Modify | manager | Required | Page ticket after fetch. |
| `apps/mobile/test/features/profile/profile_editor_manager_test.dart` | Modify | test | Required | Continuation matrix. |

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
- Settings writes after `quiesce()` are rejected.
- The test double records zero requests after disposal.

**Tests:** `apps/mobile/test/features/profile/profile_editor_manager_test.dart` — (a) dispose between upload and patch, (b) late GET after dispose writes nothing, (c) settings flush after quiesce rejected.

**Watch out for:** relying on `command.dispose()` to stop the body. It does not; the body continues.

**Depends on:** Steps 40, 42, 44.

---

### Step 46: Back favourites with one account-level reactive source

**Phase:** P4
**Audit reference:** §3 [M07] "Favourites are disconnected snapshots", Recommended fix (one account-level reactive favourites source, preferably backed by an owner-filtered DB query; reconcile authoritative stored state); evidence `apps/mobile/lib/features/broadcast/manager/favourites_manager.dart:15–20,50`, `apps/mobile/lib/features/broadcast/model/proxies/broadcast_proxy.dart:21–29`, `apps/mobile/lib/features/profile/pages/my_profile_page.dart:316–321`.
**Why (plain words):** The favourites tab watches a list that starts empty and is never fetched, and a proxy's own favourite write does not update that list or any other proxy instance. One reactive collection owned by the account manager, seeded from the owner-filtered database and updated by the desired-state setter, makes every surface agree.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/manager/favourites_manager.dart` | Modify | manager | Required | `ListNotifier`, loaded once, mutated on set. |
| `apps/mobile/lib/features/broadcast/model/proxies/broadcast_proxy.dart` | Modify | freezed model | Required | Favourite write delegates to the manager. |
| `apps/mobile/lib/features/profile/pages/my_profile_page.dart` | Modify | widget | Required | Renders from the manager's collection. |
| `apps/mobile/test/features/broadcast/favourites_manager_test.dart` | Create | test | Required | Cross-surface consistency. |

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
        AWAIT local.setFavourite(summary.id, isFavourite)  // desired state (Step 38)
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

**Depends on:** Step 38.

---

### Step 47: Record the ObjectBox preview risk and gate native schema changes

**Phase:** P4
**Audit reference:** §3 [I02] "Dependency risk is explicit, not proven obsolescence", Recommended fix (document preview acceptance and migration policy; standardize mocking when tests are repaired; keep the strict analyzer baseline); evidence `apps/mobile/pubspec.yaml:38–39,55–62`.
**Why (plain words):** The database, its native libraries and its generator all sit on `6.0.0-preview.4`, and two mocking libraries coexist. Neither is a proven defect, but both need a written policy and a migration gate, or the first schema change silently becomes a release risk.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/docs/objectbox-policy.md` | Create | config | Required | Preview acceptance, migration rules, rollback stance. |
| `apps/mobile/README.md` | Modify | config | Optional | Link the policy from the top-level readme; skip and it is one click further away. |

**Pseudo-code**

```
Policy document contents
    ACCEPTED RISK: objectbox/objectbox_flutter_libs/objectbox_generator pinned to preview.4
    MIGRATION RULES
        never delete objectbox-model.json or edit UIDs by hand
        additions only through the generator; review the diff for UID churn
        enum ordinal order is persisted data: do not reorder without an explicit migration
        uniqueness/enum changes are exercised by the release-checklist line that installs over a previous dev build (Step 53) BEFORE release
    ROLLBACK STANCE
        forward-only for credentials; local rows need a compatible reader
        never delete the store to "fix" a migration
    MOCKING
        new tests use Mocktail; existing Mockito tests stay untouched (Step 55)

    // NO UID snapshot test: a checked-in map of UIDs is maintenance whose only failure it can
    // catch is caught faster by reviewing the generator's own diff (review OE-3; A9: no installed build exists)
```

**Done when:**
- The policy document exists and states the four rules above without hedging.
- No UID was edited by hand: the model diff contains only generator output.
- No `pubspec.yaml` version changed.

**Tests:** none new; the ObjectBox behaviour is covered by Step 38's `broadcast_local_service_test.dart`, and the upgrade path is a release-checklist line (Step 53).

**Watch out for:** "fixing" a migration problem by deleting the model file. That is a data-loss path the design explicitly forbids.

**Depends on:** Step 38.

---

## 15. Phase P5 — Connection, platform and reconnection

**Findings finished here:** [H06], [M03], [H07], [M16].

**What you will have when this phase is done:** one connection owner that installs handlers before connecting, hands the credential over in order and stops before disposal; a push payload contract that matches the backend plus launch-detail ingestion; reconnection reconciled into one phase with terminal reasons honoured.

**How to know it's safe to ship:** no handler fires after dispose; a token refresh on connect uses the current credential without restarting the socket; a terminal close ends the session instead of retrying.

**Order:** 48 → 49 → 50. Step 50 closes the loop on Phase R.

**What we are deliberately NOT doing in P5:** no socket restart on every token refresh (a WebSocket authenticates at handshake), no notification coordinator yet (Feature track), no feature-flag system.


### Step 48: Add the account connection owner

**Phase:** P5
**Audit reference:** §3 [H06] "No production path starts realtime delivery", Recommended fix (an explicit account connection owner initializes handlers before connecting, updates credentials, and closes on account exit); §3 [M03] "Scoped credentials and socket tokens stay stale", Recommended fix (stable account credential holder; deliberate connection credential refresh; avoid unregister/re-register for ordinary state changes); design §5.6. Evidence `apps/mobile/lib/locator.dart:170–185,290–299`, `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart:72–129`.
**Why (plain words):** Nothing in production ever calls `connect()` or `listen()`, and `SocketClient` captures the credential passed to the registrar, so a rotated token never reaches the socket. One account-owned coordinator installs handlers first, starts after readiness, applies committed credential revisions and stops before disposal.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/auth/services/account_connection_owner.dart` | Create | service | Required | Lifecycle + credential handoff; owns no media state. |
| `apps/mobile/lib/_shared/services/socket_client.dart` | Modify | service | Required | Takes a credential *supplier*, plus `start`/`stop`; no constructor token. |
| `apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart` | Modify | service | Required | `install()`/`detach()` idempotent, composition-callable. |
| `apps/mobile/lib/app/composition/account_registrations.dart` | Modify | config | Required | Register and start after `ready`. |
| `apps/mobile/test/features/auth/account_connection_owner_test.dart` | Create | test | Required | Start/credential/stop matrix. |

**Pseudo-code**

```
AccountConnectionOwner (account scope)
    new(context, socket, adapters, credentialSupplier)      // supplier reads committed revision
    _stopping = false
    _stopResult : Future<void>?

FUNCTION initialize()
    REQUIRE context live AND not stopping
    FOR each adapter: adapter.install()                     // handlers BEFORE any connection attempt
    // NO credential subscription: SocketClient reads credentialSupplier on connect and reconnect only

FUNCTION start()                                            // called after `ready` AND verified
    REQUIRE context live, ready, verified, credential usable
    AWAIT socket.start(credentialSupplier)
    CHECK context live; IF stale AWAIT socket.stop(); RETURN   // publish nothing
    IF start failed PUBLISH connectionUnavailable              // HTTP session stays valid

FUNCTION stop()
    _stopping = true                                        // rejects start and further callbacks
    AWAIT socket.stop()
    FOR each adapter: AWAIT adapter.detach()
    // completes only when delivery has actually stopped; repeats join the same future
```

**Done when:**
- After login, handlers are installed before the first connection attempt (test asserts call order).
- A token refresh while connected changes nothing; the next reconnect reads the fresh credential.
- `stop()` twice awaits one completion and no further delivery occurs.
- Account exit awaits `stop()` before the socket is disposed.

**Tests:** `apps/mobile/test/features/auth/account_connection_owner_test.dart` — (a) install-before-start order, (b) a token refresh does not restart the socket, (c) late start after stop publishes nothing, (d) stop idempotent.

**Watch out for:** holding the token in a field. The supplier reads the committed credential each time — that is the fix for the captured-token defect.

**Depends on:** Steps 27, 5.

---

### Step 49: Fix the push payload and install platform adapters at the root

**Phase:** P5
**Audit reference:** §3 [M16] "Push payloads, channels, and tap lifecycle disagree", Recommended fix (contract fixtures, recipient attribution, validated destinations, cold/warm handling); §3 [H24] "Push and durable notifications are not implemented end-to-end", mobile half (payload ingestion at the root). Evidence `apps/mobile/lib/_shared/services/push_notifications_service.dart`, `apps/mobile/lib/_shared/services/local_notifications_service.dart`, `apps/mobile/lib/main.dart:6–23`.
**Why (plain words):** The client and the sender disagree about the payload keys and channel ids, a tap during cold start is dropped because nothing reads the launch details, and the service navigates directly. Fixing the contract and handing one validated destination to Step 19's slot addresses all three.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/_shared/services/push_notifications_service.dart` | Modify | service | Required | Decode the agreed payload; never navigate; no coordinator. |
| `apps/mobile/lib/_shared/services/local_notifications_service.dart` | Modify | service | Required | Same payload contract; read app-launch details on cold start (review A-4). |
| `apps/mobile/lib/main.dart` | Modify | config | Required | Create the platform handlers at the root, before `runApp`. |
| `apps/mobile/test/_shared/services/push_payload_test.dart` | Create | test | Required | Contract fixture decoded on the client. |

**Pseudo-code**

```
Simplified from design doc §5.11 because an account-scoped coordinator created before any inbox
exists cannot show anything; safety preserved because a tap is still validated, still waits for
readiness and still cannot cross accounts (Step 19's slot). The coordinator arrives in Step 59,
with the inbox that gives it content.

Payload contract (agreed with the backend, asserted by a shared fixture)
    type:        'broadcast_live' | 'broadcast_ended' | 'follow' | 'system'
    recipientId: String                 // whose notification this is
    broadcastId: String?                // required for the two broadcast types
    title, body: String                 // plain text until Step 57's limitation is lifted

PushNotificationsService (root, created before runApp)
    onMessage(payload)
        d = decode(payload)             // unknown type -> drop, log the type only
        destinations.put(pathFor(d), d.recipientId)     // Step 19; validates the path
        // no Navigator here: there is no context and no readiness guarantee

    onMessageOpenedApp(payload) -> same as onMessage
    initialMessage() -> AWAIT firebase.getInitialMessage(); handle it like a tap   // cold start

LocalNotificationsService (root)
    launchDetails() -> details = AWAIT plugin.getNotificationAppLaunchDetails()
                       IF details.didNotificationLaunchApp -> handle(details)
                       // previously ignored, so a cold-start local tap did nothing (M16)

    show(d) -> channel = channels[d.type]     // ids from the same fixture, not string literals

main()
    AWAIT push.init()
    AWAIT local.init()
    AWAIT push.initialMessage()
    AWAIT local.launchDetails()
    runApp(...)                                // the slot survives until Step 19 consumes it
```

**Done when:**
- The shared fixture's payload decodes to a path and a `recipientId` on the client, with a test that fails if either side renames a key.
- A cold-start tap and a cold-start local-notification launch both produce exactly one pending destination.
- No platform callback calls `Navigator` or `GoRouter` directly.
- An unknown `type` is dropped without a crash and without a navigation.
- No `NotificationCoordinator` is registered in the root (it belongs to Step 59).

**Tests:** `apps/mobile/test/_shared/services/push_payload_test.dart` — (1) each fixture type decodes; (2) an unknown type is dropped; (3) `recipientId` survives decoding; (4) cold-start handling produces one destination.

**Watch out for:** initialising these services inside the authenticated scope. A tap can arrive before any account exists; the root is the only correct place (H24).

**Depends on:** Steps 19, 48.

---

### Step 50: Reconcile reconnection into one phase and honour terminal reasons

**Phase:** P5
**Audit reference:** §3 [H07] "Reconnection has stale credentials, conflicting state, and ignored terminal reasons", Recommended fix (one phase machine, preserve disconnect reasons, let the SDK reconnect first, obtain fresh entry credentials when a full reconnect requires them, reconcile after transport return); §3 [M03] "Scoped credentials and socket tokens stay stale", part "deliberate connection credential refresh"; evidence `apps/mobile/lib/features/broadcast/manager/live_session_manager.dart:276–289,307–384`, `apps/mobile/lib/_shared/services/livekit_client.dart:266–278`.
**Why (plain words):** Session `state` and `status` can contradict each other, failed initial connections paint over later events, and disconnection reasons are discarded — so a kick or room deletion is retried as if it were a network blip, and a long session retries with an expired media token. Mapping transport events into the single `LivePhase` and classifying terminal reasons removes both.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/manager/active_broadcast_manager.dart` | Modify | manager | Required | Consume connection/room phases; classify terminal. |
| `apps/mobile/lib/_shared/services/livekit_client.dart` | Modify | service | Required | Expose typed disconnect reasons; no state painting. |
| `apps/mobile/lib/features/broadcast/model/live_snapshot.dart` | Modify | freezed model | Required | Add `LiveFailure.terminal(reason)` and retryability. |
| `apps/mobile/test/features/broadcast/live_reconnect_test.dart` | Create | test | Required | Reason matrix. |

**Pseudo-code**

```
FUNCTION onRoomEvent(event)
    SWITCH event
        reconnecting      -> IF phase == live PUBLISH reconnecting        // SDK owns the first attempt
        reconnected       -> AWAIT reconcileRosterAndFeeds(); PUBLISH live
        disconnected(r)   -> classify(r)
        participantLeft   -> roster delta (Step 30)
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

**Depends on:** Steps 24, 26, 48.

---

## 16. Phase D — Release and device validation

**Findings finished here:** [M17], [M25], [C04] (proven), and the device evidence the audit demands for [C02], [C03], [M05], [M06], [M18], [M16], [H24].

**What you will have when this phase is done:** a release build that can be signed and installed, a stated background-audio behaviour that was tested, a realtime protocol verified against the real backend, and one `docs/release-checklist.md` with pass/fail lines instead of four device documents and three integration tests.

**How to know it's safe to ship:** every checklist line has a ticked result from a named device, or the release is blocked.

**Order:** 51 → 52 → 53. Step 52 needs 24 and 50; step 53 needs 51.

**What we are deliberately NOT doing in D:** no CI pipeline (A13), no `integration_test` files for secure storage, upgrade or ObjectBox, no device document per gate, no upgrade-from-installed-build scenario (A9).


### Step 51: Make the release build releasable and establish background audio

**Phase:** D
**Audit reference:** §3 [M25] "Build/platform configuration is not release-ready", Recommended fix (production signing and provisioning, real notification resources, permission audit, platform build/device validation); §3 [M17] "Background-audio support is declared but not established", Recommended fix (define supported background behaviour, implement the service/audio-session lifecycle, test locked screen, interruptions and OS termination). Evidence `apps/mobile/android/app/build.gradle.kts:1–5,37–49`, `apps/mobile/android/app/src/main/AndroidManifest.xml`, `apps/mobile/ios/Runner/Info.plist`, `apps/mobile/lib/_shared/services/permissions_service.dart:249–290`.
**Why (plain words):** The release build is signed with the debug key, has no notification icon or iOS entitlements, and declares a live-audio product without proving audio survives a locked screen. Nothing can be installed or reviewed until the build itself is real.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/android/app/build.gradle.kts` | Modify | config | Required | Release signing from a local, git-ignored `key.properties`; fail the build if it is missing. |
| `apps/mobile/android/app/src/main/AndroidManifest.xml` | Modify | config | Required | Media-playback service, `FOREGROUND_SERVICE_MEDIA_PLAYBACK`, notification icon; no unused permissions. |
| `apps/mobile/android/app/src/main/res/drawable/ic_notification.xml` | Create | config | Required | A real small icon; the default launcher icon is rejected by Android. |
| `apps/mobile/ios/Runner/Runner.entitlements` | Create | config | Required | Push + background audio. |
| `apps/mobile/ios/Runner/Info.plist` | Modify | config | Required | `UIBackgroundModes: audio`; microphone usage string already present — verify wording. |
| `apps/mobile/lib/_shared/services/permissions_service.dart` | Modify | service | Required | Ask for microphone only at the point of going live. |
| `docs/release-checklist.md` | Create | doc | Required | The pass/fail list finished in Step 53. |

**Pseudo-code**

```
DECISION (review §1 rule 7): Option A — supported background audio — is the default, because the
product is live audio. If the developer picks Option B, hide the live feature and say so in the UI
instead of shipping a session that dies when the screen locks.

Signing (A13: no CI exists, so this is local)
    key.properties (git-ignored) -> storeFile, storePassword, keyAlias, keyPassword
    build.gradle.kts
        IF key.properties is missing -> FAIL the release build with a message naming the file
        signingConfigs.release { ... }
        buildTypes.release { signingConfig = signingConfigs.release }

Android
    <service android:name="...ForegroundService" android:foregroundServiceType="mediaPlayback"/>
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK"/>
    <uses-permission android:name="android.permission.POST_NOTIFICATIONS"/>   // only if push ships
    notification small icon = @drawable/ic_notification
    // audit: every declared permission is either used or removed

iOS
    UIBackgroundModes = [audio]
    entitlements: aps-environment (only when push ships), audio
    AVAudioSession category = playback, active only while a session is live

Background-audio behaviour to implement and then test
    lock the screen while live -> audio continues
    incoming call -> audio ducks/stops, resumes afterwards (or ends cleanly, stated in the checklist)
    app swiped away -> session ends and the user is told on next launch
```

**Done when:**
- `flutter build apk --release` fails with a clear message when `key.properties` is absent, and produces a signed artifact when it is present.
- The release APK installs on a device and shows the notification icon, not a white square.
- Locking the screen during a live broadcast keeps audio playing (device step, recorded in Step 53).
- No permission is declared that the app never requests.

**Tests:** no unit test can prove signing or audio; the evidence is the build output plus the device lines in `docs/release-checklist.md` (Step 53).

**Watch out for:** committing `key.properties` or the keystore. Add both to `.gitignore` in this step; a leaked release key cannot be un-leaked.

**Depends on:** Step 27.

---

### Step 52: Validate the realtime protocol against the real backend

**Phase:** D
**Audit reference:** §3 [C04] "Mobile and backend use incompatible realtime protocols", Recommended fix (raw-WebSocket adapter with heartbeat, authenticated connection, bounded reconnect and reconciliation — verified, not assumed); §3 [H07] "Reconnection has stale credentials, conflicting state, and ignored terminal reasons". Evidence `apps/api/src/routes/ws.rs:241–271,492–503`.
**Why (plain words):** A local test client against a mock proves framing; only the real backend proves the handshake, the heartbeat cadence, the close codes and the recovery behaviour after a real network change.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `docs/release-checklist.md` | Modify | doc | Required | Record these six rows under "Realtime"; the checklist is the only device document (review OE-19). |
| `apps/api/tests/ws_protocol.rs` | Create | test | Required | Server-side framing contract (Rust). |

**Pseudo-code (manual procedure — the exact tests to run)**

| # | Scenario | Pass condition |
| - | -------- | -------------- |
| 1 | Sign in with the backend running; enter a broadcast room | Handshake succeeds; one room event arrives within 5 seconds |
| 2 | Airplane mode for 60 seconds, then restore | Client shows reconnecting, then live; no duplicate session; roster reconciles |
| 3 | Host ends the broadcast while a listener is connected | Listener moves to `ended` once, no reconnect loop |
| 4 | Kick a participant from the room | Kicked client ends immediately; no retry |
| 5 | Rotate the access token (force a refresh) | Connection continues or reconnects once; no 401 storm |
| 6 | Background the app for 5 minutes and resume | Exactly one connection; no duplicate delivery |


**Done when:**
- All six rows recorded in `docs/release-checklist.md`, each with a device, a date, and the backend/client logs.
- Framing matches `{event, data}` exactly (server test asserts the shape).
- Production realtime is enabled only after this passes; otherwise the app ships with the visible "realtime unavailable" state from Step 24.

**Tests:** `apps/api/tests/ws_protocol.rs` — (a) frame shape on a representative event, (b) auth rejection on upgrade with a bad token, (c) close code for a kicked participant.

**Watch out for:** Concluding from a mock or simulator run. The handshake and close codes only exist on the real server.

**Depends on:** Steps 24, 50, 51.

---

### Step 53: Prove the release gates with one checklist

**Phase:** D
**Audit reference:** §3 [C02] "Session-store mutations are neither atomic nor identity-conditional" and [C03] "Explicit logout can leave a usable session recoverable", "verify on device" requirement; §3 [M18] "…recovery stores a bearer token in preferences"; §3 [M05] "Local storage isolation is optional rather than enforced"; §3 [M06] "ObjectBox writes can duplicate logical rows", schema/uniqueness half; §3 [I02] "Dependency risk is explicit, not proven obsolescence", Recommended fix (installed-schema evidence before release); §3 [M16] "Push payloads, channels, and tap lifecycle disagree", cold/warm taps; §3 [M24] "Critical lifecycle tests are commented out" (the evidence requirement).
**Why (plain words):** Four of the audit's release gates are things only a real device can show: does a token survive a kill, does a logout stay logged out, does a tap open the right page, does the schema upgrade keep one row. They were four separate steps, three integration tests and four documents. One checklist with pass/fail lines gives the same evidence and is the thing you will actually keep current.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `docs/release-checklist.md` | Modify | doc | Required | Extends the file created in Step 51 with the pass/fail lines below. |
| `docs/device-validation/` | Do not create | doc | Required | Replaced by the single checklist (review OE-19). |
| `apps/mobile/integration_test/` | Do not create | test | Required | A solo developer will not maintain three integration suites that need a device; the manual lines carry the same evidence. |

**Pseudo-code**

```
docs/release-checklist.md
    ## Build
    [ ] release APK/IPA signed with the release key (Step 51)
    [ ] installs on <device> from a clean uninstall

    ## Secure storage durability            (C02, C03)
    [ ] sign in, force-kill, relaunch -> still signed in
    [ ] sign out, force-kill, relaunch -> signed out
    [ ] deny/clear the keystore entry while signed in -> app asks to sign in again,
        shows ONE retry state, never a half-logged-in screen

    ## Local data                           (M05, M06, I02)
    [ ] install this build over the previous dev build -> exactly one ObjectBox handle,
        no duplicated rows for one logical item
    [ ] two accounts on one device -> neither sees the other's cached rows

    ## Background audio                     (M17, Step 51)
    [ ] lock screen while live -> audio continues
    [ ] incoming call -> stated behaviour happens (duck-and-resume or clean end)
    [ ] swipe the app away -> session ends; next launch says so

    ## Realtime                             (C04, H07 -> Step 52)
    [ ] see docs/device-validation is gone; record the result of Step 52 here

    ## Push and taps                        (H24, M16)
    [ ] cold-start tap -> correct page, once
    [ ] warm tap -> correct page, once
    [ ] tap addressed to another account -> snackbar, no navigation

    ## Upgrade from a published build       (A9)
    [ ] N/A today: no build has been published. The first public release must add
        "install over version N-1" here before it ships.
```

**Done when:**
- Every line above exists in the file with a blank box, a device name column and a date column.
- Step 52's realtime result is recorded there rather than in a separate document.
- No `integration_test/` file or `docs/device-validation/` directory was created.
- The A9 line is present and explicit, so the gap is a recorded decision rather than a forgotten gate.

**Tests:** the checklist itself, plus the automated suites named in Steps 4, 5 and 38. A checklist line is "done" only with a device name and a date.

**Watch out for:** ticking lines from a debug build. Signing changes storage behaviour on iOS and notification behaviour on Android; the checklist is only meaningful on the release artifact.

**Depends on:** Steps 2, 3, 9, 47, 51.

---

## 17. Phase C — Cleanup and reference quality

**Findings finished here:** [M20], [M22], [M24], [M26], [L04], [L05], [L06].

**What you will have when this phase is done:** routes that exist, a one-way import graph checked by a script you run before pushing, the commented-out lifecycle tests living in the step that owns their subject, no UI pretending to do what it cannot, and a stated localization limitation.

**How to know it's safe to ship:** `tool/check_import_direction.sh` passes, no commented-out suite remains, and every hidden or disabled control has a stated reason.

**Order:** 54 → 55 → 56 → 57. These steps touch files earlier steps already changed, so do them last.

**What we are deliberately NOT doing in C:** no CI pipeline, no ARB/localization framework, no feature-flag system, no re-listing of tests earlier steps already wrote.


### Step 54: Navigate by routes that exist and make the import graph one-way

**Phase:** C
**Audit reference:** §3 [M20] "Named navigation is used for a route without that name", Recommended fix (declare a distinct route name or navigate by a constructed location); §3 [M22] "Shared infrastructure imports features and broad barrels hide the dependency graph", Recommended fix (move broadcast/auth/app files to their owners, use narrow imports, keep useful feature barrels, drop umbrella exports); §3 [L05] "Dead paths, fixtures, and documentation obscure the reference implementation", Recommended fix (remove or isolate unused implementations, correct lifecycle comments, document bootstrap and decisions). Evidence `apps/mobile/lib/features/broadcast/pages/home_page.dart:150–151,192–195,236–238`, `apps/mobile/lib/_routing/router.dart:92–96`, `apps/mobile/lib/_shared/_shared.dart:1–23`, `apps/mobile/lib/features/features.dart`, `apps/mobile/README.md`.
**Why (plain words):** A home card navigates by a name no route declares, so it either throws or silently does nothing. Meanwhile `_shared` exports feature code and a single `features.dart` re-exports everything, which is why the dependency graph cannot be seen. Both are cheap to fix now that the moves from earlier phases have landed.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/_routing/router.dart` | Modify | config | Required | Declare the route the home page already uses (or change the call site). |
| `apps/mobile/lib/features/broadcast/pages/home_page.dart` | Modify | widget | Required | Navigate by a declared name or a constructed location. |
| `apps/mobile/lib/_shared/_shared.dart` | Modify | config | Required | Remove exports of feature files; keep shared widgets/services. |
| `apps/mobile/lib/features/features.dart` | Modify | config | Required | Delete the umbrella export (review A-8: it is Required, not optional). |
| `apps/mobile/lib/_shared/services/database.dart` | Leave in place | service | Required | The reference tree keeps it in `_shared/services` (audit §7); do not move it (review A-8). |
| `apps/mobile/lib/features/broadcast/model/entities.dart` | Modify | freezed model | Required | Delete unused unions; keep `EndReason` (Step 25 uses it). |
| `apps/mobile/lib/features/broadcast/model/fixtures/broadcast_samples.dart` | Create | model | Optional | Only if a widget test needs sample data; skipped otherwise, losing nothing at runtime. |
| `apps/mobile/lib/features/profile/manager/user_profile_manager.dart` | Modify | manager | Required | Replace the stale lifecycle comment with the real one. |
| `apps/mobile/README.md` | Modify | doc | Required | Bootstrap order, the two scopes, and the decisions recorded in this guide. |
| `tool/check_import_direction.sh` | Create | config | Required | Fails when `_shared` or `_core` imports `features`/`app`. |
| `apps/mobile/test/features/broadcast/home_navigation_test.dart` | Create | test | Required | The named route exists. |

**Pseudo-code**

```
Simplified from design doc §8 because there is no CI (A13) and no need for a graph tool; safety
preserved because the direction is still checked by a script before every push.

Route
    router declares: name 'broadcastDetails', path '/broadcast/:id'
    home_page: context.pushNamed('broadcastDetails', pathParameters: {'id': id})
    // test asserts GoRouter finds a match for the name

Barrels
    _shared/_shared.dart    exports services/, widgets/, layouts/ ONLY
    features/features.dart  DELETED (no umbrella); callers import the feature's own file
    per-feature barrels (features/broadcast/broadcast.dart) stay: they are the public surface

tool/check_import_direction.sh
    FOR file IN lib/_shared/**, lib/_core/**
        IF grep -q "import .*\(features\|app\)/" -> FAIL with the file and the line
    # run by hand before merging, or install as a hook:
    #   git config core.hooksPath tool/hooks   (tool/hooks/pre-push calls this script)

Dead paths
    entities.dart: delete the unused unions; keep EndReason
    user_profile_manager.dart: state the real lifecycle (page-owned, disposed with the page)
    README: how to run, the two scopes, where credentials live, the known limitations
```

**Done when:**
- `home_navigation_test` resolves the name to the details page; no `pushNamed` call uses an undeclared name.
- `tool/check_import_direction.sh` exits 0 on the final tree and non-zero when a violating import is added.
- `features/features.dart` is gone and the analyzer reports no unresolved exports.
- `_shared` exports nothing from `features/`.
- README describes what the code does today, including the Step 57 limitation.

**Tests:** `apps/mobile/test/features/broadcast/home_navigation_test.dart` — (1) the name resolves; (2) the constructed location carries the id. The import script is its own check.

**Watch out for:** deleting `features.dart` before fixing its importers. Let the compiler list them, then remove the file last.

**Depends on:** Steps 16, 27, 38.

---

### Step 55: Reinstate the lifecycle tests that were commented out

**Phase:** C
**Audit reference:** §3 [M24] "Critical lifecycle tests are commented out", Recommended fix (reinstate focused tests with controllable futures and fakes; do not merely uncomment obsolete expectations); §3 [I02] "Dependency risk is explicit, not proven obsolescence", part "standardise mocking when tests are repaired". Evidence `apps/mobile/test/features/auth/user_scope_manager_test.dart:1–187`, `apps/mobile/test/features/broadcast/live_scope_manager_test.dart`.
**Why (plain words):** The lifecycle coverage M24 asks for now exists — it was written with the steps that changed the behaviour, against fakes and held completers. What remains is to delete the commented-out originals (their subjects no longer exist) and write down the one mocking convention, so the next test does not introduce a third style.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/test/features/auth/user_scope_manager_test.dart` | Delete | test | Required | Its subject was deleted in Step 16; its cases live in `test/app/account_transition_owner_test.dart`. |
| `apps/mobile/test/_support/README.md` | Create | doc | Required | Three lines: Mocktail for new and repaired tests, concrete fakes for collaborators, one claim per file. |

**Pseudo-code**

```
Simplified from design doc §10 because a second wave of tests would duplicate what Steps 16, 21, 26,
27 and 40 already assert; safety preserved because nothing is deleted that has a live subject, and
the grep below stops a suite from silently staying disabled.

Delete
    test/features/auth/user_scope_manager_test.dart        // subject gone (Step 16)
    // live_scope_manager_test.dart and stream_manager_test.dart were deleted with their subjects

Keep (already written, do not re-list)
    test/app/account_transition_owner_test.dart            // Step 16
    test/features/broadcast/active_broadcast_manager_test.dart  // Step 26
    test/features/profile/my_profile_manager_test.dart     // Step 22

Convention (test/_support/README.md)
    - new or repaired tests use Mocktail; existing Mockito tests are left alone
    - collaborators are concrete fakes in test/_support/, not mocks, unless a framework type
      (Plugin, PlatformChannel) forces a mock
    - one file defends one claim; a second claim gets its own file

Check
    grep -rn "^\s*//\s*\(test\|group\|testWidgets\)(" apps/mobile/test
    grep -rn "skip:\s*true\|, skip:" apps/mobile/test
    // both must be empty; a disabled suite hides a regression instead of reporting it
```

**Done when:**
- The two greps above return nothing.
- No test file's subject has been deleted (`dart analyze` reports no missing imports).
- `test/_support/README.md` exists and names Mocktail as the convention for new work.
- The full `flutter test` run is green without any `skip:`.

**Tests:** the suites already written in Steps 16, 22, 26, 27 and 40; this step adds no new assertions.

**Watch out for:** uncommenting the old expectations to "restore coverage". They assert the removed `UserScopeManager` protocol; they will fail for the right reason, which wastes an afternoon.

**Depends on:** Steps 16, 21, 40.

---

### Step 56: Stop presenting unimplemented behaviour as working

**Phase:** C
**Audit reference:** §3 [M26] "UI presents incomplete behavior as operational", Recommended fix (disable/hide unsupported actions; implement coherent capabilities before labeling the feature complete; bound the list and use real search); §3 [L04] "Small UI/validation mismatches remain", Recommended fix (align copy, validation and metrics with actual capabilities/contracts). Evidence `apps/mobile/lib/features/broadcast/pages/broadcast_details_page.dart:117–120`, `apps/mobile/lib/features/broadcast/pages/ended_broadcast_page.dart:105–107`, `apps/mobile/lib/features/broadcast/pages/broadcast_editor_page.dart:424–432`, `apps/mobile/lib/features/broadcast/widgets/add_cohost_modal.dart:33–74`, `apps/mobile/lib/features/broadcast/widgets/broadcast_control_buttons.dart:97–99`, `apps/mobile/lib/features/broadcast/widgets/broadcast_card.dart:58–67`, `apps/mobile/lib/features/settings/pages/security_settings_page.dart:25–31`.
**Why (plain words):** Restream, listen-back, password save and deletion are enabled no-ops; cohost search is unwired with fake profiles and inconsistent limits; listener leave uses host confirmation copy; the title rule omits the backend's 3–100 policy; the UI says 10 MB against a 5 MiB limit; and "live count" shows cumulative attendance. Each one teaches a user something false.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/broadcast/pages/broadcast_details_page.dart` | Modify | widget | Required | Hide restream/listen-back until implemented. |
| `apps/mobile/lib/features/broadcast/widgets/add_cohost_modal.dart` | Modify | widget | Required | Real search, bounded list, consistent limit, and a bounded-height layout instead of the current unbounded flex (audit §3 M26 evidence 33–74, 1193–1194). |
| `apps/mobile/lib/features/broadcast/pages/ended_broadcast_page.dart` | Modify | widget | Required | Remove the no-op actions shown there (audit evidence 105–107). |
| `apps/mobile/lib/features/broadcast/pages/broadcast_editor_page.dart` | Modify | widget | Required | Editor copy matches the real limits (audit evidence 424–432). |
| `apps/mobile/lib/features/settings/pages/settings_page.dart` | Modify | widget | Required | Language row is honest; the full fix is Step 57 (audit evidence 113–118). |
| `apps/mobile/lib/features/broadcast/widgets/broadcast_control_buttons.dart` | Modify | widget | Required | Listener-specific leave copy. |
| `apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart` | Modify | manager | Required | Title 3–100 validation; image limit 5 MiB copy. |
| `apps/mobile/lib/features/broadcast/widgets/broadcast_card.dart` | Modify | widget | Required | Presence count, not cumulative attendance. |
| `apps/mobile/lib/features/settings/pages/security_settings_page.dart` | Modify | widget | Required | Disable password save until wired. |

**Pseudo-code**

```
RULE: an action that cannot succeed is either hidden or explicitly labelled "coming soon"
    restream / listen-back / password-save / delete-account -> HIDDEN or removed until implemented
    // hiding is enough; a feature-flag framework would be a new mechanism (review OE-19)
COHOST MODAL
    REAL search endpoint; DEBOUNCE input; BOUND the list (e.g. 20) and page it
    LIMIT consistent with the backend constant (one source)
    LAYOUT: give the list a bounded height (Expanded inside a fixed-height sheet) — the current
            unbounded flex breaks on a device once the list grows
COPY
    listener leave: "Leave broadcast?"   host leave: "End broadcast?"
TITLE
    REQUIRE 3..100 characters, matching the backend validator
IMAGE
    state 5 MiB, matching the backend limit
COUNTER
    "X listening" reads the authoritative present count (Step 31)
```

**Done when:**
- No enabled control performs nothing (click every action once on a device).
- Cohost search returns real users and never renders a fake profile.
- Title/limit copy matches the backend constants, with a test asserting the numbers.
- Listener leave copy is listener-specific.

**Tests:** `apps/mobile/test/features/broadcast/ui_copy_contract_test.dart` — (a) title policy matches the backend constant, (b) image limit string matches, (c) listener/host copy differs, (d) disabled actions are not tappable.

**Watch out for:** Hiding a broken action without telling anyone. Label first, remove later.

**Depends on:** Steps 31, 42.

---

### Step 57: State the localization limitation and keep each screen's copy in one place

**Phase:** C
**Audit reference:** §3 [L06] "Language setting is not connected to localization", Recommended fix (introduce localization before expanding UI copy; meanwhile explicitly label unsupported language changes). Evidence `apps/mobile/lib/features/settings/manager/settings_manager.dart:29–38`, `apps/mobile/lib/app.dart:85–94`, `apps/mobile/lib/features/settings/pages/settings_page.dart:113–118`.
**Why (plain words):** The settings screen offers a language choice that changes nothing, which is a promise the app does not keep. The audit's full fix is to adopt localization; that is a project of its own, so this step makes the limitation visible instead of pretending, and keeps new copy from making it worse.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| --------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/settings/pages/settings_page.dart` | Modify | widget | Required | Label the language row as not yet applied. |
| `apps/mobile/lib/features/settings/manager/settings_manager.dart` | Modify | service | Required | Keep storing the value; expose that it is not applied. |
| `apps/mobile/README.md` | Modify | doc | Required | Three lines recording the deviation from L06 and what adopting l10n would cost. |
| `docs/localization-plan.md` | Do not create | doc | Required | A plan nobody follows is worse than a stated limitation (review A-11/OE-19). |

**Pseudo-code**

```
Simplified from the audit's own alternative ("meanwhile explicitly label unsupported changes")
because adding ARB files, a locale delegate and a translation pipeline for an untranslated app is
larger than the remediation; safety preserved because no screen now claims a behaviour it lacks.

Settings page
    Language row: current value + subtitle "Interface language is not available yet"
    Selecting a language stores the preference (it is a real setting) and shows the subtitle
    // no fake restart prompt, no "applied" snackbar

SettingsManager
    language -> stored value
    languageApplied -> false        // a single named fact, so a future l10n step flips it in one place

Copy convention (no framework, enforced by review)
    Each screen keeps its strings in one const block at the top of its file.
    // when l10n arrives, that block becomes the ARB source; nothing else has to move

README
    Known limitation: the interface is English-only; the language setting is stored but not applied.
    Adopting localization means: intl + ARB files, one delegate, and moving each screen's const block.
```

**Done when:**
- The settings page states the limitation next to the control.
- Changing the language does not show a success message implying the UI changed.
- `grep -rn "AppLocalizations\|intl" apps/mobile/lib` returns nothing (no half-adopted framework).
- README records the limitation and the cost of fixing it.
- No `docs/localization-plan.md` exists.

**Tests:** `apps/mobile/test/features/settings/settings_page_test.dart` — (1) the subtitle is present; (2) selecting a language persists it; (3) no "applied" confirmation appears.

**Watch out for:** hiding the setting instead. The value is real (it drives future behaviour and is synced), so storing it is correct; only the claim that it changes the UI is wrong.

**Depends on:** Step 44.

---

## 18. Feature track — durable notifications

**Not part of the remediation.** Audit §6 treats notifications as upcoming-feature work that waits on a backend contract. Keeping it inside the remediation would block "done" on a new backend module, so it lives here: do it when the product needs it, not to close an audit finding.

**Findings addressed here:** [H24] and the audit §6 readiness gates.

**What you will have when this track is done:** a durable, paginated, per-account inbox with a device-token policy; a client that shows real state or an honest "unavailable"; and written gates for chats, notes, discover and notifications.

**How to know it's safe to ship:** an inbox survives a restart, two accounts never see each other's items, and a revoked device token stops receiving.

**Order:** 58 → 59 → 60.

**What we are deliberately NOT doing here:** no readiness document that duplicates audit §6 (link to it), no optional seed files, no localization keys (plain text until Step 57's limitation is lifted).


### Step 58: Give notifications a durable inbox and a device-token policy

**Phase:** FT
**Audit reference:** §3 [H24] "Push and durable notifications are not implemented end-to-end", Recommended fix (define durable inbox identity/pagination/read state and device-token policy; implement the account-scoped coordinator afterwards); audit §6 "Notifications — Backend reality". Evidence `apps/api/src/modules/mod.rs:16–21,70–76`, `apps/api/src/routes/settings.rs:36–55`, `apps/api/src/infrastructure/ws/dto.rs:90–101`, `apps/api/src/infrastructure/constants.rs:75–97`, `apps/api/src/worker.rs:47–59`.
**Why (plain words):** A single push-token field per user, socket events with no durable id or read state, a best-effort 50-event buffer and no list/read routes mean notifications are not a feature — they are adapters. A real inbox needs its own table, ids, read state and pagination, or every client will treat a socket event as durable truth.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/api/src/modules/notification/mod.rs` | Create | Rust module | Required | Module root. |
| `apps/api/src/modules/notification/model.rs` | Create | Rust module | Required | Inbox row, read state, recipient. |
| `apps/api/src/modules/notification/dto.rs` | Create | Rust module | Required | List page, unread count, mark-read body. |
| `apps/api/src/modules/notification/service.rs` | Create | Rust module | Required | Create/list/read/unread + token association. |
| `apps/api/src/modules/notification/repository/pg.rs` | Create | Rust module | Required | Cursor `(created_at, id)` descending. |
| `apps/api/src/modules/notification/handlers.rs` | Create | Rust module | Required | HTTP surface. |
| `apps/api/src/routes/notifications.rs` | Create | Rust module | Required | Route wiring. |
| `apps/api/src/routes/mod.rs`, `apps/api/src/modules/mod.rs` | Modify | Rust module | Required | Registration. |
| `apps/api/tests/notification_router.rs` | Create | test | Required | Inbox contract. |

**Pseudo-code**

```
Table notifications
    id (uuid, durable), user_id, kind, title_key/body_key (localisable keys, not sentences),
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

**Depends on:** Step 11.

---

### Step 59: Make the client honest about notifications

**Phase:** FT
**Audit reference:** §3 [H24] "Push and durable notifications are not implemented end-to-end", mobile half; §3 [M16] "Push payloads, channels, and tap lifecycle disagree", Recommended fix (recipient attribution, validated destinations, contract fixtures). Evidence `apps/mobile/lib/features/notification/*`, `apps/mobile/lib/main.dart:6–23`.
**Why (plain words):** With Step 58 the inbox exists; without it, the app must say so instead of showing an empty list that looks like "no notifications". This step wires the coordinator to whichever contract is live and makes the unavailable state explicit.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `apps/mobile/lib/features/notification/manager/notification_coordinator.dart` | Modify | manager | Required | Inbox state, unread count, read actions. |
| `apps/mobile/lib/features/notification/services/notification_http_service.dart` | Create | service | Required | List/read/unread/token association. |
| `apps/mobile/lib/features/notification/pages/_pages.dart` / notification page | Modify | widget | Required | Uses the coordinator; explicit unavailable state. |
| `apps/mobile/test/features/notification/notification_coordinator_test.dart` | Create | test | Required | Read/unread + unavailable. |

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
- Against Step 58, marking read updates the badge and survives a restart.
- The unread count is per account (switching accounts shows the other account's count).
- No page constructs its own unread state.

**Tests:** `apps/mobile/test/features/notification/notification_coordinator_test.dart` — (a) unavailable state, (b) read decrements and reconciles, (c) per-account count, (d) idempotent read.

**Watch out for:** Rendering an empty list when the backend lacks the endpoint. Empty is a claim; say unavailable instead.

**Depends on:** Steps 49, 58.

---

### Step 60: Write the readiness gates for chats, notes, discover and notifications

**Phase:** FT
**Audit reference:** audit §6 "Template Readiness for Upcoming Features" (all four subsections: Backend reality, Carries over, Breaks if copied, Placement, Build first). No individual finding ID — this is the contract gate the audit asks for before UI expansion.
**Why (plain words):** Each upcoming feature has a documented backend reality that differs from the assumed one (broadcast-room chat only, cursor sync with conflicts, no Discover endpoint, no durable inbox). Writing the gate down stops the next feature from copying the current defects.

**Files**

| File (exact path from repo root) | Action | Kind | Required? | Notes |
| -------------------------------- | ------ | ---- | --------- | ----- |
| `docs/feature-readiness.md` | Do not create | doc | Required | Audit §6 already is the readiness document; link to it rather than duplicate it (review OE-19). |
| `apps/mobile/lib/features/chat/services/chat_http_service.dart` | Create | service | Optional | Only the account-bound seam (no UI); skip and chat work starts from the gate doc. Loses only a ready-made constructor wiring. |
| `apps/mobile/lib/features/note/manager/notes_sync_manager.dart` | Create | service | Optional | Registration seam only, added when sync work starts. |

**Pseudo-code**

```
Per feature: gate = list of REQUIRED facts, each with an owner and a status

Chats
    REQUIRED protocol adapter verified (Steps 24 and 52)
    REQUIRED stable message identity + idempotent send contract (documented; server has none today)
    REQUIRED H23 landed (Step 14)
    PLACEMENT account connection + message repository; room controller per open room (KEYED, never one global)
    DO NOT copy per-event whole-feed rebuilds or snapshot-overwrite reconciliation

Notes
    REQUIRED transactional owner-bound storage (Step 38)
    REQUIRED H22 landed (Step 13); PATCH omission semantics documented
    REQUIRED outbox + version/conflict model designed before any editor ships
    PLACEMENT account sync manager (outlives editors); widget-owned editor + feed

Discover
    REQUIRED a real filter/search contract, or an honest "no personalisation" label
    REQUIRED query generations + filtered-page progress (Steps 35 and 36)
    DO NOT emulate ranking client-side

Notifications
    REQUIRED Step 58 inbox + token policy; Step 49 honest state
    PLACEMENT root adapter, account coordinator, widget-owned inbox feed
```

**Done when:**
- Each of the four features links to audit §6 and names the backend contract it waits on.
- Every "Build first" item from audit §6 is referenced, not re-typed.
- No new feature UI is started until its section is fully checked.

**Tests:** none (documentation gate). The linked steps own the executable coverage.

**Watch out for:** Treating the gate document as a formality. Each unchecked line is a feature that will copy a current defect.

**Depends on:** Steps 11-14, 58.

---

## 19. Test plan summary

One row per test file a step's `Tests:` line names. Generated from the steps themselves, so a deleted step cannot leave an orphan file behind, and a merged step lists exactly the files its parts kept (review OE-24).

| Test file | Step | Findings it defends |
| --------- | ---- | ------------------- |
| `apps/mobile/test/_support/fake_secure_storage_test.dart` | 0 | M24, I02 |
| `apps/mobile/test/_support/fake_clock_test.dart` | 0 | M24, I02 |
| `apps/mobile/test/app/account_transition_owner_test.dart` | 1 | C01, H01, M14 |
| `apps/mobile/test/features/auth/session_store_test.dart` | 2 | C02, C03, H10, H25 |
| `apps/mobile/test/features/auth/auth_expiry_test.dart` | 2 | C02, C03, H10, H25 |
| `apps/mobile/test/features/auth/auth_manager_test.dart` | 3 | C03, M02 |
| `apps/mobile/test/features/auth/session_interceptor_test.dart` | 4 | C01, M15, L03 |
| `apps/mobile/test/_shared/services/http_client_test.dart` | 4 | C01, M15, L03 |
| `apps/mobile/test/features/auth/refresh_outcome_test.dart` | 5 | H09, C02 |
| `apps/mobile/test/_core/value_objects/validation_parity_test.dart` | 6 | H25 |
| `apps/api/tests/auth_validators.rs` | 6 | H25 |
| `apps/mobile/test/_shared/model/dtos_test.dart` | 6 | H25 |
| `apps/mobile/test/_core/logging/meno_logger_test.dart` | 7 | M23, C01 |
| `apps/mobile/test/_core/config/config_test.dart` | 8 | M18 |
| `apps/mobile/test/features/broadcast/live_session_store_test.dart` | 9 | M18, H14, M05 |
| `apps/mobile/test/locator_test.dart` | 10 | M22 |
| `apps/mobile/test/app_test.dart` | 10 | M22 |
| `apps/api/tests/auth_router.rs` | 11 | C05, H21 |
| `apps/api/tests/auth_service.rs` | 12 | H09 |
| `apps/api/tests/notes_repository_pg.rs` | 13 | H22 |
| `apps/api/tests/chat_service.rs` | 14 | H23 |
| `apps/mobile/test/features/broadcast/live_exit_fence_test.dart` | 15 | H02, H04 |
| `apps/mobile/test/app/account_gate_test.dart` | 17 | H01, H15 |
| `apps/mobile/test/app/bootstrap_retry_test.dart` | 18 | H01, M24 |
| `apps/mobile/test/_routing/pending_destination_test.dart` | 19 | M19, H01, M16 |
| `apps/mobile/test/_routing/router_guard_test.dart` | 19 | M19, H01, M16 |
| `apps/mobile/test/features/profile/current_user_presentation_test.dart` | 21 | M12, L02 |
| `apps/mobile/test/features/auth/email_verification_manager_test.dart` | 22 | M01, L02 |
| `apps/mobile/test/features/profile/my_profile_manager_test.dart` | 23 | M14, H15 |
| `apps/mobile/test/_shared/services/socket_client_test.dart` | 24 | C04, H06, M10 |
| `apps/mobile/test/features/broadcast/broadcast_socket_service_test.dart` | 24 | C04, H06, M10 |
| `apps/mobile/test/features/broadcast/broadcast_event_decode_test.dart` | 24 | C04, H06, M10 |
| `apps/mobile/test/features/broadcast/active_broadcast_session_test.dart` | 25 | H03, H04, H08, L01, L03 |
| `apps/mobile/test/features/broadcast/broadcast_timer_manager_test.dart` | 25 | H03, H04, H08, L01, L03 |
| `apps/mobile/test/features/broadcast/active_broadcast_manager_test.dart` | 26 | H02, M21 |
| `apps/mobile/test/features/broadcast/live_phase_routing_test.dart` | 27 | H04, M21, L05, M19 |
| `apps/mobile/test/features/broadcast/ended_page_test.dart` | 27 | H04, M21, L05, M19 |
| `apps/mobile/test/features/broadcast/broadcast_contract_test.dart` | 28 | H05 |
| `apps/api/tests/broadcast_contract.rs` | 29 | H05 |
| `apps/api/tests/broadcast_roster_events.rs` | 30 | H16 |
| `apps/mobile/test/features/broadcast/participants_manager_test.dart` | 31 | H16 |
| `apps/api/tests/broadcast_service.rs` | 32 | H17 |
| `apps/mobile/test/features/broadcast/broadcast_editor_manager_test.dart` | 34 | H19, H20 |
| `apps/mobile/test/features/broadcast/idempotency_key_test.dart` | 34 | H19, H20 |
| `apps/mobile/test/_shared/manager/feed_data_source_test.dart` | 35 | H11, M08, I01 |
| `apps/mobile/test/_shared/widgets/feed_widget_test.dart` | 35 | H11, M08, I01 |
| `apps/mobile/test/features/broadcast/filtered_feed_test.dart` | 36 | H12 |
| `apps/mobile/test/features/broadcast/feed_event_binding_test.dart` | 37 | M09 |
| `apps/mobile/test/features/broadcast/broadcast_local_service_test.dart` | 38 | M05, M06 |
| `apps/mobile/test/features/profile/profile_cache_test.dart` | 39 | H14 |
| `apps/mobile/test/features/profile/user_profile_repository_test.dart` | 40 | H13 |
| `apps/mobile/test/features/profile/user_profile_page_test.dart` | 41 | M04 |
| `apps/mobile/test/features/profile/profile_editor_manager_test.dart` | 42 | M11 |
| `apps/api/tests/settings_service.rs` | 43 | M13 |
| `apps/mobile/test/features/settings/settings_manager_test.dart` | 44 | M13 |
| `apps/mobile/test/features/broadcast/favourites_manager_test.dart` | 46 | M07 |
| `apps/mobile/test/features/_schema/schema_ids_test.dart` | 47 | I02 |
| `apps/mobile/test/features/auth/account_connection_owner_test.dart` | 48 | H06, M03 |
| `apps/mobile/test/_shared/services/push_payload_test.dart` | 49 | M16, H24 |
| `apps/mobile/test/features/broadcast/live_reconnect_test.dart` | 50 | H07, M03 |
| `apps/api/tests/ws_protocol.rs` | 52 | C04, H07 |
| `apps/mobile/test/features/broadcast/home_navigation_test.dart` | 54 | M20, M22, L05 |
| `apps/mobile/test/features/broadcast/ui_copy_contract_test.dart` | 56 | M26, L04 |
| `apps/mobile/test/features/settings/settings_page_test.dart` | 57 | L06 |
| `apps/api/tests/notification_router.rs` | 58 | H24 |
| `apps/mobile/test/features/notification/notification_coordinator_test.dart` | 59 | H24, M16 |

Generated from the steps' own `Tests:` lines, so a deleted step cannot leave an orphan row behind.

**Conventions.** New and repaired tests use Mocktail; collaborators are concrete fakes in `test/_support/` unless a platform type forces a mock (Step 55). One file defends one claim. A race test holds a `Completer` and asserts ordering, never a `Duration` sleep.

**Validation sequence for every phase exit:** focused unit suites → widget/lifecycle suites → `flutter test` (full mobile) → `dart analyze` (must stay clean) → `cargo test` for backend steps → the device lines in `docs/release-checklist.md` for Phase D. Never loosen lint rules, never replace protocol validation with the silent transport.

---

## 20. What we are deliberately NOT doing

Consolidated, mapped to audit §4 and to the review's simplification flags. Each line names what is rejected and why; the step that would otherwise have carried it says `Simplified …`.

| Not doing | Why |
| --------- | --- |
| A `live:<id>` DI scope in any form | Registration existence became a state protocol; replaced by one manager with a nullable session. |
| A private per-session `GetIt` container | The audit's comparison rejects it: too few collaborators to justify the lifecycle. |
| Any third application DI scope | Root plus zero-or-one authenticated only. |
| An interface per new service | Only `RoomTransport` and `TokenBlocklist` have real second implementations. |
| An event bus or generic session framework | §1.4 "Generalization": wait until behaviour is genuinely shared. |
| Nested command pipelines for internal sequencing | §4: commands are UI adapters; lifecycle uses plain async methods. |
| A persisted generation, transition marker or migration state | A generation only fences work inside one process; nothing survives a kill, so persisting it protects nothing (OE-1). |
| Read-back write verification and a global storage gate | Defends against a keystore that lies — no finding describes it; one failed write would lock the whole app (OE-2). |
| Legacy credential migration and upgrade-from-installed-build steps | No build has shipped (A9); the first release performs the legacy-key delete in Step 2 and records the gap in Step 53. |
| An unmount hub with acknowledgements and a timeout | The frame boundary already says the old subtree is gone (OE-4). |
| A bootstrap attempt ledger that tracks native resources | One failure mode does not need a resource-tracking framework (OE-5). |
| A notification-tap queue with expiry, dedupe and an account chooser | A product feature, not a routing fix; one slot that refuses a foreign account does the safety part (OE-6). |
| An exit-channel capability and a temporary exit adapter | The same idempotent close implemented three times; the owner calls the manager directly (OE-7). |
| A second attempt-ticket counter in `AuthManager` | One rule, one counter: the owner's (OE-8). |
| `UserManager` binding into account-scoped objects | Signed-in chrome watches `MyProfileManager` directly; `UserManager` keeps only the signed-out greeting (OE-9). |
| Stored-outcome replay for refresh rotation | Contradicts "uncertain means re-authenticate"; distinct error codes and a 429 are enough (OE-10). |
| freezed for payload-free phases and every union | Enums and hand-written sealed classes are smaller and need no generation (OE-11). |
| A generic `AccountLocalData` layer and `quiesce()` | Three typed, owner-bound services already do it; disposal already quiesces (OE-12). |
| A public/relationship profile-cache split *and* viewer keying | H14 offers one or the other; one key per `(viewer, subject)` is enough (OE-13). |
| A settings version column, 409 protocol and client overlay | Field-level PATCH means an old client cannot delete a key it never sent (OE-14). |
| Restarting the socket on every token refresh | A WebSocket authenticates at handshake; read the credential on connect only (OE-15). |
| Per-token roster fan-out to every listener | O(n²) churn; hosts get deltas, listeners poll a count (OE-16). |
| A `StreamManager` that only forwards `join()` | Callers use the manager (OE-17). |
| Redirect re-validation, a foreground retry budget, `Retry-After` parsing on the client | The audit asks for jitter order and a token that never leaves the approved origin (OE-18). |
| Integration tests for secure storage, upgrade and ObjectBox; four device documents | One `docs/release-checklist.md` carries the same evidence and gets maintained (OE-19). |
| Building the notification feature inside the remediation | It needs a new backend module; it lives in the Feature track (OE-20). |
| A client-side filtered paging loop when the backend can filter | Step 36 checks first and deletes itself if the server can (OE-21, A-9). |
| A backend idempotency rewrite | The middleware already stores outcomes; only the client was missing keys (OE-25). |
| A CI pipeline | None exists (A13); a script plus an optional pre-push hook does the same job for one developer (OE-23). |
| A feature-flag system for unfinished screens | Hide or disable the control; a flag framework is a new mechanism (OE-19). |
| Numbered-page pagination in the feed base | I01: nothing uses it; cursor only. |
| A shared empty-identity profile proxy | §4: an ID-bearing placeholder is simpler and correct. |
| Re-registering credentials as a mutable singleton to "refresh" the socket | M03: a credential supplier plus a serialized handoff. |
| Automatic retry of refresh, ambiguous mutations, or consumed streams | H09/M15/H20: uncertainty is surfaced, not retried. |
| Backend recovery of an uncertain rotation via token reuse | Design §5.5: re-authentication by default; a recovery endpoint is a separate contract. |
| Keeping legacy credentials "because they look valid" | C03/H10: nothing old is usable. |
| Persisting LiveKit tokens or notification payloads | M18/design §5.9: minimal non-secret intent, in-memory destinations. |
| Navigating from platform callbacks | M16/M19: one validated slot, consumed after readiness. |
| A fourth attempt at Socket.IO compatibility, or emulated acknowledgements | C04: the backend speaks raw WebSocket JSON; missing replay is a documented limitation. |
| Adding `creatorName` to the end response to satisfy the old client | M12: display identity has exactly one source. |
| Rotating or upgrading ObjectBox off the preview in this work | I02: the risk is documented and gated, not silently changed. |
| An external crash/analytics SDK | M23/design §5.13: sanitised platform logging until a retention decision exists. |
| A localization framework adoption | L06's alternative: label the limitation, keep copy in one block per screen (Step 57). |
| Rewriting the strict analyzer baseline | §5 item 10: no reason to weaken it. |
| Rewriting `RoomTransport`, constructor injection, one ObjectBox handle, typed errors, widget-owned feeds | §5: these are the patterns to keep. |
| `Media3` or a replacement media stack | M17's reference supports ownership, not a library swap. |
| Cross-client settings linearizability | M13: field-level patches remove the deletion bug; true linearizability is not required. |
| Store listing, marketing, accessibility certification | Out of scope; accessibility needs device semantics, contrast and large-text testing. |

---

## 21. Final Definition of Done

Each line is an acceptance check, not a claim. Tick only with a test name, a device line or a reviewed diff behind it.

- [ ] **Traceability intact.** Every ID maps to a step, and every step claiming an ID has a test or a device line. Backend prerequisites (C05, H17, H18, H21, H22, H23 and the backend halves of H05, H09, M13) are done only with `cargo test` evidence. *(C01–I02)*
- [ ] **Identity invariant.** No request, retry or replay initiated as A can execute as B, proven with held completers and a switch in between. *(C01, M15)*
- [ ] **Vault invariant.** One serialized queue, one versioned record, conditional writes, absolute expiry, fail-closed decoding. *(C02, H10)*
- [ ] **Logout invariant.** Credentials are removed locally regardless of the server outcome; a failed removal shows one retry state, and local resources still close. *(C03)*
- [ ] **Rotation invariant.** Four classified outcomes; a possibly-consumed token is never retried; a crash mid-refresh forces a login. *(H09)*
- [ ] **Two tiers only.** Exactly `root` and zero-or-one `authenticated`; no live/page/startup scope, no session readiness signalling. *(H02, M21)*
- [ ] **Readiness invariant.** The first authenticated build waits for `phase == ready`; the old subtree is disposed before the scope pops; watches and feeds remount by generation. *(H01, H15)*
- [ ] **Live ownership invariant.** One account-owned manager, one nullable session, one idempotent close, capabilities separate from phase, end summary kept outside the resources. *(H02–H04, H08, M21)*
- [ ] **Connection invariant.** Handlers installed before connect, the credential read at connect, start after verified readiness, stop awaited before disposal. *(C04, H06, H07, M03)*
- [ ] **Realtime verified (or visibly unavailable).** Raw-WebSocket framing, heartbeat, bounded reconnect and terminal reasons proven against the real backend. *(C04, H07)*
- [ ] **Local data invariant.** Every private read/write/delete is owner-qualified by construction; logical keys are unique; queries always close. *(M05, M06)*
- [ ] **Cache invariant.** Viewer-relative state is keyed by viewer and subject; no viewer-less read exists. *(H14)*
- [ ] **Profile invariant.** Stable repository identity with working reference counts; the editor compares like with like; one current-user presentation source. *(H13, M11, M12, M04)*
- [ ] **Settings invariant.** Only changed fields are sent, edits serialize, and unknown keys survive end to end. *(M13)*
- [ ] **Feed invariant.** Query generations, refresh/page arbitration, filtered-page progress, separate page errors, safe rebinding, cursor-only base. *(H11, H12, M08, M09, I01)*
- [ ] **Resend invariant.** Only a tap sends a code; availability derives from cooldown and running state; one error surface. *(M01, L02)*
- [ ] **Validation parity.** One checked-in fixture set proves client and server accept and reject the same inputs; server-valid values persist without throwing. *(H25)*
- [ ] **Routing invariant.** One validated pending destination, consumed after readiness, refused when addressed to another account. *(M19, M16)*
- [ ] **Release gate.** `docs/release-checklist.md` passes on a signed build, on a named device, with a date. *(M25, M17, C02, C03, M18)*
- [ ] **Schema gate.** ObjectBox UIDs unchanged, one handle, and the preview risk documented before release. *(M06, I02)*
- [ ] **Reference quality.** No commented-out suites, Mocktail for new tests, the import script passing, no enabled no-ops, an honest language setting, README matching the code. *(M20, M22, M24, M26, L01, L03, L04, L05, L06)*
- [ ] **Logging.** Production warnings and errors reach a sink with allowlisted fields; sentinel-secret tests pass at every level. *(M23)*
- [ ] **Nothing from audit §5 was rewritten.** Constructor injection, `RoomTransport`, widget-owned feeds, separated refresh/pagination with length-3 prefetch, success observation, optimistic overrides, media cancellation, typed errors, strict analyzer. *(keep-list)*
- [ ] **No mechanism without a finding.** Every class added by this guide names the finding it defends in its step's audit reference.

---

## Coverage Checklist

All 64 findings, each ticked only when its step's `Done when` and tests pass. Generated from the steps' own audit references, so this list and the traceability table cannot disagree.

**CRITICAL (5)**
- [ ] C01 — Requests can cross account boundaries during replay — Steps 1, 4, 7
- [ ] C02 — Session-store mutations are neither atomic nor identity-conditional — Steps 2, 5, 20, 53
- [ ] C03 — Explicit logout can leave a usable session recoverable — Steps 2, 3, 20, 53
- [ ] C04 — Mobile and backend use incompatible realtime protocols — Steps 24, 52
- [ ] C05 — Backend access-token revocation is not enforced by the actual HTTP guard — Steps 11

**HIGH (25)**
- [ ] H01 — Authenticated UI is not synchronized with account-scope readiness — Steps 1, 16, 17, 18, 19
- [ ] H02 — Live transitions can outlive account teardown or supersede newer sessions — Steps 15, 16, 26
- [ ] H03 — Session disposal can access an uninitialized timer — Steps 25
- [ ] H04 — Failed end/leave skips session closure; ended UI depends on disposed resources — Steps 15, 25, 27
- [ ] H05 — Broadcast end and cohost-removal contracts are wrong — Steps 28, 29
- [ ] H06 — No production path starts realtime delivery — Steps 24, 48
- [ ] H07 — Reconnection has stale credentials, conflicting state, and ignored terminal reasons — Steps 50, 52
- [ ] H08 — Publishing permission is mistaken for creator authority — Steps 25
- [ ] H09 — Refresh failures destroy sessions, including ambiguous rotated-token outcomes — Steps 5, 12
- [ ] H10 — Stored access-token expiry moves forward on every read — Steps 2
- [ ] H11 — Refresh, pagination, query changes, and invalidations race — Steps 35
- [ ] H12 — Client-filtered empty pages strand pagination — Steps 36
- [ ] H13 — Cold profile proxies share an empty identity and cannot be released correctly — Steps 40
- [ ] H14 — Viewer-specific profile cache state leaks across accounts — Steps 9, 39
- [ ] H15 — Async operations continue after their owner is disposed — Steps 17, 23, 45
- [ ] H16 — Participant UI does not implement the backend roster contract — Steps 30, 31
- [ ] H17 — Backend cohost removal does not reliably revoke role or presence — Steps 32
- [ ] H18 — Backend broadcast transitions are not safely retryable — Steps 33
- [ ] H19 — Editor progress and retries can report success without completing entry — Steps 34
- [ ] H20 — Broadcast mutations do not carry stable logical-operation idempotency keys — Steps 34
- [ ] H21 — Password-reset requests revoke sessions before proving ownership — Steps 11
- [ ] H22 — Notes sync can lose folders created in the same batch — Steps 13
- [ ] H23 — Chat deletion can mutate one room and publish to another — Steps 14
- [ ] H24 — Push and durable notifications are not implemented end-to-end — Steps 49, 58, 59
- [ ] H25 — Client validation rejects server-valid accounts and can break credential storage — Steps 2, 6

**MEDIUM (26)**
- [ ] M01 — Email resend availability drives an automatic resend loop — Steps 22
- [ ] M02 — Add-account flow retains password-only login mode — Steps 3, 20
- [ ] M03 — Scoped credentials and socket tokens stay stale — Steps 48, 50
- [ ] M04 — Public profile page resolves an unregistered proxy — Steps 41
- [ ] M05 — Local storage isolation is optional rather than enforced — Steps 9, 38, 53
- [ ] M06 — ObjectBox writes can duplicate logical rows; queries lack deterministic cleanup — Steps 38, 53
- [ ] M07 — Favourites are disconnected snapshots — Steps 46
- [ ] M08 — Pagination errors are absent from the feed error surface — Steps 35
- [ ] M09 — Socket rebinding cancels the wrong handles; feed events ignore filters — Steps 37
- [ ] M10 — Invalidation parsing and event enrichment are inconsistent — Steps 24
- [ ] M11 — Profile editor compares different types and ignores bio validity — Steps 42
- [ ] M12 — Current-user presentation has multiple unsynchronized sources — Steps 21
- [ ] M13 — Settings writes race and overwrite unknown preference keys — Steps 43, 44
- [ ] M14 — Account-scoped managers redundantly track global account selection — Steps 1, 16, 23
- [ ] M15 — HTTP failure paths can hang, misclassify, or leak credentials to external origins — Steps 4
- [ ] M16 — Push payloads, channels, and tap lifecycle disagree — Steps 19, 49, 53, 59
- [ ] M17 — Background-audio support is declared but not established — Steps 51
- [ ] M18 — Production can select a successful silent transport; recovery stores a bearer token — Steps 8, 9, 53
- [ ] M19 — Routing discards identity and intended destinations — Steps 19, 27
- [ ] M20 — Named navigation is used for a route without that name — Steps 54
- [ ] M21 — Scope/connection orchestration is over-commanded — Steps 26, 27
- [ ] M22 — Shared infrastructure imports features and broad barrels hide the dependency graph — Steps 10, 54
- [ ] M23 — Production logging suppresses every severity — Steps 7
- [ ] M24 — Critical lifecycle tests are commented out — Steps 0, 18, 53, 55
- [ ] M25 — Build/platform configuration is not release-ready — Steps 51
- [ ] M26 — UI presents incomplete behavior as operational — Steps 56

**LOW (6)**
- [ ] L01 — Rebuild granularity is too broad for a one-second clock — Steps 25
- [ ] L02 — Derived observables are recreated by getters — Steps 16, 20, 21, 22
- [ ] L03 — Timer semantics and retry timing are inaccurate — Steps 4, 25
- [ ] L04 — Small UI/validation mismatches remain — Steps 56
- [ ] L05 — Dead paths, fixtures, and documentation obscure the reference implementation — Steps 27, 54
- [ ] L06 — Language setting is not connected to localization — Steps 57

**INFO (2)**
- [ ] I01 — The feed base mixes unused page arithmetic with cursor pagination — Steps 35
- [ ] I02 — Dependency risk is explicit, not proven obsolescence — Steps 0, 47, 53, 55

**Total: 5 + 25 + 26 + 6 + 2 = 64 findings, 64 checkboxes. 64/64 covered by at least one step.**

---

## Appendix A — Step index

Fast lookup. Phase, title, dependencies.

| Step | Phase | Title | Depends on |
| ---- | ----- | ----- | ---------- |
| 0 | P0 | Tooling and safety net | nothing |
| 1 | P1 | Add the account identity value types | Step 0 |
| 2 | P1 | Write credentials through one serialized, conditional vault | Step 1 |
| 3 | P1 | Make logout durable and separate it from deactivate | Step 2 |
| 4 | P1 | Bind every request to the identity that started it | Steps 2, 3 |
| 5 | P1 | Classify refresh outcomes and never retry a possibly-consumed token | Step 4 |
| 6 | P1 | Align client validation with the server and stop validating during serialization | Steps 1, 2 |
| 7 | P1 | Report production failures safely and stop the blanket sign-out | Steps 1, 4 |
| 8 | P1 | Make production configuration fail loudly instead of silently | Step 7 |
| 9 | P1 | Stop keeping secrets and viewer-blind caches outside the vault | Step 2 |
| 10 | P1 | Move auth persistence out of _shared | Steps 2, 4 |
| 11 | B0 | Backend: enforce account-wide revocation and prove reset ownership | nothing (backend repo) |
| 12 | B0 | Backend: keep refresh rotation outcome-safe without stored replays | Step 11 |
| 13 | B0 | Backend: resolve note folder ownership after folder mutations | nothing (backend repo) |
| 14 | B0 | Backend: guard chat deletion by room as well as sender | nothing (backend repo) |
| 15 | P2 | Fence the old live tier so it cannot outlive a transition | Steps 3, 4 |
| 16 | P2 | Replace UserScopeManager with one transition owner | Steps 2, 3, 15 |
| 17 | P2 | Gate the account subtree by generation | Step 16 |
| 18 | P2 | Make startup retry rebuild what failed | Steps 8, 16 |
| 19 | P2 | Refresh the router on identity and keep one pending destination | Steps 16, 17 |
| 20 | P2 | Reduce AuthManager to a UI facade with attempt tickets | Steps 16, 3 |
| 21 | P2 | Give the current user exactly one presentation source | Steps 20, 16 |
| 22 | P2 | Send verification codes only when the user taps | Step 20 |
| 23 | P2 | Inject immutable account context into account-scoped managers | Steps 16, 21 |
| 24 | R | Replace the Socket.IO client with a raw-WebSocket client | Step 4 |
| 25 | P3 | Create the session's resources with the session and close them once | Step 23 |
| 26 | P3 | Build ActiveBroadcastManager as the single live owner | Step 25 |
| 27 | P3 | Switch the app to the manager and delete the live tier | Step 26 |
| 28 | B | Fix the end and cohost-removal contracts on the client | Step 25 |
| 29 | B | Make the backend end/cohost contract explicit and fixture-tested | nothing (backend repo) |
| 30 | B | Publish roster events to the people who need them | nothing (backend repo) |
| 31 | B | Page and reconcile the roster on the client | Steps 26, 30 |
| 32 | B | Backend: revoke cohost role, presence and media permission atomically | Step 30 |
| 33 | B | Backend: make broadcast start/join retry-safe | Step 29 |
| 34 | B | Make the editor create → upload → start → entry one operation | Steps 26, 27, 33 |
| 35 | F | Capture immutable query generations and give page failures their own surface | Step 4 |
| 36 | F | Keep paging through client-filtered pages only if the backend cannot filter | Step 35 |
| 37 | F | Rebind subscriptions safely and respect query predicates on events | Steps 24, 35 |
| 38 | P4 | Bind local storage to the account at construction | Steps 16, 23 |
| 39 | P4 | Key the profile cache by viewer and subject | Step 38 |
| 40 | P4 | Give the profile repository a stable subject identity | Steps 39, 38 |
| 41 | P4 | Target the page's own proxy in the public profile page | Step 40 |
| 42 | P4 | Compare and validate the profile editor correctly | Step 6 |
| 43 | P4 | Backend: patch only the settings fields the client sends | nothing (backend repo). Land it in the same change as Step 44: the client that sends patches and the endpoint that merges them are one contract |
| 44 | P4 | Serialize settings edits and send only what changed | Steps 23, 38. Land it with Step 43 |
| 45 | P4 | Fence profile and settings continuations against owner death | Steps 40, 42, 44 |
| 46 | P4 | Back favourites with one account-level reactive source | Step 38 |
| 47 | P4 | Record the ObjectBox preview risk and gate native schema changes | Step 38 |
| 48 | P5 | Add the account connection owner | Steps 27, 5 |
| 49 | P5 | Fix the push payload and install platform adapters at the root | Steps 19, 48 |
| 50 | P5 | Reconcile reconnection into one phase and honour terminal reasons | Steps 24, 26, 48 |
| 51 | D | Make the release build releasable and establish background audio | Step 27 |
| 52 | D | Validate the realtime protocol against the real backend | Steps 24, 50, 51 |
| 53 | D | Prove the release gates with one checklist | Steps 2, 3, 9, 47, 51 |
| 54 | C | Navigate by routes that exist and make the import graph one-way | Steps 16, 27, 38 |
| 55 | C | Reinstate the lifecycle tests that were commented out | Steps 16, 21, 40 |
| 56 | C | Stop presenting unimplemented behaviour as working | Steps 31, 42 |
| 57 | C | State the localization limitation and keep each screen's copy in one place | Step 44 |
| 58 | FT | Give notifications a durable inbox and a device-token policy | Step 11 |
| 59 | FT | Make the client honest about notifications | Steps 49, 58 |
| 60 | FT | Write the readiness gates for chats, notes, discover and notifications | Steps 11-14, 58 |

---

*End of guide. Source of truth for what changes: [docs/audit-report.md](audit-report.md). Reference design for the account/DI slice: [docs/account-di-migration.md](account-di-migration.md). Review this revision applies: `Implementation_Guide_Review_Alignment__Drift_and_Over-Engineering.md`.*
