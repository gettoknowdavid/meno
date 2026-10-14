/// The socket's vocabulary: what it can carry, and the handle a listener keeps.
///
/// Shared by both ends — a manager names a [SocketEvent] to listen for or to
/// emit, and holds the [SocketSubscription] it gets back. The client that turns
/// those into traffic lives in `_shared/services`; nothing here knows about it,
/// or about Socket.IO.
library;

/// Every event this app sends or listens for over the socket.
///
/// The value is the wire name — the API's own spelling. It is carried
/// separately from the member name so an event can be renamed on the wire
/// without renaming Dart, and so no call site has to build a string.
enum SocketEvent {
  newBroadcast('newBroadcast'),
  endedBroadcast('endedBroadcast'),
  leaveBroadcast('leaveBroadcast'),
  startedBroadcast('startedBroadcast'),
  joinBroadcast('joinBroadcast'),
  endBroadcast('endBroadcast'),
  newBroadcastListener('newBroadcastListener'),
  broadcastListenerLeft('broadcastListenerLeft'),
  newMessage('newMessage'),
  editedMessage('editedMessage'),
  deletedMessage('deletedMessage'),
  sendChatMessage('sendChatMessage'),
  editChatMessage('editChatMessage'),
  deleteChatMessage('deleteChatMessage'),
  getChatMessages('getChatMessages'),
  hostDisconnected('hostDisconnected'),
  hostReconnected('hostReconnected'),
  notification('notification');

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
