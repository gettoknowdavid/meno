import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/socket.dart';
import 'package:meno_mobile/_shared/services/socket_client.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/model/dtos.dart';

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
  final _ended = StreamController<EndedBroadcast>.broadcast();
  final _homeInvalidated = StreamController<HomeInvalidatedDto>.broadcast();
  final _joined = StreamController<BroadcastParticipantEvent>.broadcast();
  final _left = StreamController<BroadcastParticipantEvent>.broadcast();
  final _hostState = StreamController<HostConnectionEvent>.broadcast();
  final _error = StreamController<BroadcastErrorEvent>.broadcast();

  final List<SocketSubscription> _subscriptions = [];

  bool _listening = false;

  /// A broadcast was created or went on air.
  Stream<Broadcast> get onNewBroadcast => _newBroadcast.stream;

  /// A broadcast ended, in any of the ways it can.
  Stream<EndedBroadcast> get onEndedBroadcast => _ended.stream;

  /// The home sections the socket told this client to refetch.
  Stream<HomeInvalidatedDto> get onHomeInvalidated => _homeInvalidated.stream;

  /// Somebody joined a room.
  Stream<BroadcastParticipantEvent> get onParticipantJoined => _joined.stream;

  /// Somebody left a room.
  Stream<BroadcastParticipantEvent> get onParticipantLeft => _left.stream;

  /// The host of a room you are in dropped or came back.
  Stream<HostConnectionEvent> get onHostConnectionChanged => _hostState.stream;

  /// Something went wrong; the server is telling this client directly.
  ///
  /// `broadcastError` is the only frame that carries a decision the client has
  /// to make — [BroadcastErrorEvent.recoverable] separates "retry" from
  /// "navigate away", and `BroadcastErrorCode.tokenExpired` is the one that
  /// leads to a token refresh rather than a re-join.
  Stream<BroadcastErrorEvent> get onBroadcastError => _error.stream;

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
          SocketEvent.endedBroadcast,
          (data) => _emit(_ended, _endedBroadcast(data)),
        ),
      )
      ..add(
        _client.on(
          SocketEvent.participantJoined,
          (data) => _emit(_joined, _participantEvent(data)),
        ),
      )
      ..add(
        _client.on(
          SocketEvent.participantLeft,
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
      )
      ..add(
        _client.on(
          SocketEvent.broadcastError,
          (data) => _emit(_error, _broadcastError(data)),
        ),
      )
      ..add(
        _client.on(
          SocketEvent.homeInvalidated,
          (data) {
            final frame = HomeInvalidatedDto.fromJson(
              _asMap(data) ?? <String, dynamic>{},
            );
            _emit(_homeInvalidated, frame);
          },
        ),
      );
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
      _ended.close(),
      _homeInvalidated.close(),
      _joined.close(),
      _left.close(),
      _hostState.close(),
      _error.close(),
    ]);
  }

  /// Decodes a broadcast payload, or reports why it could not be.
  ///
  /// A malformed event is dropped rather than thrown: it arrives on a stream
  /// nothing awaits, and one bad frame must not take the room down.
  Broadcast? _broadcast(dynamic data) {
    return _decode(data, (json) => BroadcastDto.fromJson(json).toDomain);
  }

  /// A broadcast ending.
  ///
  /// The socket's frame is the *ending*, not the whole summary: it carries
  /// the id and the reason, which is all a client needs to stop drawing a room
  /// and to ask the API what to show instead. It is decoded here rather than
  /// through [EndBroadcastDto] so the two spellings the API has used for it —
  /// the response's `broadcast_id` and the event's `broadcastId` — both land,
  /// and a frame with no id is dropped rather than becoming an
  /// `EndedBroadcast` that matches no room.
  EndedBroadcast? _endedBroadcast(dynamic data) {
    return _decode<EndedBroadcast?>(data, (json) {
      final rawId = json['broadcast_id'] ?? json['broadcastId'];
      final broadcastId = Id.fromWire(rawId as String? ?? '');
      if (!broadcastId.isValid) return null;

      final rawTitle = json['broadcast_title'] ?? json['broadcastTitle'];

      return EndedBroadcast(
        broadcastId: broadcastId,
        title: rawTitle is String && rawTitle.isNotEmpty
            ? SingleLineString(rawTitle)
            : SingleLineString.empty,
        creatorId: Id.fromWire(
          (json['creator_id'] ?? json['creatorId']) as String? ?? '',
        ),
        endedAt: DateTime.tryParse(
          (json['ended_at'] ?? json['endedAt']) as String? ?? '',
        ),
        reason: EndReason.fromWire(
          (json['ended_reason'] ?? json['reason']) as String?,
        ),
      );
    });
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

  /// Decodes a [`broadcastError`] frame.
  ///
  /// The payload is `WsError` from `apps/api/src/infrastructure/ws/errors.rs`: a
  /// stable code, a human-readable message, and `recoverable`, which is derived
  /// from the code server-side so it can never contradict it. A frame with no
  /// recognisable code is dropped rather than treated as fatal — an unknown
  /// error is not a reason to tear down a working room.
  BroadcastErrorEvent? _broadcastError(dynamic data) {
    return _decode<BroadcastErrorEvent?>(data, (json) {
      final rawCode = json['code'] as String?;
      final code = BroadcastErrorCode.fromWire(rawCode);
      if (code == null) return null;

      return BroadcastErrorEvent(
        broadcastId: json['broadcastId'] == null
            ? null
            : Id.fromWire(json['broadcastId'] as String),
        code: code,
        message: (json['message'] as String? ?? '').trim(),
        recoverable: json['recoverable'] as bool? ?? code.isRecoverable,
      );
    });
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

/// Why the server sent a `broadcastError`, mirroring `WsErrorCode` in
/// `apps/api/src/infrastructure/ws/errors.rs`.
///
/// The wire value is SCREAMING_SNAKE_CASE and is a stability contract: the API
/// may add a code but never rename one, so matching on it is safe.
enum BroadcastErrorCode {
  tokenExpired('TOKEN_EXPIRED'),
  broadcastForciblyEnded('BROADCAST_FORCIBLY_ENDED'),
  kickedFromRoom('KICKED_FROM_ROOM'),
  roomNotFound('ROOM_NOT_FOUND'),
  mediaServerError('MEDIA_SERVER_ERROR'),
  unsupported('UNSUPPORTED');

  new(this.wire);

  /// The exact string the API sends.
  final String wire;

  /// Whether retrying the same action can succeed.
  ///
  /// Mirrors `WsErrorCode::is_recoverable`: the socket flag is derived from
  /// this server-side, so this default only applies when the frame omits it.
  bool get isRecoverable =>
      this == tokenExpired || this == mediaServerError;

  /// Whether this error means the room is gone and the client should leave it.
  ///
  /// Mirrors `WsErrorCode::is_fatal`: an `unsupported` frame is not recoverable
  /// by retrying, but it is also not a reason to drop a working connection.
  bool get isFatal =>
      this == broadcastForciblyEnded ||
      this == kickedFromRoom ||
      this == roomNotFound;

  static BroadcastErrorCode? fromWire(String? wire) {
    for (final code in BroadcastErrorCode.values) {
      if (code.wire == wire) return code;
    }
    return null;
  }
}

/// A `broadcastError` frame, decoded once here.
final class BroadcastErrorEvent {
  const new({
    required this.code,
    required this.message,
    required this.recoverable,
    this.broadcastId,
  });

  /// The room this concerns, when it concerns one. Connection-scoped errors
  /// (a malformed heartbeat, an unsupported frame, a server shutdown) carry
  /// no id.
  final Id? broadcastId;

  /// The stable code to branch on; never match on [message].
  final BroadcastErrorCode code;

  /// Human-readable detail. Safe to show, never used for control flow.
  final String message;

  /// Whether retrying can succeed — the server derives this from [code].
  final bool recoverable;
}
