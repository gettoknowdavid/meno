import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/room_transport.dart';
import 'package:meno_mobile/features/auth/model/auth_context.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/broadcast_http_service.dart';
import 'package:meno_mobile/features/broadcast/services/live_session_store.dart';
import 'package:meno_mobile/features/broadcast/session/active_broadcast_session.dart';

/// How a live entry reaches the room.
enum LiveEnterMode {
  /// The creator is taking their own broadcast on air (`POST …/live`).
  host,

  /// Someone is joining a live broadcast (`POST …/join`).
  listen,

  /// A previous process left this room open; come back to it (`POST …/resume`).
  resume,
}

/// One request to enter a live room.
@immutable
final class LiveEnterRequest {
  const new({required this.broadcastId, required this.mode});

  final Id broadcastId;
  final LiveEnterMode mode;

  /// Whether this entry is a recovered session rather than a fresh one.
  bool get isRecovery => mode == LiveEnterMode.resume;

  @override
  String toString() => 'LiveEnterRequest($broadcastId, ${mode.name})';
}

/// What came of an entry.
enum EnterOutcome { entered, rejected, failed }

/// The answer [ActiveBroadcastManager.enter] gives.
@immutable
final class EnterResult {
  const new({required this.outcome, this.broadcastId, this.failure});

  /// The session is live.
  const new entered(this.broadcastId)
    : outcome = EnterOutcome.entered,
      failure = null;

  /// The entry was abandoned — the account is exiting, or a newer entry
  /// superseded this one. Never a failure to report.
  const new rejected()
    : outcome = EnterOutcome.rejected,
      broadcastId = null,
      failure = null;

  /// The room could not be entered.
  const new failed(this.broadcastId, this.failure)
    : outcome = EnterOutcome.failed;

  final EnterOutcome outcome;
  final Id? broadcastId;
  final LiveFailure? failure;

  bool get isEntered => outcome == EnterOutcome.entered;
}

/// The API calls an entry and an exit make.
///
/// Narrow on purpose: the manager decides *who* may end a broadcast (creator
/// identity) and this answers *how* the call is made, so the decisions stay in
/// one place and a test can drive the manager without a transport.
abstract interface class LiveGateway {
  /// Joins, starts or resumes the room, as [request] says.
  Future<BroadcastSession> enter(
    LiveEnterRequest request,
    AccountContext context,
  );

  /// Ends the broadcast for everyone (creator only).
  Future<EndedBroadcast> end(Id broadcastId);

  /// Stops listening.
  Future<void> leave(Id broadcastId);
}

/// The real gateway: the broadcast endpoints, with the account's identity
/// attached by the manager rather than looked up here.
final class BroadcastLiveGateway implements LiveGateway {
  const new(this._http);

  final BroadcastHttpService _http;

  @override
  Future<BroadcastSession> enter(
    LiveEnterRequest request,
    AccountContext context,
  ) async {
    final dto = switch (request.mode) {
      LiveEnterMode.host => await _http.goLive(request.broadcastId),
      LiveEnterMode.listen => await _http.join(request.broadcastId),
      LiveEnterMode.resume => await _http.resume(request.broadcastId),
    };
    return dto.toDomain(context.accountId);
  }

  @override
  Future<EndedBroadcast> end(Id broadcastId) => _http.end(broadcastId);

  @override
  Future<void> leave(Id broadcastId) async {
    await _http.leave(broadcastId);
  }
}

/// Builds the resource object for one room.
///
/// The manager owns sessions but does not know how a room is built — which
/// transport, which roster — so that stays a supplied factory. Registered in
/// the account scope by the composition root.
typedef LiveSessionFactory = ActiveBroadcastSession Function({
  required BroadcastSession session,
  required LiveServerExit exitOnServer,
});

/// The account's one live owner.
///
/// # One session, one snapshot
///
/// At most one [ActiveBroadcastSession] is open, and it is an ordinary object
/// rather than a DI scope. The previous design made scope *registration* mean
/// "a room is open", which made the room's identity, navigation readiness and
/// connection state three views of the same fact (audit §1.3) — and let two
/// transitions overlap (audit §3 H02). Here every entry, exit and recovery runs
/// on one FIFO, and each operation carries a generation that invalidates the
/// work of any older one.
///
/// # Plain methods, not a command pipeline
///
/// [enter] and [closeForAccountExit] are plain async methods: orchestration is
/// not a user action (audit M21). The two commands are the user-facing pairs —
/// [join] and [leaveOrEnd] — and they only call into those methods.
///
/// # Closing for an account exit
///
/// [closeForAccountExit] memoises its own future, so the transition owner and a
/// disposal cannot both close the same session. That memoisation lives here and
/// nowhere else (review OE-7).
final class ActiveBroadcastManager implements Disposable {
  new({
    required this._context,
    required this._gateway,
    required this._store,
    required this._createSession,
    required this._logger,
    this.endedBroadcasts,
    this.reconnectBudget = const Duration(seconds: 10),
  }) {
    join = .createAsyncNoResult<LiveEnterRequest>((request) async {
      await enter(request);
    });
    leaveOrEnd = .createAsyncNoParamNoResult(exitRoom);

    // One listener for the server's endings: a host-end or a forced end ends
    // this session exactly once, with no reconnect afterwards.
    _endedSubscription = endedBroadcasts?.listen(_onEndedSignal);
  }

  final AccountContext _context;
  final LiveGateway _gateway;
  final LiveSessionStore _store;
  final LiveSessionFactory _createSession;
  final MenoLogger _logger;

  /// The socket's endings, when the manager is wired to one. Null in tests
  /// that drive the room directly.
  final Stream<EndedSignal>? endedBroadcasts;

  /// How long the room may stay down before a fresh entry is fetched.
  ///
  /// The SDK owns the first reconnect attempt; past this budget its old media
  /// token is stale, so a new one is minted rather than reused (audit H07).
  final Duration reconnectBudget;

  /// The one authoritative view of the live broadcast.
  final snapshot = ValueNotifier<LiveSnapshot>(LiveSnapshot.idle());

  ActiveBroadcastSession? _session;

  /// The request the last entry was made with, kept so a failed entry can be
  /// retried without the page guessing a mode.
  LiveEnterRequest? _lastRequest;

  /// The FIFO every entry, exit and recovery is appended to.
  Future<void> _queue = Future<void>.value();

  /// Bumped by every entry and by an account exit; an operation whose
  /// generation is stale does not publish and does not keep its session.
  int _operationGeneration = 0;

  /// Whether an account exit has begun. Once true it never goes back.
  bool _exiting = false;

  /// The memoised account-exit close.
  Future<void>? _exitPending;

  /// The open session's room subscription, and the outage budget.
  StreamSubscription<RoomConnectionState>? _roomSubscription;
  Timer? _outage;
  StreamSubscription<EndedSignal>? _endedSubscription;

  /// Whether the manager itself is closing the room.
  ///
  /// The transport reports this client's own hang-up as a disconnect, which is
  /// not a drop worth reconnecting from.
  bool _closingRoom = false;

  /// Enters a live room: the creator's go-live, a listener's join, or a
  /// recovery. Queued, so two entries cannot overlap.
  Future<EnterResult> enter(LiveEnterRequest request) =>
      _enqueue(() => _enter(request));

  /// The user-facing join.
  late final Command<LiveEnterRequest, void> join;

  /// The user-facing end (creator) or leave (everyone else).
  late final Command<void, void> leaveOrEnd;

  /// Whether a room is open right now.
  bool get hasSession => _session != null;

  /// The open session, for the screen that draws its clock and roster. Null
  /// whenever no room is open.
  ActiveBroadcastSession? get session => _session;

  /// The account this manager belongs to.
  Id get accountId => _context.accountId;

  /// The request the last entry was made with.
  LiveEnterRequest? get lastRequest => _lastRequest;

  /// Re-runs the last entry after a failure.
  Future<EnterResult> retry() => _lastRequest == null
      ? Future.value(const EnterResult.rejected())
      : enter(_lastRequest!);

  /// Puts a recovered session back, if there is one and nothing newer is open.
  ///
  /// Runs inside the queue and calls the *private* entry rather than [enter]:
  /// entering through the queued method from inside a queued job would wait on
  /// itself (D-4).
  Future<void> recover() => _enqueue(() async {
    if (_exiting) return;

    final intent = _store.read();
    if (intent == null) return;

    // Recovery never replaces a newer session.
    if (_session != null) return;

    await _enter(
      LiveEnterRequest(
        broadcastId: intent.broadcastId,
        mode: LiveEnterMode.resume,
      ),
    );
  });

  /// Ends or leaves the room the account is in.
  ///
  /// Plain and awaitable, because it is also the step the pre-stream lobby
  /// takes before opening another room; the command above is only the room's
  /// own button pair.
  Future<void> exitRoom() => _enqueue(() async {
    final session = _session;
    if (session == null) return;

    final phase = snapshot.value.phase;
    if (phase == .failed || phase == .ended) return;

    _closingRoom = true;
    _cancelRoomWatch();

    snapshot.value = LiveSnapshot.room(
      phase: .ending,
      broadcast: session.broadcast,
      accountId: _context.accountId,
      canPublish: session.canPublish,
    );

    final outcome = await session.close(LiveCloseReason.userExit);
    _closingRoom = false;
    if (identical(_session, session)) _session = null;

    // The summary is retained here, outside the closed session, so the ended
    // page never reads a disposed object (audit H04).
    final summary = outcome.summary?.details ?? session.broadcast;

    snapshot.value = LiveSnapshot(
      phase: .ended,
      endReason: outcome.summary?.reason == EndReason.none
          ? null
          : outcome.summary?.reason ?? _realReason(summary),
      endedSummary: summary,
    );

    await _quietly(_store.clear, 'clearing the recovery record');
  });

  /// The account exit contract: quiesce every live operation and close once.
  ///
  /// Idempotent — a second caller awaits the first future — and awaited before
  /// the account scope is popped, so no transport, timer or roster outlives the
  /// account it belonged to.
  Future<void> closeForAccountExit() => _exitPending ??= _closeForAccountExit();

  Future<void> _closeForAccountExit() async {
    _exiting = true;
    // Every in-flight entry is now stale and will publish nothing.
    _operationGeneration++;

    await _queue;

    _closingRoom = true;
    _cancelRoomWatch();

    final session = _session;
    _session = null;
    if (session != null) {
      await session.close(LiveCloseReason.accountExit);
    }
    _closingRoom = false;

    await _quietly(_store.clear, 'clearing the recovery record');
    snapshot.value = LiveSnapshot.idle();
  }

  /// The entry itself. Private and *not* queued: only a queued job runs it.
  Future<EnterResult> _enter(LiveEnterRequest request) async {
    if (_exiting) return const EnterResult.rejected();

    _lastRequest = request;
    final generation = ++_operationGeneration;
    snapshot.value = LiveSnapshot(
      phase: .connecting,
      broadcast: _session?.broadcast,
    );

    // Never overlap two sessions: the old one is closed before the new entry
    // asks the API for anything.
    final previous = _session;
    if (previous != null) {
      _session = null;
      await previous.close(LiveCloseReason.replaced);
    }

    try {
      final details = await _gateway.enter(request, _context);
      if (generation != _operationGeneration || _exiting) {
        return const EnterResult.rejected();
      }

      final session = _createSession(
        session: details,
        exitOnServer: () => _exitOnServer(details),
      );

      // Assigned before start(), so a start that throws still leaves the
      // half-built session findable for the local close below (D-5). The room
      // is watched before connecting, so no event of the join is missed.
      _session = session;
      _watchRoom(session);
      await session.start();

      if (generation != _operationGeneration) {
        // A newer operation owns the app now; this one closes what it opened
        // and publishes nothing.
        if (identical(_session, session)) _session = null;
        await session.close(LiveCloseReason.replaced);
        return const EnterResult.rejected();
      }

      _publishLive(session);
      await _quietly(
        () => _store.save(details),
        'recording the recovery record',
      );

      return EnterResult.entered(session.broadcast.id);
    } on Object catch (error, stackTrace) {
      _logger.e(
        'Could not get into the live room',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );

      await _closeLocal();
      if (generation != _operationGeneration) {
        return const EnterResult.rejected();
      }

      final message = errorMessage(error);
      final failure = _isRetryable(error)
          ? LiveFailure(message: message, retryable: true, cause: error)
          : LiveFailure.terminal(message);
      snapshot.value = LiveSnapshot(phase: .failed, failure: failure);
      return EnterResult.failed(request.broadcastId, failure);
    }
  }

  /// Follows one session's room for as long as it is the open one.
  ///
  /// This is the *only* listener that feeds the published phase; the session's
  /// own reader only pauses the clock. Two listeners that each publish is how
  /// the old manager's `state` and `status` came to disagree (audit H07).
  void _watchRoom(ActiveBroadcastSession session) {
    unawaited(_roomSubscription?.cancel() ?? Future<void>.value());
    _roomSubscription = session.connectionState.listen(
      (state) => _onRoomState(session, state),
    );
  }

  void _cancelRoomWatch() {
    unawaited(_roomSubscription?.cancel() ?? Future<void>.value());
    _roomSubscription = null;
    _outage?.cancel();
    _outage = null;
  }

  void _onRoomState(ActiveBroadcastSession session, RoomConnectionState state) {
    if (!identical(_session, session) || _closingRoom) return;

    final phase = snapshot.value.phase;
    switch (state) {
      case RoomConnectionState.connected:
        _outage?.cancel();
        _outage = null;
        // The SDK's own reconnect succeeded: the same session is live again,
        // with the same token, so nothing is re-entered.
        if (phase == LivePhase.reconnecting) _publishLive(session);

      case RoomConnectionState.reconnecting:
        if (phase == LivePhase.live) _publishReconnecting(session);

      case RoomConnectionState.disconnected:
        final reason = session.disconnectReason;
        if (_isTerminal(reason)) {
          unawaited(_endTerminally(_endReasonFor(reason)));
          return;
        }
        if (phase != LivePhase.live && phase != LivePhase.reconnecting) return;

        _publishReconnecting(session);
        _startOutageBudget(session);

      case RoomConnectionState.connecting:
        // The entry already published `connecting`; nothing new to say.
        return;
    }
  }

  /// Gives the transport one budget to come back before asking for a new token.
  ///
  /// A media token minted before a long outage is the wrong one to resume
  /// with, so the retry is a fresh entry — the API's `/resume` answers with a
  /// new token — rather than a reconnect with the old one (audit H07).
  void _startOutageBudget(ActiveBroadcastSession session) {
    _outage ??= Timer(reconnectBudget, () {
      _outage = null;
      if (!identical(_session, session) ||
          _closingRoom ||
          session.isClosed ||
          snapshot.value.phase != LivePhase.reconnecting) {
        return;
      }

      unawaited(() async {
        _closingRoom = true;
        _cancelRoomWatch();
        if (identical(_session, session)) _session = null;
        await session.close(LiveCloseReason.replaced);
        _closingRoom = false;

        await enter(
          LiveEnterRequest(
            broadcastId: session.broadcast.id,
            mode: LiveEnterMode.resume,
          ),
        );
      }());
    });
  }

  /// The server said the broadcast is over.
  void _onEndedSignal(EndedSignal signal) {
    final session = _session;
    if (session == null || session.broadcast.id != signal.broadcastId) return;
    if (!snapshot.value.isInRoom) return;
    unawaited(_endTerminally(signal.reason));
  }

  /// Ends the session because the room is gone, keeping its summary.
  ///
  /// No server call: the room ended, or was taken away; the only way to make
  /// that worse is to ask the API to end it again. Everything local is closed,
  /// once, and the published phase never returns to live from here.
  Future<void> _endTerminally(EndReason reason) async {
    final session = _session;
    if (session == null || _closingRoom) return;

    _closingRoom = true;
    _cancelRoomWatch();
    _session = null;

    await session.close(LiveCloseReason.ended);
    _closingRoom = false;

    final summary = session.broadcast;
    await _quietly(_store.clear, 'clearing the recovery record');

    snapshot.value = LiveSnapshot(
      phase: LivePhase.ended,
      endReason: reason == EndReason.none ? null : reason,
      endedSummary: summary,
      role: summary.participantRole,
      canPublish: session.canPublish,
      canEndBroadcast: summary.canEndBroadcast(_context.accountId),
    );
  }

  void _publishReconnecting(ActiveBroadcastSession session) {
    snapshot.value = LiveSnapshot.room(
      phase: LivePhase.reconnecting,
      broadcast: session.broadcast,
      accountId: _context.accountId,
      canPublish: session.canPublish,
    );
  }

  /// Whether a disconnect means the room is gone rather than briefly away.
  bool _isTerminal(RoomDisconnectReason? reason) => switch (reason) {
    RoomDisconnectReason.duplicateIdentity ||
    RoomDisconnectReason.partyRemoved ||
    RoomDisconnectReason.roomDeleted => true,
    _ => false,
  };

  /// The ending a terminal reason corresponds to.
  EndReason _endReasonFor(RoomDisconnectReason? reason) => switch (reason) {
    RoomDisconnectReason.partyRemoved ||
    RoomDisconnectReason.duplicateIdentity ||
    RoomDisconnectReason.roomDeleted => EndReason.adminForced,
    _ => EndReason.none,
  };

  /// Closes a session that never became live.
  Future<void> _closeLocal() async {
    _cancelRoomWatch();
    final session = _session;
    _session = null;
    if (session != null) {
      await session.close(LiveCloseReason.failed);
    }
  }

  /// The server call a session makes on a user exit.
  ///
  /// Creator authority decides which endpoint: only the account that created
  /// the broadcast may end it for everyone, and everyone else leaves (audit §3
  /// H08). The identity is captured here, from this manager's own context.
  Future<LiveServerOutcome> _exitOnServer(BroadcastSession session) async {
    final broadcastId = session.broadcast.id;

    if (session.broadcast.canEndBroadcast(_context.accountId)) {
      final ended = await _gateway.end(broadcastId);
      return LiveServerOutcome(confirmed: true, summary: ended);
    }

    await _gateway.leave(broadcastId);
    return const LiveServerOutcome(confirmed: true);
  }

  void _publishLive(ActiveBroadcastSession session) {
    snapshot.value = LiveSnapshot.room(
      phase: .live,
      broadcast: session.broadcast,
      accountId: _context.accountId,
      canPublish: session.canPublish,
    );
  }

  /// A reason worth carrying, or null for the API's `none`.
  EndReason? _realReason(Broadcast summary) =>
      summary.endReason == EndReason.none ? null : summary.endReason;

  /// Whether another attempt could plausibly work.
  ///
  /// A token or an authorization the server refused will be refused again; a
  /// transport failure may not be. Anything unrecognised is treated as
  /// retryable, because a room the user can try again is better than one the
  /// UI has given up on.
  bool _isRetryable(Object error) => switch (error) {
    MenoException(:final code) =>
      code != MenoErrorCode.unauthorized && code != MenoErrorCode.forbidden,
    _ => true,
  };

  /// Waits for every queued live operation, then runs [job] after them.
  Future<T> _enqueue<T>(Future<T> Function() job) {
    final result = _queue.then((_) => job());
    // The chain must survive a failed job, or one bad entry would stop every
    // later one from running.
    _queue = result.then((_) {}, onError: (Object _) {});
    return result;
  }

  Future<void> _quietly(Future<void> Function() step, String what) async {
    try {
      await step();
    } on Object catch (error, stackTrace) {
      _logger.w(
        'The live manager could not finish $what',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Whether this manager has been disposed.
  ///
  /// Disposal is idempotent: a test that rebuilds over the same gateway and an
  /// exit that already ran both call it, and disposing a Command twice throws.
  bool _disposed = false;

  @override
  FutureOr<dynamic> onDispose() async {
    if (_disposed) return;
    _disposed = true;

    await closeForAccountExit();
    await _endedSubscription?.cancel();
    _endedSubscription = null;
    join.dispose();
    leaveOrEnd.dispose();
    snapshot.dispose();
  }
}
