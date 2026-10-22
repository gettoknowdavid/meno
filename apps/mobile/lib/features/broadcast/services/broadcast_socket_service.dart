import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/socket.dart';
import 'package:meno_mobile/_shared/services/socket_client.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';

/// The broadcast events the socket carries.
///
/// Each is exposed as a stream and decoded once here, so a manager that listens
/// for "a broadcast ended" gets an [EndedBroadcast] rather than a map it has to
/// remember the keys of. The client reattaches handlers after a reconnect, so a
/// subscription stays live across one — [onReconnected] is where a client is
/// told that it happened and that the server has forgotten its rooms.
final class BroadcastSocketService implements Disposable {
  new({required this._client, required this._logger});

  final SocketClient _client;
  final MenoLogger _logger;

  final _newBroadcast = StreamController<Broadcast>.broadcast();
  final _started = StreamController<Broadcast>.broadcast();
  final _ended = StreamController<EndedBroadcast>.broadcast();
  final _joined = StreamController<BroadcastParticipantEvent>.broadcast();
  final _left = StreamController<BroadcastParticipantEvent>.broadcast();
  final _hostState = StreamController<HostConnectionEvent>.broadcast();

  final List<SocketSubscription> _subscriptions = [];

  bool _listening = false;

  /// A broadcast was created or went on air.
  Stream<Broadcast> get onNewBroadcast => _newBroadcast.stream;

  /// A broadcast went live.
  Stream<Broadcast> get onStartedBroadcast => _started.stream;

  /// A broadcast ended, in any of the ways it can.
  Stream<EndedBroadcast> get onEndedBroadcast => _ended.stream;

  /// Somebody joined a room.
  Stream<BroadcastParticipantEvent> get onParticipantJoined => _joined.stream;

  /// Somebody left a room.
  Stream<BroadcastParticipantEvent> get onParticipantLeft => _left.stream;

  /// The host of a room you are in dropped or came back.
  Stream<HostConnectionEvent> get onHostConnectionChanged => _hostState.stream;

  /// The underlying socket dropped and came back.
  ///
  /// The client is looking at a fresh socket with no rooms on it, so a client
  /// that is still in a broadcast has to say so again — see
  /// `LiveSessionManager.reconnectToSocket`.
  Stream<void> get onReconnected => _client.onReconnected;

  /// Starts listening for the broadcast events.
  ///
  /// Called once when the account's scope is opened. A second call is a no-op:
  /// every call appends handlers to the client, which keeps them for the life
  /// of the account, so listening twice would deliver each event twice.
  void listen() {
    if (_listening) return;
    _listening = true;

    _subscriptions
      ..add(
        _client.on(
          SocketEvent.newBroadcast,
          (data) => _emit(_newBroadcast, _broadcast(data)),
        ),
      )
      ..add(
        _client.on(
          SocketEvent.startedBroadcast,
          (data) => _emit(_started, _broadcast(data)),
        ),
      )
      ..add(
        _client.on(
          SocketEvent.endedBroadcast,
          (data) => _emit(_ended, _endedBroadcast(data)),
        ),
      )
      ..add(
        _client.on(
          SocketEvent.newBroadcastListener,
          (data) => _emit(_joined, _participantEvent(data)),
        ),
      )
      ..add(
        _client.on(
          SocketEvent.broadcastListenerLeft,
          (data) => _emit(_left, _participantEvent(data)),
        ),
      )
      ..add(
        _client.on(
          SocketEvent.hostDisconnected,
          (data) => _emit(_hostState, _hostStateEvent(data, connected: false)),
        ),
      )
      ..add(
        _client.on(
          SocketEvent.hostReconnected,
          (data) => _emit(_hostState, _hostStateEvent(data, connected: true)),
        ),
      );
  }

  /// Tells the server this client is watching [broadcastId].
  ///
  /// Best effort. Room membership is a hint the server uses for presence, not
  /// something audio depends on — a socket that is still connecting must not
  /// stop a client from entering the room, and `LiveSessionManager` says it
  /// again when the socket comes back.
  void joinRoom(Id broadcastId) => _emitRoomEvent(
    SocketEvent.joinBroadcast,
    broadcastId,
  );

  /// Tells the server this client stopped watching [broadcastId].
  void leaveRoom(Id broadcastId) => _emitRoomEvent(
    SocketEvent.leaveBroadcast,
    broadcastId,
  );

  void _emitRoomEvent(SocketEvent event, Id broadcastId) {
    try {
      _client.emit(event, {'broadcastId': broadcastId.getOrCrash});
    } on SocketNotConnectedException catch (error) {
      _logger.d(
        'Could not send ${event.value}: ${error.message}',
        tag: MenoLogTag.network.label,
      );
    }
  }

  @override
  FutureOr<dynamic> onDispose() async {
    for (final subscription in _subscriptions) {
      subscription.cancel();
    }
    _subscriptions.clear();
    _listening = false;
    await Future.wait([
      _newBroadcast.close(),
      _started.close(),
      _ended.close(),
      _joined.close(),
      _left.close(),
      _hostState.close(),
    ]);
  }

  /// Decodes a broadcast payload, or reports why it could not be.
  ///
  /// A malformed event is dropped rather than thrown: it arrives on a stream
  /// nothing awaits, and one bad frame must not take the room down.
  Broadcast? _broadcast(dynamic data) {
    return _decode(data, (json) => BroadcastDto.fromJson(json).toDomain);
  }

  EndedBroadcast? _endedBroadcast(dynamic data) {
    return _decode(data, (json) => EndBroadcastDto.fromJson(json).toDomain);
  }

  BroadcastParticipantEvent? _participantEvent(dynamic data) {
    return _decode(data, (json) {
      final participant = json['participant'];
      return BroadcastParticipantEvent(
        broadcastId: Id.fromWire(json['broadcastId'] as String? ?? ''),
        count: (json['count'] as num?)?.toInt() ?? 0,
        participant: participant == null
            ? null
            : ParticipantDto.fromJson(_asMap(participant)!).toDomain,
      );
    });
  }

  HostConnectionEvent? _hostStateEvent(
    dynamic data, {
    required bool connected,
  }) {
    return _decode(
      data,
      (json) => HostConnectionEvent(
        broadcastId: Id.fromWire(json['broadcastId'] as String? ?? ''),
        connected: connected,
      ),
    );
  }

  T? _decode<T>(dynamic data, T Function(Map<String, dynamic>) decode) {
    final json = _asMap(data);
    if (json == null) return null;
    try {
      return decode(json);
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Dropped a broadcast socket event that could not be decoded',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
      return null;
    }
  }

  void _emit<T>(StreamController<T> controller, T? event) {
    if (event != null) controller.add(event);
  }

  static Map<String, dynamic>? _asMap(dynamic data) =>
      data is Map<String, dynamic> ? data : null;
}

/// Somebody joined or left a broadcast.
final class BroadcastParticipantEvent {
  const new({required this.broadcastId, required this.count, this.participant});

  final Id broadcastId;

  /// How many accounts are in the room after the change.
  final int count;
  final Participant? participant;
}

/// The host's socket is in its grace window, or came back.
final class HostConnectionEvent {
  const new({required this.broadcastId, required this.connected});

  final Id broadcastId;
  final bool connected;
}
