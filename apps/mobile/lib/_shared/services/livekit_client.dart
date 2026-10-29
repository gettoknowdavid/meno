import 'dart:async';

import 'package:livekit_client/livekit_client.dart' as sdk;
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/dev_room_token.dart';
import 'package:meno_mobile/_shared/services/room_transport.dart';

/// The room, as the LiveKit SDK sees it.
///
/// [RoomTransport] is this app's vocabulary — join, hang up, microphone — and
/// this is the one implementation that translates it into the SDK's. Everything
/// vendor-specific about the room stays in this file: the session manager, the
/// live shell and the transition page never learn that a `Room`, an `Engine` or
/// a token grant exists.
///
/// # The token
///
/// [connect] is handed the token the API minted for the session and uses it as
/// it stands. The single exception is [devToken]: a development build has no
/// API-minted token (the broadcast flow around it is still fed from fixtures),
/// so it signs one for the docker server instead — see [DevRoomTokenSource] and
/// [Config.liveKitUrl]. A release build is never given one.
///
/// # Events are the truth
///
/// The connection is followed from the room's own events rather than inferred
/// from the calls that succeeded: a drop, a reconnection and a return to the
/// room all happen without anybody calling this class, and the session's status
/// has to follow them. [RoomConnectionState.connecting]/[RoomConnectionState.connected]
/// are emitted from the calls as well, so the state moves even on a platform
/// whose events are late.
final class LiveKitRoomTransport implements RoomTransport {
  /// Opens the transport onto the room server at [url].
  ///
  /// The room is built here rather than at the first join: its listener has to
  /// exist before [connect] runs, or the connection events that arrive while it
  /// is still connecting are lost.
  new({required this.url, required this._logger, this.devToken})
    : _room = sdk.Room(
        roomOptions: const sdk.RoomOptions(
          adaptiveStream: true,
          dynacast: true,
          defaultAudioPublishOptions: sdk.AudioPublishOptions(
            name: sdk.AudioPublishOptions.defaultMicrophoneName,
          ),
        ),
      ) {
    // The listener has to exist before `connect`: the events that arrive while
    // the room is still joining are what the session's status follows.
    _listener = _room.createListener()..listen(_handleEvent);
  }

  /// The room server, as the SDK takes it (`ws://` or `wss://`).
  final String url;

  /// Signs a room token for the local development server, or null in any build
  /// that gets its token from the API.
  final DevRoomTokenSource? devToken;

  final MenoLogger _logger;
  final sdk.Room _room;

  sdk.EventsListener<sdk.RoomEvent>? _listener;

  final _states = StreamController<RoomConnectionState>.broadcast();

  RoomConnectionState _state = RoomConnectionState.disconnected;
  RoomDisconnectReason? _lastReason;

  bool _disposed = false;

  @override
  RoomDisconnectReason? get lastDisconnectReason => _lastReason;

  @override
  Stream<RoomConnectionState> get connectionState => _states.stream;

  @override
  bool get isConnected => _state == RoomConnectionState.connected;

  @override
  bool get isMicrophoneEnabled =>
      _room.localParticipant?.isMicrophoneEnabled() ?? false;

  @override
  Future<void> connect(String token, {required bool canPublish}) async {
    if (_disposed) throw const LiveKitNotConnected();

    // A room that is still up has to be taken down first: connecting it twice
    // would leave the first engine behind. `disconnect` already waits for the
    // engine, so there is nothing to sleep on here.
    if (_room.connectionState != sdk.ConnectionState.disconnected) {
      _logger.w('The room was still up; taking it down before joining again');
      await disconnect();
    }

    final accessToken = _accessToken(token, canPublish: canPublish);

    _emit(RoomConnectionState.connecting);
    _logger.i(
      'Joining ${devToken == null ? 'the room' : 'the local room'} at $url '
      'as ${canPublish ? 'a publisher' : 'a listener'}',
    );

    try {
      // Warms DNS and TLS, and on LiveKit Cloud also pins the nearest region,
      // so the connect below does not pay for either.
      await _room.prepareConnection(url, accessToken);

      await _room.connect(url, accessToken);

      // A host enters to speak: the microphone is published before the join is
      // reported as done, so a screen that reads `isMicrophoneEnabled` the
      // moment the session settles does not find it still false.
      if (canPublish) await _enableMicrophoneAfterJoining();

      _logger.i('Joined the room');
      _emit(RoomConnectionState.connected);
    } on sdk.ConnectException catch (error) {
      _logger.e('The room refused the connection - ${error.message}');
      await _disconnectQuietly();
      throw _failureFrom(error);
    } on sdk.TimeoutException catch (error) {
      _logger.e('The room did not answer in time - ${error.message}');
      await _disconnectQuietly();
      throw const LiveKitConnectionTimeout();
    } on sdk.LiveKitException catch (error) {
      _logger.e('Could not join the room - ${error.message}');
      await _disconnectQuietly();
      throw LiveKitConnectionFailed(error.message);
    } on Exception catch (error) {
      _logger.e('Could not join the room', error: error);
      await _disconnectQuietly();
      throw LiveKitUnexpectedError(error.toString());
    }
  }

  @override
  Future<void> disconnect() async {
    if (_disposed) return;

    if (_room.connectionState == sdk.ConnectionState.disconnected) {
      _emit(RoomConnectionState.disconnected);
      return;
    }

    _lastReason = RoomDisconnectReason.clientInitiated;
    _logger.i('Leaving the room');
    try {
      await _room.disconnect();
    } on Object catch (error, stackTrace) {
      // The room is gone as far as the session is concerned either way; what
      // is left is an engine that did not acknowledge it, which the dispose
      // below will finish.
      _logger.w(
        'Leaving the room did not complete cleanly',
        error: error,
        stackTrace: stackTrace,
      );
    } finally {
      _emit(RoomConnectionState.disconnected);
    }
  }

  @override
  Future<void> setMicrophoneEnabled(bool enabled) async {
    final participant = _room.localParticipant;
    if (participant == null || !isConnected) throw const LiveKitNotConnected();

    try {
      await participant.setMicrophoneEnabled(enabled);
      _logger.i('Microphone ${enabled ? 'unmuted' : 'muted'}');
    } on Object catch (error) {
      _logger.e('Could not switch the microphone', error: error);
      throw LiveKitMicError(error.toString());
    }
  }

  @override
  FutureOr<dynamic> onDispose() async {
    if (_disposed) return;
    _disposed = true;

    _logger.d('Closing the room');

    // The listener is what keeps the room's events flowing into this transport,
    // so it goes before the room does.
    await _listener?.dispose();
    _listener = null;

    try {
      await _room.disconnect();
      await _room.dispose();
    } on Object catch (error, stackTrace) {
      // Teardown must not throw: the scope that owns this transport is being
      // dropped either way, and a room left half-closed is worse than a log.
      _logger.w(
        'Could not close the room cleanly',
        error: error,
        stackTrace: stackTrace,
      );
    }

    await _states.close();
  }

  /// The token [connect] actually joins with.
  ///
  /// The session's own token everywhere but a development build, where there is
  /// no API-minted one to use and the local server only accepts what it can
  /// verify.
  String _accessToken(String token, {required bool canPublish}) {
    final signer = devToken;
    if (signer == null) return token;

    _logger.d(
      'Signing a development room token for ${signer.identity} '
      'in ${signer.room}',
    );
    return signer.token(canPublish: canPublish);
  }

  /// Publishes the microphone once the join has gone through.
  ///
  /// A device that refuses the microphone fails the join rather than entering
  /// the room silently: a host with no microphone is a broadcast nobody can
  /// hear, and the screen that started it should say so.
  Future<void> _enableMicrophoneAfterJoining() async {
    final participant = _room.localParticipant;
    if (participant == null) {
      throw const LiveKitMicError('No local participant');
    }

    await participant.setMicrophoneEnabled(true);
  }

  /// What a refused connection means in this app's vocabulary.
  ///
  /// The SDK reports a rejected token as a [sdk.ConnectException] carrying the
  /// status the server answered with, which is what is matched on here rather
  /// than the message text — the SDK is free to reword that at any release.
  LiveKitFailure _failureFrom(sdk.ConnectException error) {
    if (error.statusCode == 401) return const LiveKitInvalidToken();
    if (error.reason == sdk.ConnectionErrorReason.Timeout) {
      return const LiveKitConnectionTimeout();
    }
    return LiveKitConnectionFailed(error.message);
  }

  /// Tears the room down after a failed connect.
  ///
  /// Cleaning up can fail on its own — a peer that never answered, an engine
  /// that already gave up — and that failure must not replace the one that
  /// caused the teardown.
  Future<void> _disconnectQuietly() async {
    try {
      await disconnect();
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Could not clean up after a failed connection',
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Follows the room's own events.
  void _handleEvent(sdk.RoomEvent event) {
    switch (event) {
      case sdk.RoomConnectedEvent():
      case sdk.RoomReconnectedEvent():
        _emit(RoomConnectionState.connected);

      case sdk.RoomReconnectingEvent():
        _emit(RoomConnectionState.reconnecting);

      case sdk.RoomDisconnectedEvent(:final reason):
        _logger.w('Left the room - $reason');
        _lastReason = _disconnectReason(reason);
        _emit(RoomConnectionState.disconnected);

      case sdk.ParticipantConnectedEvent(:final participant):
        _logger.i('${participant.identity} joined the room');

      case sdk.ParticipantDisconnectedEvent(:final participant):
        _logger.i('${participant.identity} left the room');

      default:
        // Everything else is background detail at this layer; the SDK's own
        // log carries it.
        _logger.v('Room event ${event.runtimeType}');
    }
  }

  /// The SDK's reason, in this app's words.
  ///
  /// Kept explicit rather than a `name` lookup: a kick and a room deletion are
  /// what the manager must treat as terminal, and a silent SDK rename would
  /// turn both back into an unknown drop it retries (audit H07).
  RoomDisconnectReason _disconnectReason(sdk.DisconnectReason? reason) {
    return switch (reason) {
      sdk.DisconnectReason.clientInitiated =>
        RoomDisconnectReason.clientInitiated,
      sdk.DisconnectReason.duplicateIdentity =>
        RoomDisconnectReason.duplicateIdentity,
      sdk.DisconnectReason.participantRemoved =>
        RoomDisconnectReason.partyRemoved,
      sdk.DisconnectReason.roomDeleted => RoomDisconnectReason.roomDeleted,
      sdk.DisconnectReason.serverShutdown =>
        RoomDisconnectReason.serverShutdown,
      sdk.DisconnectReason.stateMismatch => RoomDisconnectReason.stateMismatch,
      sdk.DisconnectReason.signalClose => RoomDisconnectReason.signalClose,
      _ => RoomDisconnectReason.unknown,
    };
  }

  /// Publishes [next] unless it is where the room already is.
  void _emit(RoomConnectionState next) {
    if (_state == next) return;
    _state = next;
    if (!_states.isClosed) _states.add(next);
  }
}
