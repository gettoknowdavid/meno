import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/room_transport.dart';
import 'package:meno_mobile/features/broadcast/manager/broadcast_timer_manager.dart';
import 'package:meno_mobile/features/broadcast/manager/participants_manager.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';

/// Why a session is closing.
///
/// The reason decides whether the server hears about it at all: only a person
/// leaving or ending a broadcast is a server write. A replaced session, an
/// account exit and a broadcast that ended underneath the client are local
/// teardowns.
enum LiveCloseReason {
  /// The account ended it (creator) or stopped listening (everyone else).
  userExit,

  /// Another session took its place; the new one announces itself.
  replaced,

  /// The account went away.
  accountExit,

  /// The broadcast ended, as the API or the socket said.
  ended,

  /// The session could not start.
  failed,
}

/// What the server said about the exit, kept rather than thrown.
///
/// An end that fails on the wire must still close everything locally (audit
/// H04), so the failure is a value the caller can show — not an exception that
/// skips the `finally`.
@immutable
final class LiveServerOutcome {
  const new({required this.confirmed, this.summary, this.error});

  /// The server could not be reached, so the exit is unconfirmed.
  const new unconfirmed(Object this.error) : confirmed = false, summary = null;

  /// Whether the server acknowledged the exit.
  final bool confirmed;

  /// The ended broadcast, when the caller was its creator and the API answered.
  final EndedBroadcast? summary;

  /// What went wrong, when it did. For logs and messages, never for branching.
  final Object? error;

  @override
  String toString() =>
      'LiveServerOutcome(confirmed: $confirmed, summary: $summary, '
      'error: $error)';
}

/// The server call a session makes when a person leaves or ends it.
///
/// Supplied by the owner, because which endpoint to call is authority — the
/// creator's `/end` versus everyone else's `/leave` — and the session has no
/// business deciding that. Captured identity, one attempt: a retry loop inside
/// a close would keep a room open after the person asked to leave.
typedef LiveServerExit = Future<LiveServerOutcome> Function();

/// What a session needs from the room's roster.
///
/// A session attaches the roster once the transport is up and detaches it
/// exactly once on the way out. Narrow on purpose: the session does not read
/// participants, so it is given only the two calls it makes.
abstract interface class LiveRoster {
  /// Starts following the room's roster.
  Future<void> attach();

  /// Stops following it, and releases whatever it held.
  Future<void> detach();
}

/// The roster a session uses in production: the room's [ParticipantsManager].
final class ParticipantsRoster implements LiveRoster {
  new(this._participants);

  final ParticipantsManager _participants;

  @override
  Future<void> attach() => _participants.initialize.runAsync();

  @override
  Future<void> detach() async {
    await _participants.onDispose();
  }
}

/// One live room, as an ordinary object rather than a DI scope.
///
/// # What it owns
///
/// The transport, the roster, the subscriptions and the on-air clock — created
/// here, closed here, once. The clock used to be a `late final` that only the
/// live page initialised, so a session torn down before that page ran threw a
/// `LateInitializationError` while cleaning up (audit H03). Now it is
/// constructed with the session, so it always exists.
///
/// # Closing
///
/// [close] is idempotent and runs every step in one `finally`: the server call
/// (for [LiveCloseReason.userExit] only) is attempted once and its failure
/// recorded, then the clock, roster, transport and subscriptions are released
/// regardless. Nothing runs a command from here — teardown is not a user action
/// (audit M21).
final class ActiveBroadcastSession {
  new({
    required BroadcastSession session,
    required this._room,
    required this._roster,
    required Clock clock,
    required MenoLogger logger,
    this.participants,
    this._exitOnServer,
    Duration? initialElapsed,
  }) : _session = session,
       _logger = logger {
    // Created with the session, never by a page.
    timer = BroadcastTimerManager(
      clock: clock,
      logger: logger,
      initialElapsed:
          initialElapsed ?? _elapsedSinceStart(session.broadcast, clock),
    );

    // Installed here rather than in start(), so no state the transport reports
    // is missed between construction and connecting.
    _roomStateSubscription = _room.connectionState.listen(_onRoomState);
  }

  final BroadcastSession _session;
  final RoomTransport _room;
  final LiveRoster _roster;
  final MenoLogger _logger;
  final LiveServerExit? _exitOnServer;

  /// The roster the room's screens read, when there is one.
  ///
  /// Owned by the session — created with it and closed with it — but exposed so
  /// the listening tab does not have to resolve a manager that no longer
  /// exists (audit H04).
  final ParticipantsManager? participants;

  /// The on-air clock, owned for as long as this session is.
  late final BroadcastTimerManager timer;

  StreamSubscription<RoomConnectionState>? _roomStateSubscription;

  bool _closed = false;
  bool _started = false;
  Future<LiveServerOutcome>? _closeResult;

  /// The broadcast this session is in.
  Broadcast get broadcast => _session.broadcast;

  /// The account whose credential opened the room.
  Id get userId => _session.userId;

  /// Whether this account may publish audio.
  bool get canPublish => _session.canPublish;

  /// Whether this account created the broadcast — and so may end it.
  bool get canEndBroadcast =>
      _session.broadcast.canEndBroadcast(_session.userId);

  /// The API's own answer for this room, tokens included. Never persisted.
  BroadcastSession get details => _session;

  /// Whether the session has been closed (or is closing).
  bool get isClosed => _closed;

  /// The room's connection, for the owner's snapshot.
  Stream<RoomConnectionState> get connectionState => _room.connectionState;

  /// Whether the room is up right now.
  bool get isConnected => _room.isConnected;

  /// Joins the room and starts following the roster.
  ///
  /// Listeners first, connect second: a transport that reports its state
  /// before anyone listens leaves the session blind until the next change.
  Future<void> start() async {
    if (_started || _closed) return;
    _started = true;

    await _room.connect(_session.token, canPublish: _session.canPublish);
    await _roster.attach();
  }

  /// Mutes or unmutes the local microphone.
  Future<void> setMicrophoneEnabled(bool enabled) =>
      _room.setMicrophoneEnabled(enabled);

  /// Whether this client is publishing its microphone.
  bool get isMicrophoneEnabled => _room.isMicrophoneEnabled;

  /// Closes the session, once.
  ///
  /// A second call awaits the same future rather than closing twice, and a
  /// call during an in-flight close joins it.
  Future<LiveServerOutcome> close(LiveCloseReason reason) {
    return _closeResult ??= _close(reason);
  }

  Future<LiveServerOutcome> _close(LiveCloseReason reason) async {
    var outcome = const LiveServerOutcome(confirmed: true);

    try {
      // Only a session that opened a room has anything to tell the API. One
      // closed before start() never joined, so it never leaves.
      if (reason == LiveCloseReason.userExit && _started) {
        final exit = _exitOnServer;
        if (exit != null) outcome = await exit();
      }
    } on Object catch (error, stackTrace) {
      // Recorded, not thrown: everything below still has to close (H04).
      _logger.w(
        'The server did not confirm leaving the room',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
      outcome = LiveServerOutcome.unconfirmed(error);
    } finally {
      timer.stop();
      await timer.onDispose();

      await _roomStateSubscription?.cancel();
      _roomStateSubscription = null;

      await _quietly(_roster.detach, 'detaching the roster');
      await _quietly(() async {
        await _room.onDispose();
      }, 'disposing the room');

      _closed = true;
    }

    return outcome;
  }

  /// Follows the room for as long as the session is on air.
  ///
  /// The clock pauses with the transport: a dropped connection is time the
  /// broadcast was not on air, which is what [BroadcastTimerManager] subtracts.
  void _onRoomState(RoomConnectionState state) {
    if (_closed) return;
    switch (state) {
      case RoomConnectionState.connected:
        timer.resume();
      case RoomConnectionState.reconnecting:
      case RoomConnectionState.disconnected:
      case RoomConnectionState.connecting:
        timer.pause();
    }
  }

  /// Runs one teardown step, logging a failure instead of letting it skip the
  /// rest of the close.
  Future<void> _quietly(Future<void> Function() step, String what) async {
    try {
      await step();
    } on Object catch (error, stackTrace) {
      _logger.w(
        'The live session could not finish $what',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// How long the broadcast had been on air when this session opened.
  static Duration _elapsedSinceStart(Broadcast broadcast, Clock clock) {
    final startedAt =
        broadcast.startTime ?? broadcast.publishedAt ?? broadcast.createdAt;
    if (startedAt == null) return Duration.zero;
    final since = clock.nowUtc().difference(startedAt);
    return since < Duration.zero ? Duration.zero : since;
  }
}
