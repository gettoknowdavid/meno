import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/socket.dart';
import 'package:meno_mobile/_shared/services/socket_client.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';

/// The broadcast events the socket carries.
///
/// Each is exposed as a stream and decoded once here, so a manager that listens
/// for "a broadcast ended" gets an [EndedSignal] rather than a map it has to
/// remember the keys of.
///
/// # Installing is explicit
///
/// [install] attaches the handlers and [detach] removes them; neither is
/// implied by anything else. This adapter used to attach them inside
/// `listen()`, which nothing called — subscribing to its streams did not
/// install anything, so even a working transport delivered nothing (audit §3
/// H06). The account connection owner now decides when delivery is active: it
/// calls [install] before the socket connects and [detach] when the account
/// goes away.
///
/// Both are idempotent. A second [install] would attach a second set of
/// handlers and every event would arrive twice; a second [detach] would cancel
/// handles that are already gone. The client keeps the handlers across a
/// reconnect, so installing once covers every socket the client builds —
/// [onReconnected] is how a manager hears that the socket is new and the server
/// has forgotten its rooms.
final class BroadcastSocketService implements Disposable, ConnectionAdapter {
  new({required this._client, required this._logger});

  final SocketClient _client;
  final MenoLogger _logger;

  final _newBroadcast = StreamController<Broadcast>.broadcast();
  final _ended = StreamController<EndedSignal>.broadcast();
  final _homeInvalidated = StreamController<HomeInvalidatedDto>.broadcast();
  final _joined = StreamController<BroadcastParticipantEvent>.broadcast();
  final _left = StreamController<BroadcastParticipantEvent>.broadcast();
  final _hostState = StreamController<HostConnectionEvent>.broadcast();
  final _error = StreamController<BroadcastErrorEvent>.broadcast();
  final _cohost = StreamController<CohostEvent>.broadcast();
  final _deleted = StreamController<BroadcastDeletedEvent>.broadcast();
  final _liveCount = StreamController<LiveParticipantCountEvent>.broadcast();
  final _scheduled = StreamController<ScheduledBroadcastEvent>.broadcast();
  final _recording = StreamController<RecordingEvent>.broadcast();

  /// The handles [install] created, held so [detach] can cancel exactly them.
  List<SocketSubscription> _handles = [];

  bool _installed = false;

  /// A broadcast was created or went on air.
  Stream<Broadcast> get onNewBroadcast => _newBroadcast.stream;

  /// A broadcast ended, in any of the ways it can.
  ///
  /// Carries [EndedSignal]'s id and reason and nothing else: the frame has no
  /// summary, and fabricating one is what audit §3 M10 was.
  Stream<EndedSignal> get onEndedBroadcast => _ended.stream;

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

  /// The cohost handshake and roster changes: invitation, accept, decline,
  /// demotion, addition and removal.
  Stream<CohostEvent> get onCohostEvent => _cohost.stream;

  /// A broadcast was deleted anywhere in the deployment.
  Stream<BroadcastDeletedEvent> get onBroadcastDeleted => _deleted.stream;

  /// The server's own live-participant count, independent of join/leave frames.
  Stream<LiveParticipantCountEvent> get onLiveParticipantCount =>
      _liveCount.stream;

  /// A scheduled broadcast has reached its start time.
  Stream<ScheduledBroadcastEvent> get onScheduledBroadcast =>
      _scheduled.stream;

  /// A recording became ready or was published.
  Stream<RecordingEvent> get onRecordingEvent => _recording.stream;

  /// The underlying socket dropped and came back.
  ///
  /// The client is looking at a fresh socket with no rooms on it, so a client
  /// that is still in a broadcast has to say so again — see
  /// the live session's transport reconnect.
  Stream<void> get onReconnected => _client.onReconnected;

  /// Attaches one handler per event, for the life of the account.
  ///
  /// Idempotent: a second call while installed does nothing, because attaching
  /// twice would deliver every event twice.
  @override
  void install() {
    if (_installed) return;
    _installed = true;

    _handles = [
      _client.on(
        SocketEvent.cohostInvitation,
        (data) => _dispatch(_cohostInvitationEvent(data)),
      ),
      _client.on(
        SocketEvent.cohostAccepted,
        (data) => _dispatch(_cohostAcceptedEvent(data)),
      ),
      _client.on(
        SocketEvent.cohostDeclined,
        (data) => _dispatch(_cohostDeclinedEvent(data)),
      ),
      _client.on(
        SocketEvent.cohostDemotion,
        (data) => _dispatch(_cohostDemotionEvent(data)),
      ),
      _client.on(
        SocketEvent.newCohost,
        (data) => _dispatch(_newCohostEvent(data)),
      ),
      _client.on(
        SocketEvent.removedCohost,
        (data) => _dispatch(_removedCohostEvent(data)),
      ),
      _client.on(
        SocketEvent.broadcastDeleted,
        (data) => _dispatch(_broadcastDeletedEvent(data)),
      ),
      _client.on(
        SocketEvent.numberOfLiveParticipants,
        (data) => _dispatch(_liveParticipantCountEvent(data)),
      ),
      _client.on(
        SocketEvent.scheduledBroadcast,
        (data) => _dispatch(_scheduledBroadcastEvent(data)),
      ),
      _client.on(
        SocketEvent.recordingReady,
        (data) => _dispatch(_recordingEvent(data, published: false)),
      ),
      _client.on(
        SocketEvent.recordingPublished,
        (data) => _dispatch(_recordingEvent(data, published: true)),
      ),
      _client.on(
        SocketEvent.newBroadcast,
        (data) => _dispatch(_newBroadcastEvent(data)),
      ),
      _client.on(
        SocketEvent.endedBroadcast,
        (data) => _dispatch(_endedBroadcastEvent(data)),
      ),
      _client.on(
        SocketEvent.participantJoined,
        (data) => _dispatch(
          _decode(
            data,
            (json) =>
                ParticipantJoinedEvent(participant: _participantEvent(json)),
          ),
        ),
      ),
      _client.on(
        SocketEvent.participantLeft,
        (data) => _dispatch(
          _decode(
            data,
            (json) =>
                ParticipantLeftEvent(participant: _participantEvent(json)),
          ),
        ),
      ),
      _client.on(
        SocketEvent.hostDisconnected,
        (data) => _dispatch(_hostStateEvent(data, connected: false)),
      ),
      _client.on(
        SocketEvent.hostReconnected,
        (data) => _dispatch(_hostStateEvent(data, connected: true)),
      ),
      _client.on(
        SocketEvent.broadcastError,
        (data) => _dispatch(_broadcastErrorEvent(data)),
      ),
      _client.on(
        SocketEvent.homeInvalidated,
        (data) => _dispatch(
          _decode(
            data,
            (json) => HomeInvalidatedEvent(
              invalidation: HomeInvalidatedDto.fromJson(json),
            ),
          ),
        ),
      ),
    ];
  }

  /// Removes every handler [install] attached.
  ///
  /// Idempotent, and the handles are copied to a local first: a new install
  /// racing this one would otherwise overwrite the list mid-detach and leave
  /// the old handlers attached (the M09 bug shape).
  @override
  void detach() {
    if (!_installed) return;
    _installed = false;

    final handles = _handles;
    _handles = [];
    for (final handle in handles) {
      handle.cancel();
    }
  }

  @override
  FutureOr<dynamic> onDispose() async {
    detach();
    await Future.wait([
      _newBroadcast.close(),
      _ended.close(),
      _homeInvalidated.close(),
      _joined.close(),
      _left.close(),
      _hostState.close(),
      _error.close(),
      _cohost.close(),
      _deleted.close(),
      _liveCount.close(),
      _scheduled.close(),
      _recording.close(),
    ]);
  }

  /// Routes one decoded event to its stream.
  void _dispatch(BroadcastEvent? event) {
    switch (event) {
      case null:
        return;
      case NewBroadcastEvent(:final broadcast):
        _emit(_newBroadcast, broadcast);
      case final EndedSignal event:
        _emit(_ended, event);
      case HomeInvalidatedEvent(:final invalidation):
        _emit(_homeInvalidated, invalidation);
      case ParticipantJoinedEvent(:final participant):
        _emit(_joined, participant);
      case ParticipantLeftEvent(:final participant):
        _emit(_left, participant);
      case HostConnectionChangedEvent(:final host):
        _emit(_hostState, host);
      case BroadcastFailedEvent(:final error):
        _emit(_error, error);
      case final CohostEvent event:
        _emit(_cohost, event);
      case final BroadcastDeletedEvent event:
        _emit(_deleted, event);
      case final LiveParticipantCountEvent event:
        _emit(_liveCount, event);
      case final ScheduledBroadcastEvent event:
        _emit(_scheduled, event);
      case final RecordingEvent event:
        _emit(_recording, event);
    }
  }

  /// Decodes a broadcast payload, or reports why it could not be.
  ///
  /// A malformed event is dropped rather than thrown: it arrives on a stream
  /// nothing awaits, and one bad frame must not take the room down.
  BroadcastEvent? _newBroadcastEvent(dynamic data) {
    return _decode(
      data,
      (json) =>
          NewBroadcastEvent(broadcast: BroadcastDto.fromJson(json).toDomain),
    );
  }

  /// A broadcast ending.
  ///
  /// The socket's frame is the *ending*, not the whole summary: `{broadcastId,
  /// reason}` and nothing more. Both spellings the API has used are accepted —
  /// the response's `broadcast_id` and the event's `broadcastId` — and a frame
  /// with no valid id is dropped rather than becoming a signal that matches no
  /// room. No title, creator or duration is read here, because the frame has
  /// none: inventing them is exactly the M10 bug.
  BroadcastEvent? _endedBroadcastEvent(dynamic data) {
    return _decode<BroadcastEvent?>(data, (json) {
      final rawId = json['broadcast_id'] ?? json['broadcastId'];
      final broadcastId = Id.fromWire(rawId as String? ?? '');
      if (!broadcastId.isValid) return null;

      return EndedSignal(
        broadcastId: broadcastId,
        reason: EndReason.fromWire(
          (json['ended_reason'] ?? json['reason']) as String?,
        ),
      );
    });
  }

  BroadcastParticipantEvent _participantEvent(Map<String, dynamic> json) {
    final participant = json['participant'];
    return BroadcastParticipantEvent(
      broadcastId: Id.fromWire(json['broadcastId'] as String? ?? ''),
      count: (json['count'] as num?)?.toInt() ?? 0,
      participant: participant == null
          ? null
          : ParticipantDto.fromJson(_asMap(participant)!).toDomain,
    );
  }

  BroadcastEvent? _hostStateEvent(dynamic data, {required bool connected}) {
    return _decode(
      data,
      (json) => HostConnectionChangedEvent(
        host: HostConnectionEvent(
          broadcastId: Id.fromWire(json['broadcastId'] as String? ?? ''),
          connected: connected,
        ),
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
  BroadcastEvent? _broadcastErrorEvent(dynamic data) {
    return _decode<BroadcastEvent?>(data, (json) {
      final rawCode = json['code'] as String?;
      final code = BroadcastErrorCode.fromWire(rawCode);
      if (code == null) return null;

      return BroadcastFailedEvent(
        error: BroadcastErrorEvent(
          broadcastId: json['broadcastId'] == null
              ? null
              : Id.fromWire(json['broadcastId'] as String),
          code: code,
          message: (json['message'] as String? ?? '').trim(),
          recoverable: json['recoverable'] as bool? ?? code.isRecoverable,
        ),
      );
    });
  }

  BroadcastEvent? _cohostInvitationEvent(dynamic data) {
    return _decode<BroadcastEvent?>(data, (json) {
      final broadcastId = _wireId(json['broadcastId']);
      final invitationId = _wireId(json['invitationId']);
      if (broadcastId == null || invitationId == null) return null;

      return CohostInvitedEvent(
        broadcastId: broadcastId,
        invitationId: invitationId,
        inviterName: (json['inviterName'] as String? ?? '').trim(),
        broadcastTitle: (json['broadcastTitle'] as String? ?? '').trim(),
      );
    });
  }

  BroadcastEvent? _cohostAcceptedEvent(dynamic data) {
    return _decode<BroadcastEvent?>(data, (json) {
      final broadcastId = _wireId(json['broadcastId']);
      final userId = _wireId(json['userId']);
      if (broadcastId == null || userId == null) return null;

      return CohostAcceptedEvent(
        broadcastId: broadcastId,
        userId: userId,
        userName: (json['userName'] as String? ?? '').trim(),
      );
    });
  }

  BroadcastEvent? _cohostDeclinedEvent(dynamic data) {
    return _decode<BroadcastEvent?>(data, (json) {
      final broadcastId = _wireId(json['broadcastId']);
      final userId = _wireId(json['userId']);
      if (broadcastId == null || userId == null) return null;

      return CohostDeclinedEvent(broadcastId: broadcastId, userId: userId);
    });
  }

  BroadcastEvent? _cohostDemotionEvent(dynamic data) {
    return _decode<BroadcastEvent?>(data, (json) {
      final broadcastId = _wireId(json['broadcastId']);
      if (broadcastId == null) return null;

      return CohostDemotionEvent(
        broadcastId: broadcastId,
        livekitToken: json['token'] as String? ?? '',
      );
    });
  }

  BroadcastEvent? _newCohostEvent(dynamic data) {
    return _decode<BroadcastEvent?>(data, (json) {
      final broadcastId = _wireId(json['broadcastId']);
      final userId = _wireId(json['userId']);
      if (broadcastId == null || userId == null) return null;

      return NewCohostEvent(
        broadcastId: broadcastId,
        userId: userId,
        userName: (json['userName'] as String? ?? '').trim(),
        livekitToken: json['livekitToken'] as String? ?? '',
      );
    });
  }

  BroadcastEvent? _removedCohostEvent(dynamic data) {
    return _decode<BroadcastEvent?>(data, (json) {
      final broadcastId = _wireId(json['broadcastId']);
      final userId = _wireId(json['userId']);
      if (broadcastId == null || userId == null) return null;

      return RemovedCohostEvent(broadcastId: broadcastId, userId: userId);
    });
  }

  BroadcastEvent? _broadcastDeletedEvent(dynamic data) {
    return _decode<BroadcastEvent?>(data, (json) {
      final broadcastId = _wireId(json['broadcastId']);
      if (broadcastId == null) return null;

      return BroadcastDeletedEvent(broadcastId: broadcastId);
    });
  }

  BroadcastEvent? _liveParticipantCountEvent(dynamic data) {
    return _decode<BroadcastEvent?>(data, (json) {
      final broadcastId = _wireId(json['broadcastId']);
      if (broadcastId == null) return null;

      return LiveParticipantCountEvent(
        broadcastId: broadcastId,
        count: (json['count'] as num?)?.toInt() ?? 0,
      );
    });
  }

  BroadcastEvent? _scheduledBroadcastEvent(dynamic data) {
    return _decode(
      data,
      (json) => ScheduledBroadcastEvent(
        broadcast: BroadcastDto.fromJson(json).toDomain,
      ),
    );
  }

  BroadcastEvent? _recordingEvent(dynamic data, {required bool published}) {
    return _decode<BroadcastEvent?>(data, (json) {
      final broadcastId = _wireId(json['broadcastId']);
      if (broadcastId == null) return null;

      return published
          ? RecordingPublishedEvent(broadcastId: broadcastId)
          : RecordingReadyEvent(broadcastId: broadcastId);
    });
  }

  static Id? _wireId(Object? raw) {
    if (raw is! String) return null;
    final id = Id.fromWire(raw);
    return id.isValid ? id : null;
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

  void _emit<T>(StreamController<T> controller, T event) {
    if (!controller.isClosed) controller.add(event);
  }

  static Map<String, dynamic>? _asMap(dynamic data) =>
      data is Map<String, dynamic> ? data : null;
}
