/// The socket's vocabulary: what it can carry, and the handle a listener keeps.
///
/// Shared by both ends — a manager names a [SocketEvent] to listen for or to
/// emit, and holds the [SocketSubscription] it gets back. The client that turns
/// those into traffic lives in `_shared/services`; nothing here knows about it,
/// or about Socket.IO.
library;

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
  // Server → client: broadcast lifecycle.
  newBroadcast('newBroadcast'),
  endedBroadcast('endedBroadcast'),
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
}

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
