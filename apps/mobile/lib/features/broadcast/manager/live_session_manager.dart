import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/room_transport.dart';
import 'package:meno_mobile/features/broadcast/manager/broadcast_timer_manager.dart';
import 'package:meno_mobile/features/broadcast/manager/live_scope_manager.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';
import 'package:meno_mobile/features/profile/profile.dart';

/// The room one client is in, from the inside.
///
/// # The three things that have to happen, in order
///
/// Getting into a broadcast is connect → announce → settle: connect to LiveKit
/// with the token the API minted, tell the room over the socket, and only then
/// paint "live". Each step is its own command chained to the next, so a failure
/// at any of them leaves the client in a state it can retry from rather than in
/// one that looks live but has no audio.
///
/// # Who ends the broadcast
///
/// The host does, through the API — `POST /broadcasts/{id}/end` — and not by
/// shouting on the socket: ending is a write to the row that decides a
/// broadcast is over, and a socket event is a hint that it happened. A listener
/// that receives `onEndedBroadcast` is being told to stop drawing, never told
/// to end anything.
final class LiveSessionManager implements Disposable, WillSignalReady {
  new({
    required this._session,
    required this._http,
    required this._local,
    required this._socket,
    required this._room,
    required this._live,
    required this._logger,
  }) {
    setupConfigs = .createSyncNoParamNoResult(
      () {
        // Where the clock counts from: when the API says this broadcast went
        // on air. `startTime` is the *scheduled* time and exists only for a
        // scheduled broadcast; `publishedAt` is written by go-live and is what
        // every unscheduled broadcast has — including one a restarted process
        // just came back to. The session's own timestamp is stamped on the way
        // in, so falling back to it would count from the moment this process
        // returned (or joined) rather than from when the broadcast started,
        // which is what reset the timer to zero after a recovery and left a
        // late listener at zero too.
        final startedAt =
            broadcast.value.startTime ?? broadcast.value.publishedAt;
        timer = BroadcastTimerManager(
          broadcastStartTime: startedAt ?? _session.timestamp ?? DateTime.now(),
          initialElapsed: startedAt == null
              ? null
              : DateTime.now().difference(startedAt),
          logger: _logger,
        );
        // Sets `elapsed` now rather than on the first tick a second later, so
        // the frame that opens on this room already shows the right time.
        timer.initialize.run();

        _setupListeners();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.session.setup',
    );

    _connectToLiveKit = .createAsyncNoParamNoResult(
      () async {
        final token = _session.token;
        if (token.isEmpty) {
          throw const MalformedResponseException(
            message: 'This session arrived without a room token.',
          );
        }

        status.value = LiveStatus.connecting;

        // A failure here is a connection that did not happen; raising it lets
        // the error listener below decide between retrying and giving up.
        await _room.connect(token, canPublish: isHost.value);
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.session.connect',
    );
    _announcePresence = .createAsyncNoParamNoResult(
      () async {
        // The draft the host created this broadcast from has become a real
        // broadcast; keeping it would offer to resume a creation that is done.
        if (isHost.value) {
          await _local.deleteDraft(
            draftId: broadcast.value.id,
            ownerId: _session.userId,
          );
        }
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.session.announce',
    );
    _settle = .createSyncNoParamNoResult(
      () {
        if (isHost.value) {
          isMicrophoneEnabled.value = _room.isMicrophoneEnabled;
        }
        state.value = isHost.value
            ? const LiveSessionState.broadcasting()
            : const LiveSessionState.streaming();
        status.value = LiveStatus.live;
        _reconnectionAttempts = 0;
        timer.start.run();

        // The live scope's registrations are done; anything waiting on
        // `allReady`— the transition page, the router — may proceed.
        di.signalReady(this);
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.session.settle',
    );

    endSession = .createAsyncNoParamNoResult(
      () async {
        if (_isLeaving.value) return;
        _isLeaving.value = true;
        status.value = LiveStatus.disconnecting;
        timer.stop.run();

        try {
          if (isHost.value) {
            endedBroadcast.value = await _http.end(broadcast.value.id);
          } else {
            await _http.leave(broadcast.value.id);
          }
        } finally {
          await _room.disconnect();
          state.value = const LiveSessionState.ended();
          status.value = LiveStatus.offAir;
          _isLeaving.value = false;
        }

        // The room is gone, so the scope that owns it goes too. Queued rather
        // than awaited: this command is registered *inside* that scope, and
        // waiting for its own teardown would dispose this manager while it is
        // still running — which is why the pop happens in the live scope's
        // queue and not in this one.
        unawaited(_live.end());
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.session.end',
    );
    toggleMicrophone = .createUndoableNoResult<bool, bool>(
      (value, stack) async {
        stack.push(isMicrophoneEnabled.value);
        isMicrophoneEnabled.value = value;
        await _room.setMicrophoneEnabled(value);
      },
      undo: (stack, _) => isMicrophoneEnabled.value = stack.pop(),
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.session.toggleMicrophone',
    );
    // Chained here, after every command exists. A `..pipeToCommand(…)` written
    // on the line that builds a command reads the field it points at, and a
    // `late final` assigned two lines below has not been initialised yet — the
    // constructor would throw before the first room could open. The closures
    // above only reach for their neighbours when they *run*, so the order they
    // are created in does not matter; the order they are read does.
    setupConfigs.pipeToCommand(_connectToLiveKit);
    _connectToLiveKit.pipeToCommand(_announcePresence);
    _announcePresence.pipeToCommand(_settle);

    // The room's own connection, for as long as the session lives. Exactly one
    // listener, so there is exactly one to cancel; `_handleRoomState` ignores
    // it until the chain above has settled.
    _roomStateSubscription = _room.connectionState.listen(
      _handleRoomState,
      onError: (Object error) =>
          _logger.e('Room state stream failed', error: error),
    );
  }

  final BroadcastSession _session;
  final BroadcastHttpService _http;
  final BroadcastLocalService _local;
  final BroadcastSocketService _socket;
  final RoomTransport _room;
  final LiveScopeManager _live;
  final MenoLogger _logger;

  static const int _maxReconnectionAttempts = 5;

  /// The broadcast being watched, as it changes under us.
  late final broadcast = ValueNotifier<Broadcast>(_session.broadcast);

  /// How far the session has got.
  final state = ValueNotifier<LiveSessionState>(
    const LiveSessionState.initializing(),
  );

  /// The connection status a header draws.
  final status = ValueNotifier<LiveStatus>(LiveStatus.initializing);

  final isMicrophoneEnabled = ValueNotifier<bool>(false);

  /// Whether this client may publish.
  late final isHost = ValueNotifier<bool>(
    _session.canPublish || _session.broadcast.isHostedBy(_session.userId),
  );

  late final hostProxy = UserProfileProxy(
    Profile(
      id: _session.broadcast.hostId,
      fullName: _session.broadcast.hostName,
      avatarUrl: _session.broadcast.hostImageUrl,
    ),
  );

  /// What the room ended as, once the API has said.
  final endedBroadcast = ValueNotifier<EndedBroadcast?>(null);

  /// The on-air clock, built by [setupConfigs].
  late final BroadcastTimerManager timer;

  StreamSubscription<RoomConnectionState>? _roomStateSubscription;
  StreamSubscription<HostConnectionEvent>? _hostConnectionSubscription;
  StreamSubscription<EndedSignal>? _endedBroadcastSubscription;

  Timer? _reconnectionTimer;
  int _reconnectionAttempts = 0;
  final _isLeaving = ValueNotifier<bool>(false);

  /// Whether the room is being left or was left.
  ValueListenable<bool> get isLeaving => _isLeaving;

  /// Builds the timer and subscribes to the room, then connects.
  ///
  /// The timer is built from the broadcast's own start time when the API sent
  /// one — a listener who joins ten minutes late must see ten minutes on the
  /// clock, not zero — and from the session's timestamp otherwise.
  late final Command<void, void> setupConfigs;

  late final Command<void, void> _connectToLiveKit;

  late final Command<void, void> _announcePresence;

  late final Command<void, void> _settle;

  /// Ends the broadcast (host) or stops listening (listener).
  ///
  /// Both roles leave the room and hang up locally; only the host's call ends
  /// the broadcast for everybody, which is why it goes through the API. Either
  /// way the room is over, so the scope it was opened in is closed with it.
  late final Command<void, void> endSession;

  /// Mutes or unmutes the local microphone, undoably.
  ///
  /// The flag moves first and the room follows: a mute button that waits for
  /// the round trip feels broken, and the undo stack is what puts the flag back
  /// when the room refuses.
  late final Command<bool, void> toggleMicrophone;

  /// The last failure that stopped a step, for a room that never settled.
  late final ValueListenable<CommandError<void>?> errors =
      _connectToLiveKit.errors;

  void _setupListeners() {
    _hostConnectionSubscription = _socket.onHostConnectionChanged.listen(
      _handleHostConnection,
      onError: (Object error) =>
          _logger.e('Host connection stream failed', error: error),
    );

    _endedBroadcastSubscription = _socket.onEndedBroadcast.listen(
      _handleEndedBroadcast,
      onError: (Object error) =>
          _logger.e('Ended broadcast stream failed', error: error),
    );

    _connectToLiveKit.errors.listen((commandError, _) {
      final error = commandError?.error;
      if (error == null) return;

      _logger.e('Could not get into the room', error: error);
      state.value = LiveSessionState.error(errorMessage(error));
      status.value = LiveStatus.offAir;

      // Only the failures that a retry can plausibly fix are retried. A bad
      // token will be bad on the next attempt too.
      if (error is LiveKitConnectionTimeout ||
          error is LiveKitConnectionFailed) {
        _scheduleReconnection();
      }
    });

    endSession.errors.listen((commandError, _) {
      _logger.w(
        'Leaving the room did not complete cleanly',
        error: commandError?.error,
      );
    });
  }

  /// Follows the room once this client is on air.
  ///
  /// The chain owns the status until then: a room that reports `connected`
  /// while the client is still announcing is not a reason to paint "live", so
  /// everything is ignored while the state is still
  /// [LiveSessionInitializing]. Afterwards the transport is the truth — a drop,
  /// a reconnect and a return are its to report, and the clock follows.
  void _handleRoomState(RoomConnectionState next) {
    if (state.value is LiveSessionInitializing) return;

    switch (next) {
      case RoomConnectionState.connected:
        status.value = LiveStatus.live;
        _reconnectionAttempts = 0;
        _reconnectionTimer?.cancel();
        if (!timer.isRunning.value) timer.resume.run();

      case RoomConnectionState.connecting:
        status.value = LiveStatus.connecting;

      case RoomConnectionState.reconnecting:
        status.value = LiveStatus.reconnecting;
        if (timer.isRunning.value) timer.pause.run();

      case RoomConnectionState.disconnected:
        status.value = LiveStatus.disconnected;
        if (timer.isRunning.value) timer.pause.run();

        // An ending session disconnects on purpose; retrying it would put the
        // client back in a room it just left.
        if (!_isLeaving.value && state.value is! LiveSessionError) {
          _scheduleReconnection();
        }
    }
  }

  void _handleHostConnection(HostConnectionEvent event) {
    if (event.broadcastId != broadcast.value.id) return;

    if (event.connected) {
      // Only a listener notices: the host's own connection is its own.
      if (isHost.value) return;
      _logger.i('The host came back');
      status.value = LiveStatus.live;
      state.value = const LiveSessionState.streaming();
      if (!timer.isRunning.value) timer.resume.run();
      return;
    }

    if (isHost.value) return;
    _logger.w('The host dropped out of the room');
    status.value = LiveStatus.hostDisconnected;
    state.value = const LiveSessionState.hostDisconnected();
    if (timer.isRunning.value) timer.pause.run();
  }

  /// The broadcast ended: stop drawing the room, then ask the API what ended.
  ///
  /// The socket's frame is a signal — `{broadcastId, reason}` — not a summary.
  /// Turning it into an [EndedBroadcast] and showing it produced a card with an
  /// empty title and creator (audit §3 M10); the summary is fetched from REST
  /// instead, and the page shows nothing to summarise until that answer lands.
  Future<void> _handleEndedBroadcast(EndedSignal ended) async {
    if (ended.broadcastId != broadcast.value.id) return;

    _logger.i('The broadcast ended');
    timer.stop.run();
    state.value = const LiveSessionState.ended();
    status.value = LiveStatus.offAir;

    await _room.disconnect();
    await _loadEndedSummary(ended);
  }

  /// Reads the real summary of the broadcast that just ended.
  ///
  /// [EndedSignal] carries an id and a reason, so every field the ended screen
  /// draws — title, host, duration, participants — has to come from the API.
  /// The reason is kept from the signal when it is a real one, because the
  /// summary's own `endReason` can still read `none` while the write settles.
  Future<void> _loadEndedSummary(EndedSignal ended) async {
    try {
      final summary = await _http.getBroadcast(ended.broadcastId);
      endedBroadcast.value = EndedBroadcast(
        broadcastId: summary.id,
        title: summary.title,
        creatorId: summary.hostId,
        creatorName: summary.creator?.fullName ?? SingleLineString.empty,
        endedAt: summary.endTime,
        image: summary.image,
        reason: ended.reason == EndReason.none
            ? summary.endReason
            : ended.reason,
        duration: summary.duration ?? Duration.zero,
        totalParticipants: summary.totalParticipants,
        recordingEnabled: summary.recordingEnabled,
      );
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Could not read the ended broadcast summary',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Retries the connection with a growing delay, up to a ceiling.
  void _scheduleReconnection() {
    if (_reconnectionAttempts >= _maxReconnectionAttempts) {
      state.value = const LiveSessionState.error(
        'Connection lost. Please start again.',
      );
      status.value = LiveStatus.offAir;
      timer.stop.run();
      return;
    }

    _reconnectionAttempts++;
    final delay = Duration(seconds: _reconnectionAttempts * 2);
    _logger.w('Retrying the room connection in ${delay.inSeconds}s');

    _reconnectionTimer?.cancel();
    _reconnectionTimer = Timer(delay, _connectToLiveKit.run);
  }

  @override
  FutureOr<dynamic> onDispose() async {
    _logger.d('Disposing the live session');

    _reconnectionTimer?.cancel();
    _reconnectionTimer = null;

    await _roomStateSubscription?.cancel();
    await _hostConnectionSubscription?.cancel();
    await _endedBroadcastSubscription?.cancel();

    _roomStateSubscription = null;
    _hostConnectionSubscription = null;
    _endedBroadcastSubscription = null;

    // Leaving is part of tearing down: a scope that is popped without it leaves
    // the socket subscribed to a room nobody is in. A session that already
    // ended — this client's own end or leave, or the broadcast ending under
    // it — has left already, and leaving twice would be a write the API does
    // not need.
    final hasLeftAlready = state.value is LiveSessionEnded;
    if (!_isLeaving.value && !hasLeftAlready) await endSession.runAsync();

    await timer.onDispose();

    broadcast.dispose();
    state.dispose();
    status.dispose();
    isMicrophoneEnabled.dispose();
    isHost.dispose();
    hostProxy.dispose();
    endedBroadcast.dispose();
    _isLeaving.dispose();

    setupConfigs.dispose();
    _connectToLiveKit.dispose();
    _announcePresence.dispose();
    _settle.dispose();
    endSession.dispose();
    toggleMicrophone.dispose();
  }
}
