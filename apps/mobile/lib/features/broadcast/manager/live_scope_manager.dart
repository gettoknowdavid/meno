import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// Registers a live broadcast's own services in that broadcast's scope.
typedef LiveScopeRegistrar = FutureOr<void> Function(
  GetIt scope,
  LiveSession session,
);

/// The live tier: one get_it scope per broadcast session.
///
/// The account tier answers "who is signed in"; this one answers "which
/// broadcast is on air", and exists so that everything belonging to a session —
/// the LiveKit client, the session manager, participants, chat — is created
/// when the session starts and disposed the moment it ends, with nothing left
/// to remember to tear down. Its registrar is where those services go.
///
/// It is registered inside the account's scope, so a sign-out takes the session
/// with it: nothing live may outlive the account that started it.
///
/// # Who opens and closes a room
///
/// The managers that own the API's answer do, and none of them has to know what
/// a session needs: going live (`BroadcastEditorManager.startBroadcast`) and
/// joining (`StreamManager.joinBroadcast`) both call [start] with the session
/// the API minted, and the room is closed by the client leaving it
/// (`LiveSessionManager.endSession`) or by the broadcast ending under it, which
/// the socket announces. Nothing else has to remember to push or pop a scope.
////// # Rooms that outlive the process
///
/// [init] puts the device back into a session that was still open when the app
/// stopped, which [LiveSessionStore] has the record of. Transitions are
/// serialised, and an
/// [end] naming another broadcast is ignored — a late `endedBroadcast` for the
/// previous session must not close the one that replaced it.
///
/// # Why this manager outlives the session
///
/// This manager is registered in the account's scope and answers for the
/// account — *is a room open, and which one* — while the session's own
/// services answer only inside a room's scope. That split is what lets a
/// screen outside the room (the home page's banner, a card, the details
/// sheet) ask whether the account is on air without resolving anything that
/// does not exist yet: everything a widget needs to draw that answer is
/// registered for as long as the account is. Registering a session object in
/// the account's scope instead would put a room's identity where it can never
/// be disposed with the room.
final class LiveScopeManager implements Disposable {
  new(
    this._userId, {
    required this._logger,
    required this._store,
    required this._http,
    required this._endedBroadcasts,
    this._register,
  });

  final Id _userId;
  final MenoLogger _logger;
  final LiveSessionStore _store;
  final BroadcastHttpService _http;

  /// Broadcasts ending, as the socket reports them.
  ///
  /// A stream rather than the socket service: this manager has nothing to say
  /// to a room, so it is given only what it listens to.
  final Stream<EndedBroadcast> _endedBroadcasts;

  final LiveScopeRegistrar? _register;

  StreamSubscription<EndedBroadcast>? _endedSubscription;

  bool _disposed = false;

  final ValueNotifier<Id?> _broadcastId = ValueNotifier(.empty);

  /// Whether the open scope was opened with the API's session for its room.
  ///
  /// A deep link opens a room before anything has been joined, and that room
  /// has no token in it; the join that follows rebuilds the scope so the
  /// registrations that need one happen at all.
  bool _hasSession = false;

  Future<void> _queue = Future<void>.value();

  /// The broadcast on air, or null when there is none.
  ValueListenable<Id?> get activeBroadcastId => _broadcastId;

  /// Whether a session is open.
  ValueListenable<bool> get isLive => _broadcastId.select((id) => id != null);

  /// Whether this device is in [broadcastId]'s room right now.
  ///
  /// The session's own registrations live in that room's scope, so they are
  /// absent — not merely empty — until the room is entered: this is the
  /// question to ask before resolving anything of the room's, which is why
  /// there is no untyped `isRegistered` for callers to get wrong.
  bool isInRoom(Id broadcastId) {
    return _broadcastId.value == broadcastId && di.isRegistered<LiveSession>();
  }

  /// Restores the session this device was in, then follows the socket.
  ///
  /// The recovery is not awaited: it asks the API whether the broadcast is
  /// still on air, and a launch should not wait on a request before it draws
  /// its first screen. Until it answers, the app is simply not in a room.
  Future<LiveScopeManager> init() async {
    _endedSubscription = _endedBroadcasts.listen(
      (ended) => unawaited(end(ended.broadcastId)),
      onError: (Object error) => _logger.w(
        'Ended broadcast stream failed',
        tag: MenoLogTag.network.label,
        error: error,
      ),
    );
    unawaited(_recover());
    return this;
  }

  /// Opens the session scope for [broadcastId].
  ///
  /// [session] is the API-issued session for the room — the LiveKit token and
  /// the account's standing in it — when the caller already has one. It is
  /// registered in the scope, because everything that *enters* a room needs it
  /// and the room's own id does not carry a token. A scope opened without one
  /// is a room that can be listed but not entered: that is the state a deep
  /// link to a broadcast is in before the join answers, and it is why the
  /// registrations that need a token are conditional in
  /// [`_registerLiveScope`](../../../locator.dart).
  Future<void> start(Id broadcastId, {BroadcastSession? session}) {
    return _enqueue(() => _open(broadcastId, session));
  }

  /// Closes the session, or the one [broadcastId] names.
  Future<void> end([Id? broadcastId]) => _enqueue(() => _close(broadcastId));

  @override
  FutureOr<dynamic> onDispose() async {
    _disposed = true;
    await _endedSubscription?.cancel();
    _endedSubscription = null;

    final broadcastId = _broadcastId.value;
    _broadcastId.value = null;
    if (broadcastId != null) await _drop(Scope.liveFor(broadcastId));
    _broadcastId.dispose();
  }

  /// Puts the device back into the session it was in when the app stopped.
  ///
  /// The record is trusted only as far as the API confirms it. A session that
  /// has aged out, or that names a broadcast which is no longer on air, is
  /// dropped — and so is one the API cannot answer for, because a room nobody
  /// is in is worse than no room at all: it would be drawn as if it were one.
  Future<void> _recover() async {
    final stored = _store.read();
    if (stored == null || stored.isEmpty) return;

    if (stored.isExpired) {
      _logger.i('Dropping a live session that had aged out');
      await _forget();
      return;
    }

    try {
      final broadcast = await _http.getBroadcast(stored.broadcast.id);
      // The account can be signed out while the API is answering, and a scope
      // must not be pushed on top of a stack that is being taken down.
      if (_disposed) return;

      if (!broadcast.isLive) {
        _logger.i('Dropping a live session for a broadcast that has ended');
        await _forget();
        return;
      }

      _logger.i(
        'Recovering the live session for ${stored.broadcast.id.orEmpty}',
      );
      await start(stored.broadcast.id, session: stored);
    } on MenoException catch (error, stackTrace) {
      _logger.w(
        'Could not check a live session to recover it (${error.code})',
        tag: MenoLogTag.di.label,
        error: error,
        stackTrace: stackTrace,
      );
      await _forget();
    }
  }

  /// Remembers [session] so a process that dies here can come back to the room.
  Future<void> _remember(BroadcastSession session) async {
    try {
      await _store.save(session);
    } on Object catch (error, stackTrace) {
      // The room is open either way; what is lost is coming back to it after a
      // restart, which is not worth failing the session over.
      _logger.w(
        'Could not remember the live session',
        tag: MenoLogTag.di.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Forgets the remembered session.
  Future<void> _forget() async {
    try {
      await _store.clear();
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Could not forget the live session',
        tag: MenoLogTag.di.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Runs [operation] after every change queued before it.
  ///
  /// Two transitions in flight at once would interleave a push and a pop on the
  /// same scope stack; a failed one is logged rather than raised, because it is
  /// usually a socket event's reaction and nothing is waiting on the answer.
  Future<void> _enqueue(Future<void> Function() operation) {
    final next = _queue.then((_) => operation()).onError<Object>((
      error,
      stackTrace,
    ) {
      _logger.w(
        'Could not change the live scope',
        tag: MenoLogTag.di.label,
        error: error,
        stackTrace: stackTrace,
      );
    });
    _queue = next;
    return next;
  }

  /// Pushes the scope for [broadcastId], replacing any open session.
  ///
  /// A room that is already open stays open — its services are what the screen
  /// is drawing — with one exception: joining a room that was opened without a
  /// session rebuilds it. The registrations that need the token were skipped
  /// the first time (see [`_registerLiveScope`](../../../locator.dart)), so
  /// there is nothing to keep, and re-registering into a live scope from here
  /// would land in whatever scope happens to be current rather than this
  /// room's.
  Future<void> _open(Id broadcastId, BroadcastSession? broadcast) async {
    if (_broadcastId.value == broadcastId) {
      if (broadcast == null || _hasSession) return;
    }

    await _close();

    final live = LiveSession(broadcastId: broadcastId, userId: _userId);
    await di.pushNewScopeAsync(
      scopeName: Scope.liveFor(broadcastId),
      init: (scope) async {
        scope.registerSingleton<LiveSession>(live);
        // Only when the room is being entered: a scope opened from a deep link
        // before the join answered has no token, and the registrations that
        // need one are skipped in `_registerLiveScope` for the same reason.
        if (broadcast != null) {
          scope.registerSingleton<BroadcastSession>(broadcast);
        }
        await _register?.call(scope, live);
      },
    );
    _hasSession = broadcast != null;
    _broadcastId.value = broadcastId;

    // Only a room that was entered with a token is worth coming back to: the
    // one a deep link opens can be listed but not joined.
    if (broadcast != null) await _remember(broadcast);
  }

  /// Pops the open session, unless [broadcastId] names a different one.
  Future<void> _close([Id? broadcastId]) async {
    final live = _broadcastId.value;
    if (live == null) return;
    if (broadcastId != null && broadcastId != live) return;

    await _drop(Scope.liveFor(live));
    _broadcastId.value = null;

    // The session is over, so nothing is left to recover.
    await _forget();
  }

  /// Drops [scopeName] when it is still there — a sign-out may have taken it
  /// down already.
  Future<void> _drop(String scopeName) async {
    if (!di.hasScope(scopeName)) return;
    _hasSession = false;
    await di.dropScope(scopeName);
  }
}
