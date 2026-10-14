import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:fpdart/fpdart.dart';
import 'package:livekit_client/livekit_client.dart' as sdk;
import 'package:meno_mobile/_core/_core.dart';

/// Infrastructure layer - LiveKit Client
///
/// Manages LiveKit room connection and audio/video streaming.
/// This is a low-level infrastructure component that should NOT be
/// exposed to the application layer directly.
class LiveKitClient implements Disposable {
  new _(this._url, this._logger);

  final String _url;
  final MenoLogger _logger;

  sdk.Room? _room;
  sdk.EventsListener<sdk.RoomEvent>? _listener;

  final _connectionStateCtr = StreamController<LiveKitState>.broadcast();

  Stream<LiveKitState> get connectionState => _connectionStateCtr.stream;

  final _roomEventsCtr = StreamController<sdk.RoomEvent>.broadcast();

  Stream<sdk.RoomEvent> get roomEvents => _roomEventsCtr.stream;

  /// Current connection state
  LiveKitState _currentState = .disconnected;

  LiveKitState get currentState => _currentState;

  /// Check if connected
  bool get connected => _room?.connectionState == sdk.ConnectionState.connected;

  /// Get the local participant
  sdk.LocalParticipant? get localParticipant => _room?.localParticipant;

  /// Get current microphone state
  bool get isMicrophoneEnabled =>
      localParticipant?.isMicrophoneEnabled() ?? false;

  // ACTIONS

  static LiveKitClient initialize(String url, MenoLogger logger) {
    final client = LiveKitClient._(url, logger);
    client._logger.i('LiveKit: Initializing client');
    client._room = sdk.Room(
      roomOptions: const sdk.RoomOptions(
        defaultAudioPublishOptions: sdk.AudioPublishOptions(
          name: sdk.AudioPublishOptions.defaultMicrophoneName,
        ),
        adaptiveStream: true,
        dynacast: true,
      ),
    );

    client._listener = client._room?.createListener();
    client._setupRoomListeners();
    client._logger.i('LiveKit: Client initialized successfully');
    return client;
  }

  /// Connect to LiveKit room as a broadcaster (host)
  ///
  /// This enables microphone by default and prepares for broadcasting
  Future<Either<LiveKitFailure, Unit>> broadcast(String token) {
    return _connect(token: token, enableMicrophone: true, isHost: true);
  }

  /// Connect to LiveKit room as a listener (viewer)
  ///
  /// Microphone is disabled by default for listeners
  Future<Either<LiveKitFailure, Unit>> stream(String token) {
    return _connect(token: token, enableMicrophone: false, isHost: false);
  }

  /// Disconnects from the LiveKit room
  ///
  /// The wait matters: the room is only gone once this returns, so a connect
  /// that follows it does not race the teardown of the one before.
  Future<void> disconnect() async {
    _logger.i('LiveKit: Disconnecting from room');
    _updateConnectionState(.disconnected);

    await _room?.disconnect();
    _logger.i('LiveKit: Disconnected successfully');
  }

  /// Enable or disable local microphone
  Future<Either<LiveKitFailure, bool>> setMicrophoneEnabled(
    bool enabled,
  ) async {
    final participant = localParticipant;
    if (participant == null) {
      return const Left(LiveKitNotConnected());
    }

    try {
      await participant.setMicrophoneEnabled(enabled);
      _logger.i('LiveKit: Microphone ${enabled ? 'enabled' : 'disabled'}');
      return Right(enabled);
    } on Exception catch (error) {
      _logger.e('LiveKit: Failed to set microphone state', error: error);
      return Left(LiveKitMicError(error.toString()));
    }
  }

  // HELPER METHODS

  Future<Either<LiveKitFailure, Unit>> _connect({
    required String token,
    required bool enableMicrophone,
    required bool isHost,
  }) async {
    final room = _room;
    if (room == null) return const Left(LiveKitNotConnected());

    try {
      // A room that is still up has to be taken down first: connecting it
      // twice would leave the first engine behind. `disconnect` already waits
      // for the engine, so there is nothing to sleep on here.
      if (room.connectionState == sdk.ConnectionState.connected) {
        _logger.w('LiveKit: Already connected, disconnecting first');
        await disconnect();
      }

      _updateConnectionState(.connecting);
      _logger.i(
        'LiveKit: Connecting as ${isHost ? 'broadcaster' : 'listener'} '
        'to $_url',
      );

      // Warms DNS and TLS, and on LiveKit Cloud also pins the nearest region,
      // so the connect below does not pay for either.
      await room.prepareConnection(_url, token);

      await room.connect(
        _url,
        token,
        fastConnectOptions: isHost
            ? sdk.FastConnectOptions(
                microphone: sdk.TrackOption(enabled: enableMicrophone),
              )
            : null,
      );

      _logger.i('LiveKit: Connected successfully');
      return const Right(unit);
    } on sdk.ConnectException catch (error) {
      _logger.e('LiveKit: Connection refused - ${error.message}');
      await _disconnectQuietly();
      return Left(_failureFrom(error));
    } on sdk.TimeoutException catch (error) {
      _logger.e('LiveKit: Connection timed out - ${error.message}');
      await _disconnectQuietly();
      return const Left(LiveKitConnectionTimeout());
    } on sdk.LiveKitException catch (error) {
      _logger.e('LiveKit: Connection failed - ${error.message}');
      await _disconnectQuietly();
      return Left(LiveKitConnectionFailed(error.message));
    } on Exception catch (error) {
      _logger.e('LiveKit: Unexpected connection error', error: error);
      await _disconnectQuietly();
      return Left(LiveKitUnexpectedError(error.toString()));
    }
  }

  /// What a refused connection means in this client's vocabulary.
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
        'LiveKit: Could not clean up after a failed connection',
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Setup all room event listeners
  void _setupRoomListeners() {
    _logger.i('LiveKit: Setting up room listeners');

    // Forward all events to the room events stream
    _listener?.listen((event) {
      _roomEventsCtr.add(event);
      _handleRoomEvent(event);
    });
  }

  void _updateConnectionState(LiveKitState state) {
    if (_currentState != state) {
      _currentState = state;
      _connectionStateCtr.add(state);
    }
  }

  /// Handle specific room events
  void _handleRoomEvent(sdk.RoomEvent event) {
    switch (event) {
      case sdk.RoomConnectedEvent():
        _updateConnectionState(.connected);
        _logger.i('LiveKit: Room connected');

      case sdk.RoomDisconnectedEvent(:final reason):
        _updateConnectionState(.disconnected);
        _logger.w('LiveKit: Room disconnected - $reason');

      case sdk.RoomReconnectingEvent():
        _updateConnectionState(.reconnecting);
        _logger.v('LiveKit: Reconnecting...');

      case sdk.RoomReconnectedEvent():
        _updateConnectionState(.connected);
        _logger.i('LiveKit: Reconnected successfully');

      case sdk.ParticipantConnectedEvent(:final participant):
        _logger.i('LiveKit: Participant connected - ${participant.identity}');

      case sdk.ParticipantDisconnectedEvent(:final participant):
        _logger.i(
          'LiveKit: Participant disconnected - ${participant.identity}',
        );

      case sdk.TrackPublishedEvent(:final participant):
        _logger.d('LiveKit: Track published by ${participant.identity}');

      case sdk.TrackUnpublishedEvent(:final participant):
        _logger.d('LiveKit: Track unpublished by ${participant.identity}');

      case sdk.TrackSubscribedEvent(:final participant):
        _logger.d('LiveKit: Track subscribed from ${participant.identity} ');

      case sdk.TrackUnsubscribedEvent(:final participant):
        _logger.d('LiveKit: Track unsubscribed from ${participant.identity} ');

      default:
        // Everything else is background detail at this layer; the SDK's own
        // log carries it.
        _logger.v('LiveKit: Room event ${event.runtimeType}');
    }
  }

  @override
  FutureOr<dynamic> onDispose() async {
    _logger.d('LiveKit: Disposing client');

    // The listener is what keeps the room's events flowing into this client,
    // so it goes before the room does.
    await _listener?.dispose();
    _listener = null;

    try {
      await _room?.disconnect();
      await _room?.dispose();
    } on Object catch (error, stackTrace) {
      // Teardown must not throw: the scope that owns this client is being
      // dropped either way, and a room left half-closed is worse than a log.
      _logger.w(
        'LiveKit: Could not close the room cleanly',
        error: error,
        stackTrace: stackTrace,
      );
    } finally {
      _room = null;
    }

    // Close streams
    await _connectionStateCtr.close();
    await _roomEventsCtr.close();

    _logger.d('LiveKit: Client disposed successfully');
  }
}

// LIVEKIT CONNECTION STATE

enum LiveKitState { disconnected, connecting, connected, reconnecting }

extension LivekitConnectionStateX on sdk.ConnectionState {
  LiveKitState get toLiveKitState => switch (this) {
    .disconnected => LiveKitState.disconnected,
    .connecting => LiveKitState.connecting,
    .connected => LiveKitState.connected,
    .reconnecting => LiveKitState.reconnecting,
  };
}
