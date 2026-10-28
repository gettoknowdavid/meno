/// The socket's vocabulary: what it can carry, and the handle a listener keeps.
///
/// Shared by both ends — a manager names a [SocketEvent] to listen for or to
/// emit, and holds the [SocketSubscription] it gets back. The client that turns
/// those into traffic lives in `_shared/services`; nothing here knows about it,
/// or about Socket.IO.
library;

import 'dart:convert';

/// Every event this app sends or listens for over the socket.
///
/// The value is the wire name — the API's own spelling, from `WsEvent` in
/// `apps/api/src/infrastructure/ws/model.rs`. It is carried separately from the
/// member name so an event can be renamed on the wire without renaming Dart,
/// and so no call site has to build a string.
///
/// Only events the server actually emits or accepts belong here. The pre-v2
/// `startedBroadcast`, `joinBroadcast`, `leaveBroadcast` and `endBroadcast`
/// names are gone: the server's `WsEvent` has no such variants, so it refuses
/// every frame that carries one (§10.2's client→server set is `heartbeat` plus
/// chat, and entering or leaving a room is an HTTP call, not a socket frame).
/// The participant events are the server's `participantJoined`/`participantLeft`,
/// not the old `newBroadcastListener`/`broadcastListenerLeft` spellings.
enum SocketEvent {
  // Both directions: liveness. The server pings and expects an echo, and
  // accepts the client's own heartbeat as an activity signal in return.
  heartbeat('heartbeat'),
  // Server → client: broadcast lifecycle.
  newBroadcast('newBroadcast'),
  endedBroadcast('endedBroadcast'),
  homeInvalidated('homeInvalidated'),
  broadcastError('broadcastError'),
  // Server → client: host presence.
  hostDisconnected('hostDisconnected'),
  hostReconnected('hostReconnected'),
  // Server → client: participants.
  participantJoined('participantJoined'),
  participantLeft('participantLeft'),
  // Server → client: chat and notifications.
  newMessage('newMessage'),
  editedMessage('editedMessage'),
  deletedMessage('deletedMessage'),
  notification('notification'),
  // Client → server: chat (the only frames the server accepts besides
  // heartbeat).
  sendChatMessage('sendMessage'),
  editChatMessage('editMessage'),
  deleteChatMessage('deleteMessage');

  new(this.value);

  final String value;

  /// The event [wire] names, or `null` when the server named one this build
  /// does not know.
  ///
  /// An unknown event is not a crash: the API is allowed to add events, and a
  /// client receiving one is a version-skew observation, not a failure.
  static SocketEvent? fromWire(String? wire) {
    for (final event in SocketEvent.values) {
      if (event.value == wire) return event;
    }
    return null;
  }
}

/// The envelope every frame carries, in both directions.
///
/// `WsPayload`/`ClientMessage` in `apps/api/src/infrastructure/ws/dto.rs`: an
/// event name and an event-specific body, deliberately thin so neither side
/// needs a coordinated release to add an event. There are no acknowledgements
/// and no framing beyond this — the backend has none, and inventing one would
/// only hide real failures.
final class SocketFrame {
  const new({required this.event, this.data});

  final SocketEvent event;
  final Object? data;

  /// The exact text put on the wire.
  String encode() => jsonEncode({'event': event.value, 'data': data});

  /// Decodes one received frame, or `null` when it is not one.
  ///
  /// A malformed frame is dropped rather than thrown: it arrives on a stream
  /// nothing awaits, and one bad frame must not take a working connection
  /// down. The caller logs the drop; this stays pure.
  static SocketFrame? tryDecode(String raw) {
    final Object? decoded;
    try {
      decoded = jsonDecode(raw);
    } on FormatException {
      return null;
    }
    if (decoded is! Map<String, dynamic>) return null;
    if (decoded['event'] is! String) return null;
    final event = SocketEvent.fromWire(decoded['event'] as String);
    if (event == null) return null;
    return SocketFrame(event: event, data: decoded['data']);
  }
}

/// Why the socket last closed, kept rather than discarded (audit H07).
///
/// A refused handshake puts the HTTP status in [code]; a socket the server
/// closed puts its close code there. Either can be `null` when the transport
/// failed without one — a DNS failure, say — in which case [reason] is all
/// there is to say.
final class SocketClose {
  const new({required this.code, required this.reason, required this.at});

  final int? code;
  final String? reason;
  final DateTime at;

  @override
  String toString() => 'SocketClose(code: $code, reason: $reason, at: $at)';
}

/// Where the socket's connection currently stands.
///
/// One value at a time, published as it changes: `idle` before the first
/// `start`, `connecting` for that first attempt, `open` while frames can be
/// sent, `reconnecting` between a drop and the next attempt, and `closed` once
/// no further attempt will be made — by `stop`, or because a terminal reason
/// ended the chain.
enum ConnectionPhase { idle, connecting, open, reconnecting, closed }

/// A live subscription to one socket event.
///
/// Handed back by the socket client's `on` and held by whatever is listening
/// until it cancels. This is the contract rather than the implementation: the
/// socket client is the only thing that can remove a handler, and it is the
/// only thing that needs to know how — a listener holds the handle and calls
/// [cancel].
///
/// Cancelling more than once is harmless.
abstract interface class SocketSubscription {
  /// The event being listened for.
  SocketEvent get event;

  /// Stops listening.
  void cancel();
}
