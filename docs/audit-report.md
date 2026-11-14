I’ll load the seven required skills first, then map the code and trace the scope lifecycles before doing the broader review. This will be read-only; findings will distinguish verified defects from risks that need runtime verification.

The scope assessment is complete. The code currently has `root → authenticated → live_<id>`, not separate root-session and user scopes. Account-scoped DI is appropriate, but runtime account changes are not gated on scope readiness, and the live scope mixes registration state with connection state. I also found an uninitialized-timer teardown path and separate account/live transition queues that can overlap.

I’ll now review the remaining feature code and backend contracts, using those lifecycle findings as the baseline.

The broader audit has found issues more serious than scope complexity: the mobile Socket.IO client does not match the backend’s raw-WebSocket protocol, and a 401 response can replay an account A request using account B’s credentials after a switch. The backend also lacks a durable notification inbox, despite the context describing notifications as implemented.

I’m checking the key paths and consolidating the findings so the report separates template blockers from incomplete UI and lower-risk cleanup.

## 1. Scope Design Assessment (FIRST SECTION OF THE REPORT)

**Review basis:** read-only source inspection of the mobile application, configuration, platform integration, tests, and relevant backend contracts. All seven required skills were loaded first and used as the rubric.

**Validation boundary:** no builds, analyzer runs, tests, device runs, or performance profiles were executed; no execution tool was available. Findings below distinguish source-established defects from items requiring runtime verification. No files were modified.

**Important correction to the supplied context:** the current code defines `root → authenticated → live:<broadcastId>`. It does **not** define separate `root-session` and `user_scope` scopes. See `meno/apps/mobile/lib/_core/config/config.dart:49–63`.

### 1.1 Scorecard

| Scope level              | Score /10 | Verdict         | Best suited for—and current fit                                                                                                                                                               |
| ------------------------ | --------: | --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Root                     |         7 | Keep with fixes | Account-independent infrastructure and coordination. Mostly used appropriately; authenticated HTTP behavior needs identity isolation.                                                         |
| Auth/session             |         5 | Keep with fixes | Session selection, credential renewal, and account-transition coordination. These are root-manager responsibilities here, not a distinct scope. Coordination is incomplete.                   |
| Account: `authenticated` |         6 | Keep with fixes | Resources belonging to the **active account**. Correct boundary for multi-account switching, but readiness, identity binding, and teardown need repair.                                       |
| Broadcast: `live:<id>`   |         3 | Redesign        | A disposable collection of session resources. Valid in principle, but currently doubles as connection state, navigation readiness, and session identity, producing unnecessary failure modes. |

The scores describe implementation quality, not whether multi-account scoping is inherently appropriate.

### 1.2 Evidence and failure scenarios per level

#### Root scope

**Observed registrations**

`configureDependencies()` pushes `Scope.root`, then registers infrastructure, platform services, onboarding, auth coordination, appearance, and the router:

- Push and registration order: `meno/apps/mobile/lib/locator.dart:40–49`.
- Configuration, logger, secure storage, ObjectBox, preferences, HTTP, media: `locator.dart:52–81`.
- Permissions, push, local notifications: `locator.dart:89–100`.
- Auth store and managers: `locator.dart:113–142`.
- Theme and router: `locator.dart:400–411`.
- Bootstrap occurs before `runApp()`: `meno/apps/mobile/lib/main.dart:6–23`.

**What this scope is best suited for:** app-lifetime, account-independent capabilities and the coordinator that selects the active account.

**Is it being used for that?** Mostly yes.

A shared ObjectBox handle, secure credential vault, configuration, media picker, and root auth coordinator belong here. A root HTTP transport is also acceptable **if requests carry immutable account identity**. The current interceptor instead consults mutable active-account storage during dispatch and replay.

**Concrete failures**

1. Account A sends a request; the user switches to B; A’s 401 is replayed with B’s credentials. Root HTTP lifetime is not the problem—the mutable identity lookup is.
2. Secure-storage operations can interleave, so the root vault is not actually a single serialized writer.
3. Startup retry only repeats readiness waiting; it does not recreate a failed service. `MenoApp._retry()` therefore cannot reliably recover failed initialization.

The intentional choice to retain the last theme after logout is not an account-isolation defect. It is documented presentation policy.

#### Auth/session coordination

There is **no separate auth/session scope**. `AuthManager`, `SessionStore`, `UserManager`, and `UserScopeManager` live in root.

**Lifecycle call sites**

- Restore: `AuthManager.init()` → `_restore()`, `meno/apps/mobile/lib/features/auth/manager/auth_manager.dart:202–205,318–340`.
- Login adoption: `_accept()`, same file:249–257.
- Logout: `_logout()` → `signOut()`, same file:243–245,281–299.
- Account switch: `_switchAccount()`, same file:352–359.
- Scope transition subscription and serialized queue: `meno/apps/mobile/lib/features/auth/manager/user_scope_manager.dart:56–78,87–123`.
- Teardown: `_popScope()` uses `popScopesTill(root, inclusive:false)`, same file:139–142.
- Initial readiness gate: `meno/apps/mobile/lib/app.dart:106–109`.

**What this level is best suited for:** selecting the active account and coordinating credentials, account resources, and UI transition readiness.

**Is it being used for that?** Partially. Holding auth state in a root manager is appropriate; another scope would not fix the defects.

**Concrete failures**

1. Login publishes authenticated state before account-scope creation finishes. The router can redirect into pages resolving account managers before registration completes.
2. A→B switching leaves `isAuthenticated` and `emailVerified` unchanged. The root router refresh listens to those booleans—not account identity or account readiness.
3. An explicit logout leaves credentials in the vault. After a failed/offline server logout, selecting the retained account can restore the supposedly logged-out session.
4. A refresh completing after switching or logout can clear the wrong active account or restore credentials that should have been invalidated.

**Judgment:** keep auth as root-held state and coordination. Add a transition state and account/session generation, not another DI tier.

#### Account scope: `authenticated`

**Observed registrations**

`_registerAccountScope()` registers:

- Account-authenticated socket and broadcast socket adapter.
- Broadcast HTTP/local services, editor, favourites, lobby manager.
- Profile HTTP/cache/repository/current-profile manager.
- Settings services/manager.
- Live-session recovery store and live-scope coordinator.

Evidence: `meno/apps/mobile/lib/locator.dart:156–299`.

**Creation and disposal**

- Created with the selected `UserCredential`: `user_scope_manager.dart:116–122`.
- Account changes are serialized within that manager: same file:95–99.
- Account removal pops all descendants: same file:139–142.
- Most owned managers implement `Disposable`; socket and transport registrations have disposal callbacks.

**What this scope is best suited for:** resources and state owned by the active account: authenticated connection, private caches, repositories, sync, settings, active media ownership.

**Is it being used for that?** Yes in placement; not yet reliably in operation.

**Multi-account semantics**

The app supports multiple **stored sessions**, with one active account scope. It does not maintain simultaneous live DI graphs/connections for all accounts. That is a reasonable design unless simultaneous background operation for inactive accounts becomes a requirement.

**Concrete failures**

1. Existing account widgets are not keyed/remounted by account generation. Their watches and `createOnce()` feeds can retain old managers after those managers are disposed.
2. Old account teardown can issue REST calls after the active credential has already changed to B. Current HTTP authentication can therefore execute A’s cleanup as B.
3. `MyProfileManager` and `SettingsManager` still follow the global active account despite being account-scoped. During transition, an old manager can start loading the next account just before disposal.
4. Same-account renewal is skipped when a child scope is on top: `_replaceCredential()` returns unless the current scope is `authenticated`. It is never repaired merely by closing that child.
5. `SocketClient` captures the credential passed to the registrar, so replacing `UserCredential` does not update its token.

**Will this scale?** The boundary will. Eagerly starting every feature’s network activity will not. Register feature services lazily; eagerly start only deliberate account-wide owners such as notification coordination, sync scheduling, and active-session recovery.

#### Broadcast scope: `live:<id>`

**Creation and registration**

- Entry: `LiveScopeManager.start()` → queued `_open()`, `meno/apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart:155–160,314–340`.
- Push: same file:322–334.
- Conditional registration based on whether a token-bearing session exists: `meno/apps/mobile/lib/locator.dart:320–389`.
- Start/join callers:
  - `BroadcastEditorManager.startBroadcast`, `broadcast_editor_manager.dart:68–84`.
  - `StreamManager.joinBroadcast`, `stream_manager.dart:34–46`.

**Disposal**

- Explicit close: `LiveScopeManager._close()` → `dropScope()`, `live_scope_manager.dart:344–362`.
- Account teardown also removes it through `popScopesTill`.
- Session command attempts to initiate its own scope closure: `live_session_manager.dart:119–145`.
- Session disposal invokes that same UI-facing command: same file:388–426.

**What this scope is best suited for:** grouped objects with a common lifetime, where existence genuinely means “this resource bundle is owned.”

**Is it being used for that?** Only partly. A tokenless scope, a registered-but-not-connected session, a connected room, and an ended session are different states, but registration existence is repeatedly used to distinguish them.

**Early-registration/banner assessment**

The current banner has already addressed the simplest early lookup:

- It watches account-scoped `LiveScopeManager.activeBroadcastId`.
- It uses `maybeGet<LiveSessionManager>()`.
- The ID is published after scope registration.

Evidence: `meno/apps/mobile/lib/features/broadcast/widgets/live_session_banner.dart:34–47`; `live_scope_manager.dart:335–336`.

Therefore, **the current banner does not unconditionally resolve an absent session at first build**. Do not report that historical bug as unchanged.

However:

- It still needs a conditional DI lookup plus three session observables.
- It hides itself for disconnected/host-disconnected states.
- Its confirmation dialog retains a session-manager reference across an await.
- Session replacement and account switching can dispose the referenced object.
- Scope disappearance is not itself an observed state in `LiveSessionShell`; checking `isRegistered` during build does not schedule that build.

**Concrete exit-path failures**

| Exit path                                 | Current behavior and defect                                                                                                                      |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| User ends/leaves                          | REST failure runs local disconnect in `finally`, but skips `_live.end()` after it. Scope and recovery record remain.                             |
| Scope closes before entry page runs setup | `timer` is `late final`; both `endSession` and disposal access it before initialization.                                                         |
| Host-ended event                          | Account coordinator drops scope while the session listener also updates state/disconnects. Ended page then tries to resolve the removed manager. |
| Logout/account switch during opening      | Account and live transitions use separate queues. Live disposal does not drain/invalidate an in-flight `_open()`.                                |
| Network loss                              | Resources intentionally remain for reconnect; that is reasonable. Retry and terminal-failure behavior are inconsistent, detailed below.          |
| Process crash/background kill             | No Dart scope can guarantee disposal. Recovery and backend presence/timeout behavior must handle it.                                             |
| Recovery overlaps a new join              | `_recover()` has a disposed check but no session-operation generation. A late recovery can replace a newer session.                              |

**Cost judgment:** get_it lookup cost is not the issue. The cost is lifecycle coupling, conditional registration, readiness signaling, scope polling, self-disposal, and difficult race testing.

### 1.3 Options comparison for broadcasting/streaming

| Option                                                  | Complexity     | Performance                                                  | Testability                                       | Scalability                                                        | Failure-mode safety                                                           | Migration cost |
| ------------------------------------------------------- | -------------- | ------------------------------------------------------------ | ------------------------------------------------- | ------------------------------------------------------------------ | ----------------------------------------------------------------------------- | -------------- |
| Keep per-broadcast scope, fix lifecycle                 | Medium–high    | Fine; scope lookup overhead is negligible                    | Requires scope setup for lifecycle tests          | Suitable for several disposable collaborators                      | Good only with one transition owner, stable external facade, and cancellation | Low–medium     |
| **Account manager with nullable active session**        | **Low–medium** | **One stable observable; resources exist only while active** | **Manager/session fakes without real scope tree** | **Fits one active audio session and account-wide realtime owners** | **Explicit ownership, generations, idempotent close**                         | **Medium**     |
| Account owner plus private per-session `GetIt` instance | High           | No material advantage here                                   | Isolated container possible, but more setup       | Useful only for a large replaceable plugin graph                   | Avoids global stack interference, adds container lifecycle                    | High           |
| General-purpose session framework/state-machine library | High           | No demonstrated gain                                         | More abstractions to fake                         | Premature for the actual feature set                               | Depends on implementation; not inherently safer                               | High           |

**Stronger alternative considered:** a private session container avoids global scope-stack interference, but this session has too few collaborators to justify it.

### 1.4 Recommended approach

#### Recommendation

**Keep the account scope. Replace the broadcast DI scope with one account-scoped `ActiveBroadcastManager` holding a nullable owned session.**

Retain the useful external boundary, `RoomTransport`. Let the session own:

- Its `RoomTransport`.
- Participant reconciliation/subscriptions.
- Media-related timers/subscriptions.
- Immutable account identity and session generation.

Let the manager own:

- Serialized enter/replace/close.
- Recovery.
- One immutable observable snapshot for global UI.
- Terminal result/summary independent of the disposed session.
- User-facing commands.

This is not a recommendation to flatten everything into one god manager. Transport and roster remain focused collaborators; they do not need globally registered per-room identities.

**Why it wins:** the application already needs an account-lifetime owner outside the live page. Making that owner directly own the session removes an entire registration-state protocol without removing necessary resource boundaries.

#### State machine

Use one authoritative session phase, not independently mutable `state`, `status`, `_isLeaving`, `_hasSession`, and active ID.

| State          | Trigger / next transition                                                                                                                                |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `idle`         | No session. Start/join/recover → `connecting`.                                                                                                           |
| `connecting`   | Obtain/accept session, create transport, connect, reconcile roster. Success → `live`; recoverable failure → `reconnecting`; terminal failure → `failed`. |
| `live`         | Media connected. SDK reconnecting → `reconnecting`; leave/end/remote termination → `ending`.                                                             |
| `reconnecting` | SDK owns its reconnect attempt first. Success → `live`; exhausted/terminal rejection → `failed`; user exit → `ending`.                                   |
| `ending`       | Reject new session mutations; cancel retry work; close local resources regardless of REST outcome.                                                       |
| `ended`        | Immutable end reason/summary retained outside the resource session. Acknowledge → `idle`; new entry → `connecting`.                                      |
| `failed`       | Failure plus explicit retryability. Retry uses fresh credentials/session where needed; dismiss → `idle`.                                                 |

Keep **role/capabilities separate from connection phase**:

- `isCreator`
- `canPublish`
- `canEndBroadcast`

Do not equate publishing permission with creator authority.

#### Ownership and lifecycle

| Event                          | Required behavior                                                                                                                                                                            |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Start/join                     | Manager captures account generation, obtains session, creates resources once, publishes state.                                                                                               |
| Navigate away                  | No teardown. Banner/mini-player observes the same manager.                                                                                                                                   |
| Leave/end                      | Attempt server mutation with captured account identity; always close local resources in `finally`; surface uncertain server outcome separately.                                              |
| Logout                         | Stop accepting account operations; invalidate generations; close media; revoke/remove captured credentials; await account teardown. Cleanup must not borrow a newly active account’s token.  |
| Account switch                 | Close old active media before activating the replacement UI. Other stored credentials remain in the vault.                                                                                   |
| App background                 | Continue audio only through the supported OS background-audio/foreground-service path. Suspend unnecessary feed refreshes; do not disconnect ongoing audio merely because a page disappears. |
| Process death                  | Assume no disposal callback. Store minimal recovery intent; on restart ask backend whether resumption is valid and obtain a fresh media token.                                               |
| Network loss                   | Reflect SDK reconnect state; avoid competing reconnect loops. Reconcile roster/feed gaps on return.                                                                                          |
| Host-ended/kicked/room deleted | Treat as terminal, cancel reconnect, close resources, preserve ending result for UI.                                                                                                         |

**Do not persist a LiveKit bearer token in preferences.** Recovery currently obtains a fresh token anyway; persist account ID, broadcast ID, role/intention, and recovery metadata.

#### DI registration map

```text
root
├── Config, sanitized Logger
├── Database, preferences, SecureStorage
├── SessionStore/AuthManager/account-transition coordinator
├── Http transport
├── MediaService, platform permission/push adapters
├── ThemeManager
└── GoRouter

authenticated(accountId, generation)
├── Immutable AccountContext
├── Account-bound HTTP identity/facades
├── RealtimeConnection owner — raw WS matching backend
├── Broadcast/Profile/Settings services and repositories
├── ActiveBroadcastManager
│   └── Active session? — ordinary owned object, not a DI scope
│       ├── RoomTransport
│       ├── roster reconciliation
│       └── media subscriptions/timers
├── NotesSyncManager — when implemented
└── NotificationCoordinator — when backend contract exists

widget lifetime
├── Editor/form managers
├── Profile/detail managers
└── Feed data sources via createOnce
```

Place the banner in authenticated application chrome if it must appear across tabs—not only in `HomePage`, where it currently lives.

#### Short code sketch

This is an API/ownership sketch, not a complete implementation or a proposal for additional interface layers.

```dart
enum LivePhase {
  idle, connecting, live, reconnecting, ending, ended, failed,
}

class LiveSnapshot {
  const LiveSnapshot({
    required this.phase,
    this.broadcast,
    this.failure,
    this.endReason,
  });

  final LivePhase phase;
  final Broadcast? broadcast;
  final Object? failure;
  final EndReason? endReason;
}

// Private resource object. Created only after entry is accepted.
class ActiveBroadcastSession {
  ActiveBroadcastSession({
    required this.account,
    required this.session,
    required this.room,
  });

  final AccountContext account; // accountId + generation
  final BroadcastSession session;
  final RoomTransport room;

  // Own roster subscriptions, retry work and idempotent close().
}

// Account-lifetime owner.
class ActiveBroadcastManager implements Disposable {
  ActiveBroadcastManager({
    required AccountContext account,
    required BroadcastHttpService http,
    required RoomTransport Function() createRoom,
    required Stream<BroadcastEvent> events,
  });

  ActiveBroadcastSession? _active;

  ValueListenable<LiveSnapshot> get snapshot;
  Command<Id, void> get join;
  Command<void, void> get leaveOrEnd;

  Future<void> recover();
  Future<void> closeForAccountExit();

  // Implementation requirements:
  // - serialize transitions;
  // - invalidate stale async completions with an operation generation;
  // - close resources in finally;
  // - never close a replacement session from an older operation.
}

scope.registerSingleton<ActiveBroadcastManager>(
  ActiveBroadcastManager(
    account: account,
    http: broadcastHttp,
    createRoom: createRoomTransport,
    events: broadcastEvents,
  ),
);
```

One safe `watch_it` usage:

```dart
class ActiveBroadcastBanner extends WatchingWidget {
  const ActiveBroadcastBanner({super.key});

  @override
  Widget build(BuildContext context) {
    final snapshot = watchValue(
      (ActiveBroadcastManager manager) => manager.snapshot,
    );

    if (snapshot.phase == LivePhase.idle) {
      return const SizedBox.shrink();
    }

    return BroadcastBannerContent(snapshot: snapshot);
  }
}
```

The manager registration always exists while authenticated. There is no early per-session lookup. Mount this subtree only after account readiness, keyed by account generation.

#### Migration steps

| Order | Change                                                                                                                         | Risk                            |
| ----: | ------------------------------------------------------------------------------------------------------------------------------ | ------------------------------- |
|     1 | Add regression tests for opening→logout, enter failure→close, host end, replacement, failed REST exit, and account switching.  | Low                             |
|     2 | Fix account-bound HTTP identity and account-transition readiness before moving ownership.                                      | High behavior impact; mandatory |
|     3 | Introduce one session snapshot and separate creator authority from publishing permission. Keep existing internals temporarily. | Medium                          |
|     4 | Move setup/connect/close into plain session methods; eliminate self-disposal through commands and uninitialized timer paths.   | Medium                          |
|     5 | Have account manager directly construct/own the session and transport. Remove conditional live registrations.                  | Medium                          |
|     6 | Move banner/init/live widgets to the stable manager. Preserve ended summary outside resources.                                 | Medium                          |
|     7 | Remove `LiveScopeManager`, live scope names, scope polling, and `WillSignalReady` session signaling.                           | Low after callers migrate       |
|     8 | Add device validation for background audio, disconnect reasons, process recovery, and permission denial.                       | Medium                          |

#### Generalization

- **Chats:** account-scoped connection/repository; room controller/session holds its subscriptions. Multiple open chats need keyed controllers, not a single nullable global chat.
- **Notifications:** account-scoped coordinator owns account-wide events and unread state; page owns inbox pagination. No per-notification scope.
- **Notes:** account-scoped sync owner; page-owned editor/feed state. Sync must outlive an editor.
- **Recording/audio:** same explicit resource-owner principle; do not create a universal “session framework” until genuinely shared behavior exists.

#### Verifiable grounding

These recommendations follow documented principles, not unverifiable claims about large companies:

- [Android background playback](https://developer.android.com/media/media3/session/background-playback) explicitly separates media ownership from Activity/UI lifetime and requires resource release in the service lifecycle. This supports the ownership principle; it is **not** a directive to replace LiveKit with Media3.
- [LiveKit connection documentation](https://docs.livekit.io/home/client/connect/) documents SDK-managed reconnection, disconnection reasons, explicit room disconnect, and disappearance after process termination.
- [Flutter lifecycle documentation](https://api.flutter.dev/flutter/dart-ui/AppLifecycleState.html) explicitly warns that lifecycle notifications can be skipped and no notification is guaranteed before process termination.

A single long-lived resource owner with observable state is also a general engineering pattern; no specific company-internal architecture is asserted.

### 1.5 Verdict on the account multi-account scope design

**Affirm the account scope. Fix its coordination, not its existence.**

Required changes:

1. Immutable account ID plus session generation.
2. Account readiness/transition observable.
3. Authenticated UI remounted by generation.
4. Requests and retries bound to originating identity.
5. Serialized, conditional session-store mutations.
6. Account-local managers should not independently follow global account selection.
7. Socket credentials must refresh/reconnect deliberately.
8. Account exit must invalidate in-flight operations before disposal.

Do not create four more scopes for chats, notes, discover, and notifications.

---

## 2. Executive Summary

### Health scores

These are architectural judgments from static inspection, not benchmark results.

| Area                         | Score /10 |
| ---------------------------- | --------: |
| Architecture                 |         5 |
| Performance and efficiency   |         5 |
| Scalability                  |         3 |
| Maintainability              |         4 |
| Safe to mirror as a template |         2 |

### Five most important takeaways

1. **Account isolation is not reliable.** A request can be replayed under another account; session persistence and logout have related races.
2. **Realtime does not match the backend.** The app uses Socket.IO; the backend exposes raw WebSocket frames. Additionally, no production call starts the socket or installs the broadcast event handlers.
3. **Live lifecycle ownership is unsafe.** Uninitialized teardown, failed-exit cleanup, ended-page lifetime, and independent transition queues are concrete defects—not theoretical scope objections.
4. **The feed/proxy foundations are recognizable but not template-ready.** Pagination races, filtered empty pages, unstable proxy identity, and viewer-dependent cache leakage would spread directly into upcoming features.
5. **Backend readiness is overstated.** Chat and notes exist, but contain relevant defects. Discover has building-block endpoints, not a dedicated discovery/search contract. A durable notifications inbox is not implemented.

### Verdict

**Fix first.**

Mirror selected small patterns—constructor injection, external adapters, widget-owned feeds, typed errors—not whole implemented features.

---

## 3. Findings, Ordered by Severity

Effort: **S** = localized, generally up to one day; **M** = coordinated changes over several days; **L** = cross-layer work requiring integration/device validation. Estimates overlap and should not be summed mechanically.

### CRITICAL

### [C01] Requests can cross account boundaries during replay

- **Severity:** CRITICAL
- **Category:** Security
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/services/session_interceptor.dart:96–124,140–167`; `http_client.dart`, `_send`.
- **What's wrong:** Dispatch does not record originating account/session identity. A 401 handler reads the currently active credential and replays the original operation using it. Transport retries can also re-enter authentication after switching.
- **Why it matters:** A mutation initiated as A can execute as B. Copying this into note autosave or chat sending risks private data being written under the wrong account.
- **Skill reference:** `get-it-expert` scope isolation; `flutter-architecture-expert` explicit ownership and service boundaries.
- **Recommended fix:** Capture account ID and session generation before dispatch. Bind refresh/retry to that identity; cancel or reject obsolete operations. Scope disposal must not cause cleanup to borrow another account’s credentials.
- **Effort:** M
- **Fix before mirroring?** Yes

### [C02] Session-store mutations are neither atomic nor identity-conditional

- **Severity:** CRITICAL
- **Category:** Data layer & persistence
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/services/session_store.dart:105–156,242–267`; `secure_storage.dart:40–47,62–67`; `session_interceptor.dart:114–118,145–150,235–244`.
- **What's wrong:** Vault updates use unprotected read–modify–write. Active credentials span independent keys. Refresh checks account identity and later writes it; failure unconditionally clears the active account.
- **Why it matters:** Concurrent renewal/switch/logout can lose rotated credentials, clear B because A failed, or reactivate A after switching to B.
- **Skill reference:** `flutter-architecture-expert`: one state owner; scope isolation must extend into persistence.
- **Recommended fix:** Serialize mutations and check expected account/session generation **inside** the serialized operation. Store a coherent, versioned credential record. Use conditional replace/remove APIs.
- **Effort:** M
- **Fix before mirroring?** Yes

### [C03] Explicit logout can leave a usable session recoverable

- **Severity:** CRITICAL
- **Category:** Security
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/auth/manager/auth_manager.dart:243–245,281–314`; `meno/apps/mobile/lib/_shared/services/session_store.dart:132–156`.
- **What's wrong:** Logout only clears active keys; credentials remain selectable in the vault. In-flight renewal can write them back.
- **Why it matters:** After an offline/failed server logout, selecting the account can restore access without authentication. This violates explicit logout semantics.
- **Skill reference:** `get-it-expert`: teardown must follow the business lifecycle, not just UI state.
- **Recommended fix:** Distinguish deactivate/add-account from logout. Remove the captured account’s usable credentials locally regardless of server outcome, invalidate renewal generations, and retain greeting metadata separately.
- **Effort:** M
- **Fix before mirroring?** Yes

### [C04] Mobile and backend use incompatible realtime protocols

- **Severity:** CRITICAL
- **Category:** Realtime layer
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/services/socket_client.dart:69–74,153–165`; `meno/apps/api/src/routes/ws.rs:241–271,492–503`; `meno/apps/api/src/routes/mod.rs`, `/ws`.
- **What's wrong:** Mobile uses Socket.IO framing and handshake. Backend uses raw WebSocket JSON `{event,data}` frames. Selecting Socket.IO’s WebSocket transport does not make it a raw-WebSocket client.
- **Why it matters:** Realtime cannot work against this backend as implemented. Building chats or notifications on it would institutionalize a broken boundary.
- **Skill reference:** `flutter-architecture-expert`: services must correctly translate the actual external protocol.
- **Recommended fix:** Replace the Socket.IO implementation with a raw-WebSocket adapter matching `/ws`, while preserving typed feature adapters. Implement heartbeat, connection authentication, bounded reconnect, and reconciliation. Do not invent Socket.IO acknowledgements.
- **Effort:** M
- **Fix before mirroring?** Yes

### [C05] Backend access-token revocation is not enforced by the actual HTTP guard

- **Severity:** CRITICAL
- **Category:** Security
- **Type:** Bug
- **Location:** `meno/apps/api/src/modules/auth/token.rs:540–565,600–614`; `auth/cache/redis_store.rs:155–187`; `meno/apps/api/src/middleware/auth.rs:281–290,456–459`; `meno/apps/api/src/state.rs:314–321`.
- **What's wrong:** Logout-all/password reset write user-wide invalidation markers, but the installed guard checks individual JTI revocation only.
- **Why it matters:** Existing access tokens remain usable until expiry despite successful logout-all/reset responses.
- **Skill reference:** None specific; backend authentication contract.
- **Recommended fix:** Validate both token-specific and user-wide revocation using verified subject and issuance time. Test through a protected router using production wiring.
- **Effort:** M
- **Fix before mirroring?** Yes

### HIGH

### [H01] Authenticated UI is not synchronized with account-scope readiness

- **Severity:** HIGH
- **Category:** Dependency injection (get_it)
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/auth/manager/user_scope_manager.dart:56–78,95–134`; `meno/apps/mobile/lib/_routing/router.dart:375–379`; `meno/apps/mobile/lib/app.dart:106–109`.
- **What's wrong:** Startup waits for initial scope creation, but runtime authentication changes do not expose equivalent readiness. Router refresh observes authentication/verification booleans, not account identity. Existing watches and `createOnce()` objects can retain disposed account instances.
- **Why it matters:** Login can build before registration; A→B can preserve A’s UI/state without a remount. Upcoming private feeds make this an isolation problem.
- **Skill reference:** `get-it-expert` readiness; `watch-it-expert` cached observables and scope changes.
- **Recommended fix:** Publish account transition/readiness and generation. Gate authenticated routes and remount the account subtree by generation.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H02] Live transitions can outlive account teardown or supersede newer sessions

- **Severity:** HIGH
- **Category:** Dependency injection (get_it)
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/manager/live_scope_manager.dart:163–173,195–223,289–340`; `user_scope_manager.dart:139–142`.
- **What's wrong:** Account and live transitions have independent queues. Live disposal does not await or invalidate queued/in-flight `_open()`. Recovery has no operation generation.
- **Why it matters:** A late open can push into a stack being torn down; late recovery can replace a user’s newer session.
- **Skill reference:** `get-it-expert` async scope lifecycle; `flutter-architecture-expert` clear resource ownership.
- **Recommended fix:** One account-owned transition owner; disposed/generation checks after awaits; quiesce operations before disposing resources. Prefer the Section 1 design.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H03] Session disposal can access an uninitialized timer

- **Severity:** HIGH
- **Category:** Error handling & resilience
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/manager/live_session_manager.dart:40–64,119–124,219,388–410`.
- **What's wrong:** The timer is initialized only by page-triggered setup, but exit and disposal access it unconditionally.
- **Why it matters:** Opening a session then logging out, replacing it, or receiving termination before the init page runs can throw `LateInitializationError` during cleanup.
- **Skill reference:** `flutter-architecture-expert`: lifecycle initialization belongs to the owner, not a page prerequisite.
- **Recommended fix:** Create the timer with the session or use a genuinely optional owned resource with safe teardown. Do not invoke UI commands from disposal.
- **Effort:** S–M
- **Fix before mirroring?** Yes

### [H04] Failed end/leave skips session closure; ended UI depends on disposed resources

- **Severity:** HIGH
- **Category:** Error handling & resilience
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/manager/live_session_manager.dart:119–145,356–365`; `live_scope_manager.dart:133–134,344–354`; `meno/apps/mobile/lib/features/broadcast/pages/ended_broadcast_page.dart:30–33`.
- **What's wrong:** A REST exception executes disconnect in `finally`, then bypasses `_live.end()`. Conversely, successful/remote termination removes the scope whose manager the ended page resolves.
- **Why it matters:** Failed exit leaves recovery state and scope behind; successful exit can break the summary page. Two listeners also independently handle termination.
- **Skill reference:** `flutter-architecture-expert` ownership; `command-it-expert` commands are not disposal primitives.
- **Recommended fix:** One idempotent close path in `finally`; retain ending result outside active resources; ended route loads by its broadcast ID.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H05] Broadcast end and cohost-removal contracts are wrong

- **Severity:** HIGH
- **Category:** Realtime layer
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/model/dtos.dart:124–136`; `services/broadcast_http_service.dart:181–186,244–255`; `meno/apps/api/src/modules/broadcast/dto.rs:185–193,537–557`; `handlers.rs:327–340`.
- **What's wrong:** End decoding requires `creatorName`, absent from the backend response. Cohost removal sends a query parameter instead of JSON `removeFromRoom` and expects a DTO although the backend returns no data.
- **Why it matters:** Successful mutations become client errors. “Keep listening” can default to eviction. End failure also triggers H04.
- **Skill reference:** `flutter-architecture-expert`: correct external-format translation.
- **Recommended fix:** Align end DTO/enrichment; make cohost removal return `void` and send the actual JSON body. Add backend-produced fixtures.
- **Effort:** S
- **Fix before mirroring?** Yes

### [H06] No production path starts realtime delivery

- **Severity:** HIGH
- **Category:** Realtime layer
- **Type:** Under-engineered
- **Location:** `meno/apps/mobile/lib/_shared/services/socket_client.dart`, `connect`; `meno/apps/mobile/lib/features/broadcast/services/broadcast_socket_service.dart:72–129`; `meno/apps/mobile/lib/locator.dart:170–185,290–299`.
- **What's wrong:** Source search found no production call to `SocketClient.connect()` or `BroadcastSocketService.listen()`. Subscribing to the adapter’s stream does not install its underlying handlers.
- **Why it matters:** Even after fixing protocol compatibility, host-end, roster, and invalidation delivery remain inactive.
- **Skill reference:** `get-it-expert` initialization; `flutter-architecture-expert` service ownership.
- **Recommended fix:** An explicit account connection owner initializes handlers before connecting, updates credentials, and closes on account exit.
- **Effort:** S–M
- **Fix before mirroring?** Yes

### [H07] Reconnection has stale credentials, conflicting state, and ignored terminal reasons

- **Severity:** HIGH
- **Category:** Error handling & resilience
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/manager/live_session_manager.dart:276–289,307–384`; `meno/apps/mobile/lib/_shared/services/livekit_client.dart:266–278`; `socket_client.dart:15–18`; `broadcast_socket_service.dart:58–65`.
- **What's wrong:** Session `state` and `status` can contradict each other. Failed initial connection changes state to error; later connection events can paint live before the command chain settles. Disconnection reasons are discarded. Retry uses the original media token; broadcast-error and reconnection streams have no session consumer.
- **Why it matters:** Kicks/room deletion can be retried as network failures; long-lived sessions can retry expired credentials; UI can show inconsistent availability.
- **Skill reference:** `command-it-expert` observable command state; `flutter-architecture-expert` single coherent manager state.
- **Recommended fix:** One phase machine, preserve disconnect reasons, let SDK reconnect first, obtain fresh entry credentials when a full reconnect requires them, and reconcile after transport return.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H08] Publishing permission is mistaken for creator authority

- **Severity:** HIGH
- **Category:** Architecture & layering
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/manager/live_session_manager.dart:127–130,203–205`; `model/entities.dart:207–208`; `widgets/broadcast_control_buttons.dart:20–25`.
- **What's wrong:** `isHost` becomes true for any session that can publish, including cohosts. Exit then calls creator-only `/end`.
- **Why it matters:** Cohosts receive incorrect controls/behavior and cannot leave correctly through that branch.
- **Skill reference:** `flutter-architecture-expert`: domain responsibilities and capabilities.
- **Recommended fix:** Separate creator identity, publish capability, and end authority. Gate microphone and stop controls independently.
- **Effort:** S
- **Fix before mirroring?** Yes

### [H09] Refresh failures destroy sessions, including ambiguous rotated-token outcomes

- **Severity:** HIGH
- **Category:** Error handling & resilience
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/services/session_interceptor.dart:211–261`; `meno/apps/api/src/modules/auth/token.rs:439–486`.
- **What's wrong:** Offline, timeout, 429/503, malformed response, and storage failure all become `null`, then sign-out. Backend refresh rotates single-use tokens and treats reuse as a reason to revoke sessions.
- **Why it matters:** A transient outage logs users out. A lost refresh response can leave a consumed token in the vault; selecting it later can trigger wider revocation.
- **Skill reference:** `command-it-expert` deliberate error reactions; `flutter-architecture-expert` typed failure boundaries.
- **Recommended fix:** Distinguish definite rejection, temporary unavailability, and uncertain-after-dispatch. Preserve recoverable state; define backend recovery/idempotency for ambiguous rotation rather than blindly retrying.
- **Effort:** M–L
- **Fix before mirroring?** Yes

### [H10] Stored access-token expiry moves forward on every read

- **Severity:** HIGH
- **Category:** Data layer & persistence
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/auth/model/dtos.dart:38–55`; `meno/apps/mobile/lib/_shared/services/session_store.dart:79–82,233–250,282–284`.
- **What's wrong:** Storage uses API-relative `expires_in`; decoding computes `now + expires_in` again.
- **Why it matters:** Expired credentials look fresh after restart/read, defeating proactive refresh. Server expiry is unchanged, so failures shift to reactive 401 handling.
- **Skill reference:** `flutter-architecture-expert`: external DTOs and persistence representations serve different contracts.
- **Recommended fix:** Persist absolute `expires_at` in a versioned storage record; convert relative lifetime once at API receipt. Test with an injected clock.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H11] Refresh, pagination, query changes, and invalidations race

- **Severity:** HIGH
- **Category:** Feeds & pagination
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/manager/feed_data_source.dart:24–28,87–95,149–155`; `broadcast_feed_data_source.dart:75–126,183–187`.
- **What's wrong:** Refresh and pagination can run concurrently. Responses use mutable query state. Query changes and invalidations can be discarded while a command runs.
- **Why it matters:** Old-page rows/cursors overwrite fresh results; search/filter changes show the wrong dataset. High-frequency chat/notification events make this worse.
- **Skill reference:** `feed-datasource-expert` separate load/page operations; `command-it-expert` restrictions.
- **Recommended fix:** Capture immutable query generations, reject stale responses, arbitrate page versus refresh, and coalesce a trailing refresh when invalidation occurs during a request.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H12] Client-filtered empty pages strand pagination

- **Severity:** HIGH
- **Category:** Feeds & pagination
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:78–85,140–143`; `feed_data_source.dart:149–155`; `meno/apps/mobile/lib/_shared/widgets/feed_widget.dart:151`.
- **What's wrong:** Recently-live filters an inactive page locally. If nothing remains, the widget renders empty and never reads an item to trigger the next cursor.
- **Why it matters:** Existing ended broadcasts can become unreachable. The same trap applies to client-filtered Discover and notification feeds.
- **Skill reference:** `feed-datasource-expert`: item-triggered pagination assumes visible progress.
- **Recommended fix:** Add an actual backend ended filter, or advance through filtered pages until visible progress/exhaustion with bounded work and cursor-progress checks.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H13] Cold profile proxies share an empty identity and cannot be released correctly

- **Severity:** HIGH
- **Category:** Data layer & persistence
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/profile/manager/user_profile_manager.dart:44,77–82`; `model/proxies/user_profile_proxy.dart:173–198`.
- **What's wrong:** Uncached profiles acquire the same `Id.empty` proxy; fetching mutates its ID. Repository release looks up the new ID rather than the original map key.
- **Why it matters:** Concurrent cold profiles overwrite each other, and repository entries leak. This is unsafe for chat actors and notification users.
- **Skill reference:** `flutter-architecture-expert` DataRepository identity/reference counting.
- **Recommended fix:** Acquire using immutable requested identity, use an ID-bearing loading placeholder, forbid target-ID changes, and release against stable ownership.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H14] Viewer-specific profile cache state leaks across accounts

- **Severity:** HIGH
- **Category:** Data layer & persistence
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/profile/services/profile_cache_service.dart:43–51`; `model/dtos.dart:134–145`; `meno/apps/api/src/modules/profile/service.rs:172–189`.
- **What's wrong:** Cache keys identify only the profile owner, but persisted `isFollowing` depends on the viewer.
- **Why it matters:** B sees A’s follow relationship and can choose the wrong mutation before refresh. Account-scoped service instances do not isolate shared keys.
- **Skill reference:** `get-it-expert` account isolation; `flutter-architecture-expert` repository identity.
- **Recommended fix:** Key viewer-relative state by `(viewerId, subjectId)` or split public and relationship records; invalidate legacy cache entries.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H15] Async operations continue after their owner is disposed

- **Severity:** HIGH
- **Category:** Performance & efficiency
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:75–107,220–224`; `features/profile/manager/user_profile_manager.dart:77–82`; `profile_editor_manager.dart:109–137`; `features/settings/manager/settings_manager.dart:246–251`.
- **What's wrong:** Several HTTP continuations mutate disposed notifiers or begin another request after account/page exit. Disposing a command is not cancellation of its asynchronous body.
- **Why it matters:** Late profile upload→patch can cross account transitions; feeds and settings can notify disposed state. Notes autosave would be especially dangerous.
- **Skill reference:** `watch-it-expert` lifecycle; `flutter-architecture-expert` ownership.
- **Recommended fix:** Cancel requests where supported; check owner/session generation after every await; invalidate mutation/undo continuations on disposal.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H16] Participant UI does not implement the backend roster contract

- **Severity:** HIGH
- **Category:** Realtime layer
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/manager/participants_manager.dart:27–43,53–66,212–244`; `services/broadcast_http_service.dart:47–61`; `meno/apps/api/src/modules/broadcast/service.rs:931–967,1073–1108`.
- **What's wrong:** Mobile treats one historical page as the complete current roster, ignores `isJoined`, discards cursors, and counts loaded rows. Initial HTTP replacement can overwrite newer socket changes. Backend roster events currently target host/cohosts, not ordinary listeners.
- **Why it matters:** Departed users appear present; rooms over 50 are incomplete; listeners’ lists cannot remain accurate even with working transport.
- **Skill reference:** `feed-datasource-expert` cursor pagination/event reconciliation; `listen-it-expert` batched collections.
- **Recommended fix:** Define present-only versus attendance history, traverse cursor pages, use authoritative presence counts, reconcile deltas over snapshots, and correct event audience or listener refresh policy.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H17] Backend cohost removal does not reliably revoke role or presence

- **Severity:** HIGH
- **Category:** Security
- **Type:** Bug
- **Location:** `meno/apps/api/src/modules/broadcast/service.rs:896–912`; `repository/pg.rs:427–447`; `media.rs:41–95`.
- **What's wrong:** Removal calls `join` to demote. Active rows return `AlreadyPresent` without role change; absent invitees can become joined. Eviction does not close the participant row, and keeping the participant does not revoke existing media publish permissions.
- **Why it matters:** Removed cohosts can retain privileges or corrupt presence/counts.
- **Skill reference:** None specific; backend role/presence contract.
- **Recommended fix:** Dedicated atomic role/presence operations, connected media permission updates, and matching events.
- **Effort:** M–L
- **Fix before mirroring?** Yes

### [H18] Backend broadcast transitions are not safely retryable

- **Severity:** HIGH
- **Category:** Error handling & resilience
- **Type:** Bug
- **Location:** `meno/apps/api/src/modules/broadcast/service.rs:440–476,728–744`; `repository/pg.rs:283–286,349–354,468–474`.
- **What's wrong:** Start/join commit state before later fallible token/response work. Start also accepts previously ended inactive rows without clearing ending fields.
- **Why it matters:** An error can mean “already committed,” making retry conflict; an ended broadcast can become active while still reported ended.
- **Skill reference:** None specific; lifecycle transaction contract.
- **Recommended fix:** Reject terminal rows atomically; move fallible preconditions earlier; make related DB transitions atomic; define compensation or committed-outcome recovery.
- **Effort:** M–L
- **Fix before mirroring?** Yes

### [H19] Editor progress and retries can report success without completing entry

- **Severity:** HIGH
- **Category:** Commands (command_it)
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/manager/broadcast_editor_manager.dart:35–84`; `pages/broadcast_editor_page.dart:46–55`.
- **What's wrong:** `.started` is recorded before local entry succeeds; retries then return early. Busy state excludes creation/upload and local entry. Existing created rows are not patched when edited before retry.
- **Why it matters:** Fields remain mutable mid-operation, displayed edits can be ignored, and retry can navigate after a successful no-op.
- **Skill reference:** `command-it-expert` restrictions/loading state; `flutter-architecture-expert` plain internal orchestration.
- **Recommended fix:** One operation spanning create/upload/start/entry; distinguish remote commitment from local connection; retry the incomplete phase and synchronize metadata.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H20] Broadcast mutations do not carry stable logical-operation idempotency keys

- **Severity:** HIGH
- **Category:** Error handling & resilience
- **Type:** Under-engineered
- **Location:** `meno/apps/mobile/lib/features/broadcast/services/broadcast_http_service.dart:84–98,173–195`; `meno/apps/mobile/lib/_shared/services/http_client.dart:159–166,447–452`; `meno/apps/api/src/middleware/idempotency.rs:305–315`.
- **What's wrong:** Create/live/join omit the backend’s optional idempotency protection. A new manual attempt would also need the same logical key, not a newly generated one.
- **Why it matters:** Lost responses can duplicate drafts or turn successful joins into conflicts. Notes sync requires the same discipline.
- **Skill reference:** `flutter-architecture-expert`: business operation identity belongs in the manager/data boundary.
- **Recommended fix:** Persist one key per logical mutation through ambiguous retries. Do not indiscriminately retry every mutation.
- **Effort:** M
- **Fix before mirroring?** Yes

### [H21] Password-reset requests revoke sessions before proving ownership

- **Severity:** HIGH
- **Category:** Security
- **Type:** Bug
- **Location:** `meno/apps/api/src/routes/auth.rs:95–110`; `modules/auth/credentials.rs:168–189`.
- **What's wrong:** Public forgot-password revokes the target user’s sessions before validating a reset code.
- **Why it matters:** Knowing an email address permits session disruption. Fixing C05 makes premature access revocation more immediate too.
- **Skill reference:** None specific.
- **Recommended fix:** Revoke only after successful reset-code verification and password replacement; retain enumeration-resistant responses.
- **Effort:** S
- **Fix before mirroring?** Yes

### [H22] Notes sync can lose folders created in the same batch

- **Severity:** HIGH
- **Category:** Data layer & persistence
- **Type:** Bug
- **Location:** `meno/apps/api/src/modules/notes/repository/pg.rs:867–902`; `modules/notes/model.rs:181–186`.
- **What's wrong:** Owned-folder IDs are collected before applying folder mutations, contradicting folder-first batch semantics.
- **Why it matters:** Creating a folder and its note offline then syncing can silently unfile the note.
- **Skill reference:** None specific; offline synchronization contract.
- **Recommended fix:** Resolve live folder ownership after folder mutations; add PostgreSQL regression coverage.
- **Effort:** S–M
- **Fix before mirroring?** Yes

### [H23] Chat deletion can mutate one room and publish to another

- **Severity:** HIGH
- **Category:** Security
- **Type:** Bug
- **Location:** `meno/apps/api/src/modules/chat/service.rs:252–279`; `repository/pg.rs:235–252`.
- **What's wrong:** Successful deletion is guarded by message ID/sender, not supplied broadcast ID. Broadcast identity is checked only after failure.
- **Why it matters:** An author can delete their message using another room ID and send an incorrect deletion event there.
- **Skill reference:** None specific.
- **Recommended fix:** Include broadcast identity in the guarded delete and test cross-room requests.
- **Effort:** S
- **Fix before mirroring?** Yes

### [H24] Push and durable notifications are not implemented end-to-end

- **Severity:** HIGH
- **Category:** Scalability
- **Type:** Under-engineered
- **Location:** `meno/apps/mobile/lib/main.dart:6–23`; `locator.dart:89–100`; `_shared/services/push_notifications_service.dart:13–36`; `meno/apps/api/src/modules/mod.rs:16–21,70–76`; `worker.rs:47–59`.
- **What's wrong:** Firebase startup, background registration, foreground/open/token-refresh consumers, and local-notification initialization are unwired. Backend has no inbox list/read/unread routes; the worker does not run notification jobs.
- **Why it matters:** Push adapters are not a notifications feature or template. The assumed backend readiness is false.
- **Skill reference:** `flutter-architecture-expert`: adapters versus business coordination.
- **Recommended fix:** Define durable inbox identity/pagination/read state and device-token policy; implement a root platform adapter plus account-scoped coordinator.
- **Effort:** L
- **Fix before mirroring?** Yes

### [H25] Client validation rejects server-valid accounts and can break credential storage

- **Severity:** HIGH
- **Category:** Anti-patterns & code smells
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_core/value_objects/email.dart:9–35`; `password.dart:67–84`; `text_value_objects.dart:17–18,52`; `_shared/model/dtos.dart:35–55`; `meno/apps/api/src/modules/auth/validators.rs:128–270`.
- **What's wrong:** Email regex, password policy/length counting, and minimum name length diverge. Wire values become strict value objects, then storage calls `getOrCrash`.
- **Why it matters:** Valid accounts can fail login or credential persistence; client-accepted forms can fail server validation.
- **Skill reference:** `flutter-architecture-expert`: validate external conversion explicitly.
- **Recommended fix:** Shared accepted/rejected fixtures, aligned documented policies, and separate wire/storage mapping. Do not discover invalidity during serialization.
- **Effort:** M
- **Fix before mirroring?** Yes

### MEDIUM

### [M01] Email resend availability drives an automatic resend loop

- **Severity:** MEDIUM
- **Category:** Reactive state (watch_it / listen_it)
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/auth/pages/email_verification_page.dart:42–56,64–67,92–97,182–184`; `manager/email_verification_manager.dart:36–69`.
- **What's wrong:** Every `canResend` change starts cooldown and sends OTP, ignoring the boolean. Countdown completion therefore triggers another send. Verification errors also have duplicate handlers.
- **Why it matters:** Repeated mail/network activity without a user action; a bad template for debounced autosave or notification permission state.
- **Skill reference:** `watch-it-expert`: handlers for intentional effects; derived availability is not an action.
- **Recommended fix:** Send from the tap command; derive availability from cooldown and running state; keep one error handler.
- **Effort:** S
- **Fix before mirroring?** Yes

### [M02] Add-account flow retains password-only login mode

- **Severity:** MEDIUM
- **Category:** Architecture & layering
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/auth/manager/auth_manager.dart:69–73,265–271`; `pages/login_page.dart:43–45,59–92`; `widgets/switch_account_sheet.dart:76–79`.
- **What's wrong:** Add-account clears active state but retains last-known user, which hides the email field.
- **Why it matters:** The visible flow cannot enter another existing account’s email after a prior sign-in.
- **Skill reference:** `flutter-architecture-expert`: UI mode and remembered identity are distinct state.
- **Recommended fix:** Explicit “use another account” mode independent of greeting metadata.
- **Effort:** S
- **Fix before mirroring?** Yes

### [M03] Scoped credentials and socket tokens stay stale

- **Severity:** MEDIUM
- **Category:** Dependency injection (get_it)
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/auth/manager/user_scope_manager.dart:150–157`; `meno/apps/mobile/lib/locator.dart:170–175`.
- **What's wrong:** Credential replacement is skipped under a child scope; socket construction captures the initial credential permanently.
- **Why it matters:** Closing the child does not repair the scoped credential; future socket handshakes can use expired tokens.
- **Skill reference:** `get-it-expert` scope shadowing/current-scope registration.
- **Recommended fix:** Stable account credential holder; deliberate connection credential refresh. Avoid unregister/re-register for ordinary state changes.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M04] Public profile page resolves an unregistered proxy

- **Severity:** MEDIUM
- **Category:** Reactive state (watch_it / listen_it)
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/profile/pages/user_profile_page.dart:30–40,58–61`.
- **What's wrong:** The follow-error handler selects `UserProfileProxy` from DI, but the proxy is owned by the local manager/repository.
- **Why it matters:** The page throws when reached. The current public-profile route is still a stub, so the defect is latent behind routing.
- **Skill reference:** `watch-it-expert`: `target` plus typed `select` for local observables.
- **Recommended fix:** Target `manager.profile`; do not register every entity proxy globally.
- **Effort:** S
- **Fix before mirroring?** Yes

### [M05] Local storage isolation is optional rather than enforced

- **Severity:** MEDIUM
- **Category:** Data layer & persistence
- **Type:** Under-engineered
- **Location:** `meno/apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart:20–46,100–104`; `manager/broadcast_editor_manager.dart:104–119`; `_shared/services/database.dart:42–47`.
- **What's wrong:** Lists filter by owner, but draft read/delete/upsert use ID alone; supplied drafts are not ownership-checked. Raw boxes remain exposed.
- **Why it matters:** Stale/internal foreign IDs can cross accounts. This is not evidence of a remote exploit, but it is unacceptable for private notes/history.
- **Skill reference:** `get-it-expert` account isolation; `flutter-architecture-expert` service boundary.
- **Recommended fix:** Immutable owner-bound local facade; owner-qualified reads/deletes/upserts and foreign-object rejection.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M06] ObjectBox writes can duplicate logical rows; queries lack deterministic cleanup

- **Severity:** MEDIUM
- **Category:** Data layer & persistence
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/services/broadcast_local_service.dart:21–40,51–104`; `broadcast_rows.dart:30–34,76–80`.
- **What's wrong:** Lookup→write operations are not one transaction, business keys lack uniqueness, and built queries are not closed.
- **Why it matters:** Concurrent saves/toggles can duplicate rows; native query resources await finalization. High-frequency note/chat operations amplify this.
- **Skill reference:** `flutter-architecture-expert` persistence ownership.
- **Recommended fix:** Transactional upserts, unique logical keys, idempotent desired-state setters, and `try/finally` query closure.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M07] Favourites are disconnected snapshots

- **Severity:** MEDIUM
- **Category:** Reactive state (watch_it / listen_it)
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/manager/favourites_manager.dart:15–20,50`; `model/proxies/broadcast_proxy.dart:21–29`; `meno/apps/mobile/lib/features/profile/pages/my_profile_page.dart:316–321`.
- **What's wrong:** The tab watches an initially empty list without triggering fetch; proxy writes do not update that list or other proxy instances.
- **Why it matters:** Saved items remain invisible/stale. The same approach would break read/unread, likes, and local note status.
- **Skill reference:** `listen-it-expert` shared reactive collections; `flutter-architecture-expert` shared entity identity.
- **Recommended fix:** One account-level reactive favourites source, preferably backed by an owner-filtered DB query; reconcile authoritative stored state.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M08] Pagination errors are absent from the feed error surface

- **Severity:** MEDIUM
- **Category:** Feeds & pagination
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/manager/feed_data_source.dart:41–42,87–90,149–155`; `_shared/widgets/feed_widget.dart:127–147`.
- **What's wrong:** `commandErrors` exposes refresh errors only. Pagination gating also checks that channel.
- **Why it matters:** Next-page failures have no proper footer/retry state and can be retried incidentally during rebuilding.
- **Skill reference:** `feed-datasource-expert` distinct initial/page loading and errors.
- **Recommended fix:** Separate observable page error/retry state and block automatic repeated pagination until explicit retry.
- **Effort:** S–M
- **Fix before mirroring?** Yes

### [M09] Socket rebinding cancels the wrong handles; feed events ignore filters

- **Severity:** MEDIUM
- **Category:** Realtime layer
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/manager/broadcast_feed_data_source.dart:119–124,172–178,190–205`.
- **What's wrong:** Unawaited cancellation reads subscription fields across awaits while rebinding overwrites those fields. Event insertions do not enforce creator/query predicates.
- **Why it matters:** Subscriptions leak/disappear and creator-specific feeds acquire other creators’ rows.
- **Skill reference:** `feed-datasource-expert` event integration/lifecycle; `listen-it-expert` cancellation ownership.
- **Recommended fix:** Detach old handles into locals synchronously, then cancel them. Apply full query predicates or invalidate when event data is insufficient.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M10] Invalidation parsing and event enrichment are inconsistent

- **Severity:** MEDIUM
- **Category:** Realtime layer
- **Type:** Bug
- **Location:** `meno/apps/api/src/infrastructure/ws/dto.rs:169–173,234–238`; `meno/apps/mobile/lib/features/broadcast/model/dtos.dart:113–119`; `services/broadcast_socket_service.dart:120–126,167–189`; `_shared/manager/broadcast_feed_data_source.dart:202–205`.
- **What's wrong:** Bare backend `OffsetDateTime` serialization does not satisfy the client’s RFC3339 `DateTime` expectation. Invalidation decoding bypasses the adapter’s guarded `_decode`. End events contain only ID/reason, yet are converted into full-looking cards with empty summary fields.
- **Why it matters:** Invalidation can throw; recently-ended cards can have blank data until refresh.
- **Skill reference:** `flutter-architecture-expert`: boundary decoding; `feed-datasource-expert`: invalidate/refetch when event data is insufficient.
- **Recommended fix:** Explicit RFC3339 typed payload; guarded decoding; keep end signals distinct from rich REST summaries and enrich/refetch.
- **Effort:** S–M
- **Fix before mirroring?** Yes

### [M11] Profile editor compares different types and ignores bio validity

- **Severity:** MEDIUM
- **Category:** Anti-patterns & code smells
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/profile/manager/profile_editor_manager.dart:34–39,54–69,123–129`.
- **What's wrong:** Bio strings are compared with `MultiLineString`; validity includes only name. Comments claim a relaxed name type while the field is `FullName`.
- **Why it matters:** Unchanged bio appears changed; invalid bio can become a deletion; API-valid short names are blocked.
- **Skill reference:** `listen-it-expert` derived validation; `flutter-architecture-expert` boundary consistency.
- **Recommended fix:** Compare unwrapped values consistently, include bio validity, align edit validation with backend, correct misleading comments.
- **Effort:** S
- **Fix before mirroring?** Yes

### [M12] Current-user presentation has multiple unsynchronized sources

- **Severity:** MEDIUM
- **Category:** Architecture & layering
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/widgets/user_avatar_widget.dart:14–20`; `features/broadcast/pages/home_page.dart:61–63`; `features/profile/manager/profile_editor_manager.dart:137`; `my_profile_manager.dart:82–85`.
- **What's wrong:** Editing updates `MyProfileManager`, while shared avatar/greeting reads auth’s stored user.
- **Why it matters:** Profile edits do not consistently update app chrome or remembered account display.
- **Skill reference:** `flutter-architecture-expert` single authoritative entity state.
- **Recommended fix:** One presentation source for current-user name/avatar; synchronize credential metadata explicitly without making tokens the profile store.
- **Effort:** S–M
- **Fix before mirroring?** Yes

### [M13] Settings writes race and overwrite unknown preference keys

- **Severity:** MEDIUM
- **Category:** Data layer & persistence
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/settings/manager/settings_manager.dart:90–145,246–251,299–315`; `model/dtos.dart:12–19,64–72`; `meno/apps/api/src/modules/settings/repository/pg.rs:65–72`.
- **What's wrong:** Independent commands send complete preference objects; late GETs replace optimistic edits; returned settings are discarded; unknown JSON keys disappear.
- **Why it matters:** Concurrent toggles lose changes; future chat/notification preferences can be deleted by older clients.
- **Skill reference:** `command-it-expert` restrictions/undo; `flutter-architecture-expert` authoritative reconciliation.
- **Recommended fix:** Serialize/coalesce settings mutations, version reads, reconcile responses against newer edits, preserve unknown keys or use field-level backend patches.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M14] Account-scoped managers redundantly track global account selection

- **Severity:** MEDIUM
- **Category:** Over-engineering
- **Type:** Over-engineered
- **Location:** `meno/apps/mobile/lib/features/profile/manager/my_profile_manager.dart:70–74,147–160`; `features/settings/manager/settings_manager.dart:208–215,347–362`; `locator.dart:250–281`.
- **What's wrong:** Account-local managers subscribe to a root account selector to solve a lifecycle their scope already owns.
- **Why it matters:** Old managers can start next-account work during teardown; duplicate loads/subscriptions obscure ownership.
- **Skill reference:** `flutter-architecture-expert`: semantically owned managers; `get-it-expert`: scope lifetime.
- **Recommended fix:** Inject immutable account context; initialize once. Use generation/cancellation for pending work.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M15] HTTP failure paths can hang, misclassify, or leak credentials to external origins

- **Severity:** MEDIUM
- **Category:** Networking
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/_shared/services/session_interceptor.dart:96–150,166–175`; `http_client.dart:387–406`.
- **What's wrong:** Storage exceptions can escape without completing interceptor handlers. Replay transport failures become the original 401. Downloads handle 401 differently. Token attachment does not validate destination origin.
- **Why it matters:** Commands can remain pending; connectivity becomes false auth failure; an external URL passed to this client receives bearer credentials. **An attacker-controlled download caller was not established.**
- **Skill reference:** `flutter-architecture-expert` transport/error boundary.
- **Recommended fix:** Complete every handler exactly once, preserve replay failure/uncertainty, share auth dispatch semantics, and attach tokens only to the approved API origin.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M16] Push payloads, channels, and tap lifecycle disagree

- **Severity:** MEDIUM
- **Category:** Routing (go_router)
- **Type:** Bug
- **Location:** `meno/apps/api/src/infrastructure/push/model.rs:17–28,87–95`; `push/dto.rs:240–254`; `meno/apps/mobile/lib/_shared/services/local_notifications_service.dart:22–34,85–126`.
- **What's wrong:** Backend sends `deep_link`/`user_id`; mobile reads `route`. Channel IDs differ. Cold taps are dropped if navigator is absent; launch-detail handling is missing. General background use of `showStatic()` can duplicate OS notifications.
- **Why it matters:** Notifications navigate nowhere or to the wrong active account; foreground/background handling cannot be copied safely.
- **Skill reference:** `flutter-architecture-expert`: platform adapter plus account/navigation coordinator.
- **Recommended fix:** Contract fixtures, recipient attribution, validated destinations, pending-intent queue, cold/warm handling, and foreground-only local rendering policy.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M17] Background-audio support is declared but not established

- **Severity:** MEDIUM
- **Category:** Other
- **Type:** Under-engineered
- **Location:** `meno/apps/mobile/lib/_shared/services/permissions_service.dart:249–290`; `permission_prompts.dart:17–24`; `meno/apps/mobile/android/app/src/main/AndroidManifest.xml:6–40`; `meno/apps/mobile/ios/Runner/Info.plist:72–85`.
- **What's wrong:** Background permission/initialization is not `enableBackgroundExecution`; no such call was found. Broadcast gating differs from `canBroadcast`. iOS lacks remote-notification background mode while declaring processing without task setup.
- **Why it matters:** Locked-screen broadcasting and background push execution remain **needs verification**, not implemented guarantees.
- **Skill reference:** `flutter-architecture-expert` OS service boundary.
- **Recommended fix:** Define supported background behavior, implement the appropriate service/audio-session lifecycle, and test locked screen, interruptions, and OS termination on devices.
- **Effort:** M–L
- **Fix before mirroring?** Yes

### [M18] Production can select a successful silent transport; recovery stores a bearer token in preferences

- **Severity:** MEDIUM
- **Category:** Security
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/locator.dart:348–358`; `features/broadcast/services/live_session_store.dart:54–63`; `_core/config/config.dart:34–38`.
- **What's wrong:** Empty production LiveKit configuration silently uses the fake transport. Recovery stores a media token in non-secure preferences even though recovery fetches a fresh one.
- **Why it matters:** A release can claim a live broadcast with no audio. The stored token is unnecessary sensitive data.
- **Skill reference:** `flutter-architecture-expert`: production adapters should fail honestly.
- **Recommended fix:** Reject invalid production configuration; permit silent transport only through explicit test/dev configuration; persist minimal recovery intent.
- **Effort:** S
- **Fix before mirroring?** Yes

### [M19] Routing discards identity and intended destinations

- **Severity:** MEDIUM
- **Category:** Routing (go_router)
- **Type:** Under-engineered
- **Location:** `meno/apps/mobile/lib/_routing/router_guard.dart:18–37`; `router.dart:168,334–355`; `routes.dart:164–200`; `features/broadcast/widgets/pre_stream_modal.dart:16–19`.
- **What's wrong:** Auth redirects do not preserve intended destination. Public-profile routes are stubs. Ended route ignores its ID. Live routes depend on hidden active state, and lobby shortcut checks active ID rather than connected/initialized phase.
- **Why it matters:** Notification/deep links cannot reliably resume after login or process death; live shortcuts can bypass setup.
- **Skill reference:** `flutter-architecture-expert`: state ownership independent of navigation.
- **Recommended fix:** Preserve validated pending destinations, load summaries/details by route ID, and have live entry consult session phase. Add platform link registration.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M20] Named navigation is used for a route without that name

- **Severity:** MEDIUM
- **Category:** Routing (go_router)
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/pages/home_page.dart:150–151,192–195,236–238`; `_routing/router.dart:92–96`.
- **What's wrong:** Home calls `pushNamed(MRoutes.broadcasts, ...)`, while the route declares `path` but no matching `name`.
- **Why it matters:** “See all” navigation fails at runtime.
- **Skill reference:** None specific.
- **Recommended fix:** Declare a distinct route name or navigate by a constructed location with query parameters.
- **Effort:** S
- **Fix before mirroring?** Yes

### [M21] Scope/connection orchestration is over-commanded

- **Severity:** MEDIUM
- **Category:** Commands (command_it)
- **Type:** Over-engineered
- **Location:** `meno/apps/mobile/lib/features/broadcast/manager/live_session_manager.dart:40–117,159–167`; `_shared/pages/live_session_init_page.dart:98–174`; `manager/broadcast_timer_manager.dart:21–26`.
- **What's wrong:** Internal setup/connect/announce/settle is a command pipeline, timer methods are commands, and disposal runs a UI command. Session implements readiness signaling that only successful settle completes. The transition page manually bridges notifier listeners to futures.
- **Why it matters:** More observable/disposable objects and hidden sequencing without user-facing benefit; failure and teardown are harder to reason about.
- **Skill reference:** `flutter-architecture-expert`: plain internal methods, commands at UI boundary; `listen-it-expert`: subscription helpers.
- **Recommended fix:** Plain async entry/close methods; commands only for user actions. Use phase observation for the transition page, not DI readiness.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M22] Shared infrastructure imports features and broad barrels hide the dependency graph

- **Severity:** MEDIUM
- **Category:** Folder & file structure
- **Type:** Structure
- **Location:** `meno/apps/mobile/lib/_shared/_shared.dart:1–23`; `_shared/manager/broadcast_feed_data_source.dart:4`; `_shared/layouts/live_session_shell.dart:5`; `_shared/services/session_store.dart:7`; `features/auth/manager/user_manager.dart:15–17`.
- **What's wrong:** Broadcast-specific feed/live pages/layouts live in `_shared`; session storage depends on auth; features import a barrel exporting feature-dependent code. The claimed feature-independent shared bottom layer is not real.
- **Why it matters:** Four more features will make imports tangled and ownership harder to discover.
- **Skill reference:** `flutter-architecture-expert`: feature-first; shared only when genuinely cross-feature.
- **Recommended fix:** Move broadcast feed/session UI into broadcast, auth persistence into auth, and app shells into an app layer. Use narrow imports across boundaries. Keep useful feature barrels.
- **Effort:** M
- **Fix before mirroring?** Yes

### [M23] Production logging suppresses every severity

- **Severity:** MEDIUM
- **Category:** Error handling & resilience
- **Type:** Under-engineered
- **Location:** `meno/apps/mobile/lib/_core/logging/meno_logger.dart:68,100–143`; `_core/exceptions/global_exception_handler.dart:13–39`.
- **What's wrong:** `_on == env.isDev` gates warnings, errors, and fatal events too.
- **Why it matters:** Production lifecycle/network/storage failures lose their diagnostic trail despite request-ID support.
- **Skill reference:** `command-it-expert` global catch-all; architecture skill error reporting.
- **Recommended fix:** Explicit sanitized production warning/error policy and reporting sink; never log credentials or private message/note bodies.
- **Effort:** S
- **Fix before mirroring?** Yes

### [M24] Critical lifecycle tests are commented out

- **Severity:** MEDIUM
- **Category:** Testing & maintainability
- **Type:** Under-engineered
- **Location:** `meno/apps/mobile/test/features/auth/user_scope_manager_test.dart`; `features/broadcast/live_scope_manager_test.dart`; `stream_manager_test.dart`; `features/profile/my_profile_manager_test.dart`.
- **What's wrong:** Four complete suites are comments. Twenty-four test files exist, but file count overstates executable coverage. Room transport tests exercise the silent fake, not LiveKit integration.
- **Why it matters:** The exact transition boundaries proposed as templates have no executable regression protection.
- **Skill reference:** All seven skills assume explicit lifecycle testing; `get-it-expert` isolated tests.
- **Recommended fix:** Reinstate focused tests using controllable futures and fakes; add real protocol/contract and device tests. Do not merely uncomment obsolete expectations.
- **Effort:** M–L
- **Fix before mirroring?** Yes

### [M25] Build/platform configuration is not release-ready

- **Severity:** MEDIUM
- **Category:** Other
- **Type:** Other
- **Location:** `meno/apps/mobile/android/app/build.gradle.kts:1–5,37–49`; `android/gradle.properties`; `android/app/src/main/AndroidManifest.xml`; `ios/Runner.xcodeproj/project.pbxproj`; `ios/Runner/Info.plist`.
- **What's wrong:** Release uses debug signing. Inspected platform trees lack Firebase config and iOS entitlements wiring. Referenced notification icon is absent. Android has unnecessary/protected permissions and required camera hardware. Kotlin plugin/application configuration needs build verification.
- **Why it matters:** Release provisioning, push, device eligibility, and background behavior are not established.
- **Skill reference:** None specific.
- **Recommended fix:** Production signing/provisioning pipeline, real notification resources, permission audit, platform build/device validation. Missing config may be supplied by CI; that remains **needs verification**.
- **Effort:** M
- **Fix before mirroring?** Yes for platform integration templates

### [M26] UI presents incomplete behavior as operational

- **Severity:** MEDIUM
- **Category:** Other
- **Type:** Under-engineered
- **Location:** `meno/apps/mobile/lib/features/broadcast/pages/broadcast_details_page.dart:117–120`; `ended_broadcast_page.dart:105–107`; `broadcast_editor_page.dart:424–432`; `widgets/add_cohost_modal.dart:33–74`; `features/settings/pages/security_settings_page.dart:25–31`; `settings_page.dart:113–118`.
- **What's wrong:** Restream, publish/listen-back, password save, and deletion actions are enabled no-ops. Cohost UI uses fake profiles, unwired search, inconsistent limits, and an unbounded-flex layout.
- **Why it matters:** These are not completed reference flows. Reconnecting the modal exposes additional runtime defects.
- **Skill reference:** `flutter-architecture-expert`: honest capability/state presentation.
- **Recommended fix:** Disable/hide unsupported actions; implement coherent capabilities before labeling the feature complete. Bound the list and use real search.
- **Effort:** S to hide; M–L to implement
- **Fix before mirroring?** Yes

### LOW

### [L01] Rebuild granularity is too broad for a one-second clock

- **Severity:** LOW
- **Category:** Performance & efficiency
- **Type:** Performance
- **Location:** `meno/apps/mobile/lib/features/broadcast/widgets/broadcast_tab_widget.dart:12–26,28–78`.
- **What's wrong:** Clock changes rebuild the whole tab, including artwork and structural UI.
- **Why it matters:** Avoidable repeated work during long sessions. Actual frame impact is **not measured**.
- **Skill reference:** `watch-it-expert` / architecture skill: split when high-frequency state changes rebuild larger trees.
- **Recommended fix:** A small watching timer widget; retain stable artwork/header. Do not split every field into a widget.
- **Effort:** S
- **Fix before mirroring?** No

### [L02] Derived observables are recreated by getters

- **Severity:** LOW
- **Category:** Reactive state (watch_it / listen_it)
- **Type:** Performance
- **Location:** `meno/apps/mobile/lib/features/auth/manager/auth_manager.dart:182–190`; `user_manager.dart:29–34`; `features/broadcast/manager/live_scope_manager.dart:115`.
- **What's wrong:** Each getter access creates a new `select` chain. Cached watch selectors limit some creation, but imperative getter access does not.
- **Why it matters:** Unnecessary derived objects/subscriptions and unclear disposal.
- **Skill reference:** `listen-it-expert`: operators return new objects; retain reusable derived state.
- **Recommended fix:** Cache derived observables in fields and own their lifecycle where required. Do not claim every getter invocation is a proven permanent leak.
- **Effort:** S
- **Fix before mirroring?** Yes as a reactive-state template

### [L03] Timer semantics and retry timing are inaccurate

- **Severity:** LOW
- **Category:** Anti-patterns & code smells
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/manager/broadcast_timer_manager.dart:92–94,140–142`; `_shared/services/http_client.dart:600–605`.
- **What's wrong:** First tick discards a larger `initialElapsed`; wall-clock changes affect duration despite comments. Retry jitter can shorten `Retry-After`.
- **Why it matters:** Timer jumps and premature retry under throttling.
- **Skill reference:** None specific.
- **Recommended fix:** Monotonic elapsed anchor with explicit server reconciliation; apply jitter before enforcing the server’s minimum wait.
- **Effort:** S
- **Fix before mirroring?** No

### [L04] Small UI/validation mismatches remain

- **Severity:** LOW
- **Category:** Other
- **Type:** Bug
- **Location:** `meno/apps/mobile/lib/features/broadcast/widgets/broadcast_control_buttons.dart:97–99`; `manager/broadcast_editor_manager.dart:192–194`; `pages/broadcast_editor_page.dart:334`; `widgets/broadcast_card.dart:58–67`.
- **What's wrong:** Listener leave uses host confirmation text; title validation omits backend 3–100 policy; UI says 10 MB while backend image limit is 5 MiB; “live count” uses cumulative attendance.
- **Why it matters:** Misleading actions/counters and avoidable server errors.
- **Skill reference:** None specific.
- **Recommended fix:** Align copy, validation, and metrics with actual capabilities/contracts.
- **Effort:** S
- **Fix before mirroring?** No

### [L05] Dead paths, fixtures, and documentation obscure the reference implementation

- **Severity:** LOW
- **Category:** Folder & file structure
- **Type:** Structure
- **Location:** `meno/apps/mobile/lib/features/broadcast/widgets/live_broadcast_tab.dart:36–44`; `model/entities.dart:40–166`; `features/profile/manager/user_profile_manager.dart:3–8`; `meno/apps/mobile/README.md`.
- **What's wrong:** Duplicate live-tab implementation resolves an unregistered timer; production models contain mutable sample lists; redundant imports remain; README is a template. Multiple comments describe behavior the code no longer implements.
- **Why it matters:** Developers can copy the wrong path and mistake commentary for verified architecture.
- **Skill reference:** `flutter-architecture-expert`: lean feature boundaries.
- **Recommended fix:** Remove or clearly isolate unused implementations/fixtures, correct lifecycle comments, document bootstrap and decisions.
- **Effort:** S
- **Fix before mirroring?** Yes for reference cleanup

### [L06] Language setting is not connected to localization

- **Severity:** LOW
- **Category:** Other
- **Type:** Under-engineered
- **Location:** `meno/apps/mobile/lib/features/settings/manager/settings_manager.dart:29–38`; `meno/apps/mobile/lib/app.dart:85–94`; hard-coded text throughout reviewed views.
- **What's wrong:** Language is stored remotely, but application locale/localization delegates are not wired in the app frame.
- **Why it matters:** The setting does not establish i18n readiness; adding four features multiplies hard-coded copy.
- **Skill reference:** None specific.
- **Recommended fix:** Introduce localization before expanding UI copy; explicitly label unsupported language changes meanwhile.
- **Effort:** M
- **Fix before mirroring?** No for lifecycle patterns; yes before broad UI expansion

### INFO

### [I01] The feed base mixes unused page arithmetic with cursor pagination

- **Severity:** INFO
- **Category:** Over-engineering
- **Type:** Over-engineered
- **Location:** `meno/apps/mobile/lib/_shared/manager/feed_data_source.dart:100–120,171–175`; `broadcast_feed_data_source.dart:55–69`.
- **What's wrong:** The base carries numbered-page state, while the implemented backend uses opaque cursors and the concrete source overrides it.
- **Why it matters:** New features may copy the wrong pagination model.
- **Skill reference:** `feed-datasource-expert`: cursor-based source.
- **Recommended fix:** Standardize the current shared base on the actual cursor contract; add numbered pagination only when an endpoint needs it.
- **Effort:** S
- **Fix before mirroring?** Yes

### [I02] Dependency risk is explicit, not proven obsolescence

- **Severity:** INFO
- **Category:** Other
- **Type:** Other
- **Location:** `meno/apps/mobile/pubspec.yaml:38–39,55–62`; `analysis_options.yaml:76–111`.
- **What's wrong:** ObjectBox runtime/native/generator all use `6.0.0-preview.4`; both Mockito and Mocktail are present. No evidence supports calling packages outdated.
- **Why it matters:** A prerelease database deserves migration/device validation; duplicate mocking conventions add maintenance choice.
- **Skill reference:** `flutter-it`: use only necessary building blocks.
- **Recommended fix:** Document preview acceptance and migration policy; standardize mocking when tests are repaired. Keep the strict analyzer baseline.
- **Effort:** S–M
- **Fix before mirroring?** No

### Review-dimension coverage

| Dimension             | Assessment                                                                                                                                                                                   |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Architecture/layering | Ownership and shared-layer issues: H01–H04, M14, M21–M22. No rewrite needed.                                                                                                                 |
| Folder/file structure | Feature-first foundation is suitable; shared feature leakage and dead paths need cleanup.                                                                                                    |
| DI                    | Account boundary sound; runtime readiness and live ownership unsafe.                                                                                                                         |
| Reactive state        | Cached widget-local creation is useful; wrong proxy target, duplicate effects, recreated derived state need fixes. No general watch-order violation is asserted for safe early-return paths. |
| Commands              | User action commands are useful; internal lifecycle pipeline and incomplete busy restrictions are not.                                                                                       |
| Feeds                 | Existing lists are real, not hypothetical; cursor race/error/empty-page defects block reuse.                                                                                                 |
| Performance           | Virtualized list builders and cached images are present. No measured frame/startup/memory result is claimed. Resource and subscription defects are higher priority than missing `const`.     |
| Scalability           | Current patterns break on high-frequency events, offline edits, and account-relative caches.                                                                                                 |
| Over-engineering      | Live registration protocol, internal command graph, redundant account tracking, unused pagination state.                                                                                     |
| Under-engineering     | Account identity, reconciliation, durable sync ownership, notification coordination, integration tests.                                                                                      |
| Anti-patterns         | `late` lifecycle dependency, mixed identities, misleading comments, enabled no-ops.                                                                                                          |
| Error resilience      | Typed failures exist; refresh classification, terminal disconnects, and cleanup are defective.                                                                                               |
| Data/persistence      | Owner keys partly implemented; atomicity, stable identity, schema separation, and viewer-relative isolation need work.                                                                       |
| Routing               | Guard baseline present; readiness, pending intent, named route, and ended-route ownership fail.                                                                                              |
| Security              | Secure storage is appropriate; cross-account replay/logout/revocation issues are blockers. No production secret leak is asserted.                                                            |
| Testing               | Strict lint configuration is useful; critical suites disabled and no device/protocol validation established.                                                                                 |
| Realtime              | Typed vocabulary exists, but transport mismatch and inactive initialization are decisive.                                                                                                    |
| Other                 | Push/release/background/i18n gaps identified. Accessibility requires device semantics, contrast, and large-text testing; source inspection alone does not establish a pass.                  |

---

## 4. Over-Engineering Ledger

| Item                                                                          | Location                                                                         | Why excessive                                                         | Simpler alternative                                                        | Justified by planned features?                     |
| ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------- | --------------------------------------------------------------------- | -------------------------------------------------------------------------- | -------------------------------------------------- |
| Broadcast scope plus scope coordinator plus session manager plus init polling | `live_scope_manager.dart`; `live_session_init_page.dart`; `locator.dart:320–389` | Registration existence becomes a state protocol                       | Account owner with nullable resource session and phase                     | **No**, not in its present form                    |
| Tokenless live scope rebuilt after join                                       | `live_scope_manager.dart:314–334`                                                | Creates resource identity before actual entry and then replaces graph | Lobby state is plain route-local state; session only after accepted entry  | No                                                 |
| Internal connect/announce/settle command pipeline                             | `live_session_manager.dart:40–167`                                               | Commands add observables/disposal to non-UI sequencing                | One async method with explicit phases                                      | No                                                 |
| Timer methods as six commands                                                 | `broadcast_timer_manager.dart:21–26`                                             | No meaningful async/error UI benefit                                  | Plain methods; expose elapsed/running                                      | No                                                 |
| Account-local managers following global account changes                       | `my_profile_manager.dart`; `settings_manager.dart`                               | Duplicates scope lifetime and creates transition races                | Immutable account context                                                  | No                                                 |
| Numbered pagination plus cursor override                                      | `feed_data_source.dart:100–120`                                                  | Two models, one actually used                                         | Cursor base                                                                | No                                                 |
| Shared placeholder profile proxy                                              | `user_profile_proxy.dart:170–185`                                                | Deduplicates “unknown” objects that are not the same entity           | ID-bearing loading state                                                   | No                                                 |
| Reference-counted profile repository                                          | `user_profile_proxy.dart:163–204`                                                | Current implementation is wrong, but shared identity is useful        | Correct stable-key repository; plain entities where sharing is unnecessary | **Yes**, for actors appearing in multiple features |
| `RoomTransport` abstraction                                                   | `_shared/services/room_transport.dart`                                           | **Not excessive:** real SDK boundary plus fake/testing implementation | Keep                                                                       | **Yes**                                            |
| All-feature/shared umbrella exports                                           | `_shared/_shared.dart`; `features/features.dart`                                 | Hide dependency direction; pull feature-specific code into “shared”   | Narrow imports and small feature barrels                                   | No                                                 |
| Duplicate live-tab implementations                                            | `broadcast_tab_widget.dart`; `live_broadcast_tab.dart`                           | Two divergent lifecycle assumptions                                   | One routed implementation                                                  | No                                                 |
| Huge explanatory comments for unstable behavior                               | Scope/session/proxy files                                                        | Comments repeatedly claim guarantees absent from code                 | Short invariants backed by tests                                           | No                                                 |
| Separate DTO/domain/ObjectBox rows                                            | Feature models/services                                                          | **Not inherently excessive:** API, validation, and persistence differ | Keep boundaries, avoid mechanical duplicate layers where unnecessary       | Yes, especially notes sync                         |
| Account scope                                                                 | `UserScopeManager`                                                               | **Not over-engineering** for multi-account ownership                  | Keep with precise lifecycle fixes                                          | Yes                                                |

---

## 5. What's Done Well (Safe to Mirror)

Copy these patterns selectively—not their surrounding defects.

1. **Constructor-injected services/managers**
   - Example: `meno/apps/mobile/lib/features/profile/manager/my_profile_manager.dart:27–32`.
   - This makes unit tests independent of global scope construction.
   - Do not replace it with service-locator lookups inside every method.

2. **A narrow external media adapter**
   - `meno/apps/mobile/lib/_shared/services/room_transport.dart:24–55`.
   - LiveKit details remain in `livekit_client.dart`.
   - Listener-before-connect and explicit disposal are correct directions.

3. **Widget-owned feed sources**
   - `meno/apps/mobile/lib/features/broadcast/pages/home_page.dart:132–139,176–183,220–227`.
   - Matches `feed-datasource-expert` ownership. Do not turn each feed into an account singleton.

4. **Separate refresh and pagination commands, and near-end prefetch**
   - `meno/apps/mobile/lib/_shared/manager/feed_data_source.dart:24–28,87–90,149–155`.
   - Keep the separation and `length - 3` trigger; add arbitration and correct error channels.

5. **Reacting to successful command completion rather than `isRunning == false`**
   - `meno/apps/mobile/lib/features/broadcast/widgets/pre_stream_modal.dart:57–62`.
   - Matches `watch-it-expert` and `command-it-expert`.

6. **Nullable optimistic override fields on public profile proxies**
   - `meno/apps/mobile/lib/features/profile/model/proxies/user_profile_proxy.dart:60–74,91–110`.
   - The override idea matches the architecture skill. Fix repository identity and stale-response arbitration before reusing the whole proxy.

7. **Some explicit late-response guards**
   - `meno/apps/mobile/lib/features/profile/manager/my_profile_manager.dart:103–125`.
   - Its disposed/account checks are a useful local example, though immutable account context is preferable within an account scope.

8. **Media picking treats cancellation as cancellation**
   - `meno/apps/mobile/lib/_shared/services/media_service.dart:37–53`.
   - No broad up-front storage permission, null cancellation, typed permission failures.

9. **Typed transport errors, response envelopes, and request correlation**
   - `meno/apps/mobile/lib/_core/exceptions/`; `_shared/services/http_client.dart`.
   - Preserve these boundaries. Fix error classification and production reporting.

10. **Strict analyzer baseline**
    - `meno/apps/mobile/analysis_options.yaml:76–111`.
    - No reason to weaken it to accommodate the current defects.

---

## 6. Template Readiness for Upcoming Features

### Chats

**Backend reality**

This is broadcast-room chat, not a general direct-message/conversation system.

- History: `GET /broadcasts/{id}/chat/messages`.
- Requires current participant presence.
- Cursor order: descending `(created_at,id)`; subsequent pages are older messages.
- Writes: raw WS `sendMessage`, `editMessage`, `deleteMessage`, `sendReaction`.
- Message length: 1–256; reaction length: 1–32.
- Reactions belong to broadcasts, not individual messages.
- Edit ownership/window rules apply; window is 900 seconds.
- No client message ID, idempotent-send contract, or Socket.IO ack contract.
- No durable room-event replay.

Sources: `meno/apps/api/src/modules/chat/handlers.rs:21–44`; `service.rs:91–323`; `dto.rs`; `meno/apps/api/src/routes/ws.rs:577–767`.

**Carries over**

- Typed external adapter.
- Widget-owned paginated source.
- ID-based entities and small watching message widgets.
- Account connection ownership.

**Breaks if copied**

- Feed-wide rebuilding per event.
- Snapshot replacement overwriting live deltas.
- Current Socket.IO transport.
- Blind retry of a send without server deduplication.
- Shared empty proxies.
- Assuming reconnect supplies missed edits/deletes.

**Placement**

- Account: connection, message repository/cache, delivery coordination.
- Room/session or widget-owned controller: subscriptions, typing/presentation, history viewport.
- Multiple rooms need keyed ownership—not a single global nullable chat session.

**Build first**

Protocol alignment, account identity, stable message identity, snapshot/delta reconciliation, explicit pending/failed send states, reconnect resync, and H23.

### Notes/folders

**Backend reality**

- CRUD and folder-assignment routes exist.
- Client-selected UUIDs.
- Versioned update/delete via `baseVersion`; conflicts return `409 VERSION_CONFLICT`.
- Separate notes/folders sync cursors over ascending `(updated_at,id)`, including tombstones.
- Push returns ordered `APPLIED`, `CONFLICT`, or `REJECTED`.
- Sync mutation fields include snake_case even though surrounding DTOs use camelCase.
- `serverTime` is not a checkpoint.
- Optional idempotency has finite retention; it is not permanent deduplication.
- No notes/folder realtime events.
- Note PATCH currently writes folder assignment unconditionally: omission can unfile a note.
- Folder deletion normally unfiles notes; optional deletion removes them.
- Same-batch folder assignment defect: H22.

Sources: `meno/apps/api/src/routes/notes.rs:51–91`; `modules/notes/dto.rs:264–521`; `service.rs:348–446`; `repository/pg.rs:472–496,828–915`.

**Carries over**

- One app-wide DB handle.
- Feature-local persistence rows.
- Constructor-injected services.
- Widget-local editor state.

**Breaks if copied**

- Shared-preference snapshots as the source of truth.
- Nontransactional upserts.
- Optional owner checks.
- Per-keystroke server operations without durable outbox/version state.
- Last response wins.
- Treating conflicts as generic toasts.

**Placement**

- Account: owner-bound repository, sync manager, outbox/conflict state.
- Widget: editor/controller and paginated browsing source.
- Sync lifetime must not depend on the editor remaining mounted.

**Build first**

Transactional owner-qualified storage, durable outbox, cursor persistence, tombstone handling, conflict UI/model, stable operation keys, and backend PATCH/same-batch fixes.

### Discover

**Backend reality**

There is no dedicated Discover endpoint.

Available:

- Broadcast listing with creator/status/order/cursor/limit.
- User search via `GET /users?q=...`.
- User search minimum three characters, English full-text matching, newest-account-first—not relevance ranking.
- No broadcast keyword/relevance/popularity/following-only filter.
- Global invalidation is a refetch nudge, not durable synchronization.

Sources: `meno/apps/api/src/modules/broadcast/dto.rs:201–245`; `repository/pg.rs:364–397`; `modules/profile/dto.rs:114–165`; `repository/pg.rs:159–194`.

**Carries over**

- Query objects and opaque cursor handling.
- Virtualized lists and cached image widgets.
- Page-owned sources.

**Breaks if copied**

- `forYou` and `subscriptions` currently return general live listings.
- `updateKeywords()` is effectively a no-op.
- Client filtering cannot emulate global ranking/search.
- Mutable-query races.
- Empty filtered pages.
- Per-event refetch storms across retained tabs.

Evidence: `meno/apps/mobile/lib/features/broadcast/model/broadcast_query.dart:137–161,265–275`.

**Placement**

- Account: viewer-relative profile repository/cache.
- Widget: debounced query and feed source.
- No Discover scope.

**Build first**

Correct cursor source, query generations/cancellation, stable filter reset semantics, and actual product-specific backend filters. Label unavailable personalization honestly.

### Notifications

**Backend reality**

**A durable inbox is not implemented.** What exists is settings/token infrastructure, FCM adapters, and a socket payload builder.

- One push-token field per user—not multi-device registration.
- Socket payload lacks durable notification ID/read state/navigation target.
- Direct-user offline buffer: best effort, 50 events, 300 seconds.
- Room/global events are not buffered.
- Replay is not exactly-once and can duplicate.

Sources: `meno/apps/api/src/modules/mod.rs:16–21`; `routes/settings.rs:36–55`; `infrastructure/ws/dto.rs:90–101`; `infrastructure/constants.rs:75–97`; `ws/mod.rs:402–541`.

**Carries over**

- Root platform adapters plus account-owned coordination.
- Typed events and page-owned cursor source after the feed fixes.

**Breaks if copied**

- Socket events treated as a durable inbox.
- Token ownership tied to whichever account is active at callback time.
- Navigation immediately from platform callbacks.
- Subject-only caches for viewer-specific read state.
- Assuming short replay buffer prevents missed notifications.

**Placement**

- Root: Firebase/native callback adapter and pending tap ingestion.
- Account: notification coordinator/repository/unread state and token association.
- Widget: inbox pagination.
- Never a scope per notification.

**Build first**

Backend inbox contract, durable IDs, read/unread mutations, recipient attribution, multi-device token policy, replay deduplication, and pending navigation through auth/account readiness.

---

## 7. Recommended Reference Architecture

Use the current feature-first organization. Correct boundaries rather than renaming everything.

```text
lib/
  app/
    bootstrap/
      locator.dart
      account_transition_manager.dart
    routing/
    layouts/
      authenticated_shell.dart

  _core/
    config/
    exceptions/
    logging/
    value_objects/

  _shared/
    services/
      database.dart
      http_transport.dart
      realtime_connection.dart
      media_service.dart
      platform_push_adapter.dart
    feeds/
      cursor_feed_source.dart
      feed_widget.dart
    widgets/
    model/
      cursor_page.dart

  features/
    auth/
      manager/
      model/
      services/
        session_store.dart
        auth_http_service.dart

    broadcast/
      manager/
        active_broadcast_manager.dart
      session/
        active_broadcast_session.dart
        participants_controller.dart
      services/
        broadcast_http_service.dart
        broadcast_events.dart
        livekit_room_transport.dart
      feeds/
        broadcast_feed_source.dart
      model/
      pages/
      widgets/

    profile/
      manager/
      model/
      services/
      pages/
      widgets/

    chat/
      manager/
      model/
      services/
      feeds/
      pages/
      widgets/

    note/
      manager/
        notes_sync_manager.dart
      model/
      services/
      feeds/
      pages/
      widgets/

    discover/
      feeds/
      pages/
      widgets/

    notification/
      manager/
      model/
      services/
      feeds/
      pages/
      widgets/
```

### Layering convention

- **Services:** one external boundary; decode wire/storage formats; no navigation.
- **Managers:** semantically related business orchestration; immutable account context.
- **Session objects/controllers:** ordinary owned disposable resources, not mandatory DI registrations.
- **Views:** observe state, invoke user commands, perform presentation/navigation side effects.
- **Feeds:** widget-owned query/window state with generation-safe requests.
- **Repositories:** only where shared identity, local truth, or synchronization requires them. Do not create an interface/repository/use-case stack for every endpoint.
- **Errors:** value-object validation may use `Either`; transport/mutation failures may throw typed exceptions into commands. That division is acceptable if documented. Do not wrap every call in both `Either` and exceptions.
- **Optimistic updates:** use overrides where shared proxies need them; do not mandate proxy machinery for every immutable list row.

### DI scope map

```text
root
  app-lifetime infrastructure
  session vault/auth/account-transition coordination
  router/platform callbacks

authenticated
  immutable account context
  account connection/repositories/sync/coordinators
  active broadcast owner

widget-owned
  forms/details/feeds/local controllers
```

**Differences from current structure:** remove broadcast DI scope; move feature-specific shared code back into features; introduce an explicit app composition layer; stop account-local managers following the global selector; standardize cursor feeds.

---

## 8. Prioritized Action Plan

1. **Close account-isolation defects first.**  
   Bind request/retry/refresh to originating identity; serialize conditional credential mutations; make explicit logout remove usable credentials.  
   **Effort:** M, approximately 3–6 days including deterministic race tests.  
   **Why above scope simplification:** C01–C03 can affect private data and authentication.

2. **Fix backend revocation and premature reset-request revocation.**  
   Verify protected routes using actual guard wiring.  
   **Effort:** M, approximately 1–3 days.

3. **Repair account transition readiness and UI lifetime.**  
   Publish transition/generation, gate authenticated UI, remount by generation, stop old-account operations before activating new UI.  
   **Effort:** M, approximately 2–4 days.

4. **Align realtime transport and initialize it deliberately.**  
   Raw WebSocket adapter, actual handshake test, heartbeat, credential refresh, bounded reconnect, typed event decoding.  
   **Effort:** M, approximately 2–4 days.

5. **Repair live teardown before changing its structure.**  
   Cover uninitialized setup, failed REST exit, terminal events, replacement, logout, and recovery races. Keep ended results outside disposed resources.  
   **Effort:** M, approximately 2–4 days.

6. **Migrate to account-owned nullable active session.**  
   Follow Section 1’s staged migration; preserve `RoomTransport`, remove registration polling and internal command graph.  
   **Effort:** M, approximately 3–5 days, overlapping lifecycle repair.

7. **Fix broadcast/backend contracts and lifecycle safety.**  
   End/cohost DTOs, cohost revocation, ended-row restart, partial commit/retry semantics, stable operation keys.  
   **Effort:** M–L, approximately 3–7 days across client/backend.

8. **Make shared feeds safe before building another feed.**  
   Query generations, refresh/page arbitration, trailing invalidation, pagination errors, filtered-page progress, cancellation, and subscription rebinding.  
   **Effort:** M, approximately 2–4 days.

9. **Repair shared identity and persistence boundaries.**  
   Stable profile keys, viewer-relative caches, transactional owner-qualified local operations, reactive favourites, absolute auth expiry.  
   **Effort:** M, approximately 3–5 days.

10. **Fix auth/profile/settings functional defects.**  
    OTP resend, add-account mode, validation parity, profile change detection, current-user presentation, settings ordering/key preservation.  
    **Effort:** M, approximately 2–4 days.

11. **Establish upcoming-feature contracts before UI expansion.**
    - Chats: fix cross-room deletion and define send identity/reconciliation.
    - Notes: fix folder/PATCH semantics, then design outbox/conflicts.
    - Discover: implement real filters/search requirements.
    - Notifications: implement durable inbox and device registration policy.  
      **Effort:** feature-specific M–L; do not estimate these as copying existing screens.

12. **Validate release and background behavior on devices.**  
    Signing/Firebase/APNs, cold/warm taps, account token switching, locked-screen audio, permission denial, interruptions, process death, reconnect.  
    **Effort:** M–L, approximately 3–6 days once provisioning is available.

13. **Clean the reference surface.**  
    Remove dead duplicate tabs, isolate fixtures, disable no-ops, narrow shared boundaries, restore executable lifecycle tests, document bootstrap.  
    **Effort:** S–M, approximately 1–3 days.

14. **Then optimize measured hot spots.**  
    Isolate one-second timer rebuilding; inspect image decode sizing, retained tab activity, query cost, and high-frequency event batching with profiles.  
    **Effort:** S initially; further work only from measurements.

15. **Leave these alone:** account-scoped DI as the multi-account boundary, constructor injection, one app-wide ObjectBox handle, narrow media adapter, widget-owned feeds, typed failures, and the strict lint baseline.

**Bottom line:** keep the architectural vocabulary, but do not use auth/profile/live wholesale as reference implementations yet. The account boundary is worth preserving; request identity, lifecycle ownership, reconciliation, and persistence guarantees must become real before four more features depend on them.
