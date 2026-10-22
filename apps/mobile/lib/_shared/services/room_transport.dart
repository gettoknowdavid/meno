import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';

/// The live room a client is in, as the session manager needs it.
///
/// A session only ever asks four things of a room: join it with a token, hang
/// up, switch the microphone, and hear when the connection changes. This is
/// that surface and nothing else, so the room's client can be replaced — the
/// LiveKit SDK in a normal build, [SilentRoomTransport] when no room server is
/// configured — by swapping a single registration (see `_registerLiveScope`).
///
/// It is deliberately not a mirror of any vendor's SDK: a room's vocabulary is
/// this app's (join, hang up, microphone), and an implementation translates.
///
/// # Failures
///
/// A connect, a hang-up or a microphone switch that does not happen throws one
/// of the room failures in `_core` — the `LiveKitFailure` family. The name
/// predates this interface and is kept on purpose: it is already the sealed
/// branch the error filter routes as a group, and splitting it in two would
/// make a room failure mean two different things depending on who raised it.
abstract interface class RoomTransport implements Disposable {
  /// The room's connection, as it changes under the session.
  ///
  /// Broadcast rather than single-subscription: the session watches it for as
  /// long as it is on air, and a later screen (a diagnostics panel, say) may
  /// watch it too.
  Stream<RoomConnectionState> get connectionState;

  /// Whether the room is up right now.
  bool get isConnected;

  /// Whether this client is publishing its microphone.
  bool get isMicrophoneEnabled;

  /// Joins the room [token] was minted for.
  ///
  /// [canPublish] is the difference between entering to speak and entering to
  /// listen, and it is the caller's to decide: a host publishes, an audience
  /// does not.
  ///
  /// A token the room refuses is thrown, never returned: there is no `Either`
  /// here because every caller is a command whose error filter already reports.
  Future<void> connect(String token, {required bool canPublish});

  /// Leaves the room, waiting for the connection to be gone.
  ///
  /// Waiting matters: the next connect must not race the teardown of the one
  /// before it.
  Future<void> disconnect();

  /// Publishes or stops publishing this client's microphone.
  Future<void> setMicrophoneEnabled(bool enabled);
}

/// Where a room's connection stands.
///
/// The states are the session's, not a vendor's: the live screens' status is
/// derived from them, and a transport that cannot tell a reconnection from a
/// first connection is free to stay disconnected until it is back.
enum RoomConnectionState { connecting, connected, reconnecting, disconnected }

/// A room that opens at once, stays up, and makes no sound.
///
/// This is what the broadcast flow runs on when there is no LiveKit: joining
/// succeeds, the microphone flag moves, hanging up succeeds, and nothing is
/// published or subscribed. It exists so the whole path — going live, the
/// transition page, the live shell, ending — can be walked on a device without
/// a room server, and so the session's own state machine is exercised rather
/// than bypassed.
///
/// It deliberately *is* a [RoomTransport] and not a set of null checks in the
/// session: the flow it drives is the flow LiveKit will drive.
///
/// Nothing here can fail, so no failure is ever thrown — a fake that failed
/// would be testing the error path rather than the happy one.
final class SilentRoomTransport implements RoomTransport {
  new({this.connectDelay = Duration.zero});

  /// How long a join pretends to take.
  ///
  /// Zero by default, so a test does not wait. Raise it on a device to watch
  /// the transition page: the wait is the only thing that keeps it on screen.
  final Duration connectDelay;

  final _states = StreamController<RoomConnectionState>.broadcast();

  RoomConnectionState _state = RoomConnectionState.disconnected;
  bool _isMicrophoneEnabled = false;

  @override
  Stream<RoomConnectionState> get connectionState => _states.stream;

  @override
  bool get isConnected => _state == RoomConnectionState.connected;

  @override
  bool get isMicrophoneEnabled => _isMicrophoneEnabled;

  @override
  Future<void> connect(String token, {required bool canPublish}) async {
    _emit(RoomConnectionState.connecting);

    if (connectDelay > Duration.zero) {
      await Future<void>.delayed(connectDelay);
    }

    // A host enters to speak, an audience to listen — the same rule the real
    // transport follows, so the microphone's own state starts where it will.
    _isMicrophoneEnabled = canPublish;
    _emit(RoomConnectionState.connected);
  }

  @override
  Future<void> disconnect() async {
    if (!isConnected) return;
    _isMicrophoneEnabled = false;
    _emit(RoomConnectionState.disconnected);
  }

  @override
  Future<void> setMicrophoneEnabled(bool enabled) async {
    if (!isConnected) throw const LiveKitNotConnected();
    _isMicrophoneEnabled = enabled;
  }

  @override
  FutureOr<dynamic> onDispose() async {
    await _states.close();
  }

  /// Publishes [next] unless it is where the room already is.
  void _emit(RoomConnectionState next) {
    if (_state == next) return;
    _state = next;
    if (!_states.isClosed) _states.add(next);
  }
}
