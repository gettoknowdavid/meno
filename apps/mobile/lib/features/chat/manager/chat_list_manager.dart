import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/chat/manager/chat_feed_data_source.dart';
import 'package:meno_mobile/features/chat/model/_model.dart';
import 'package:meno_mobile/features/chat/services/chat_http_service.dart';
import 'package:meno_mobile/features/chat/services/chat_socket_service.dart';

/// One room's chat list: its feed, the frames that change it, and whether
/// somebody is waiting to read it.
///
/// # Two sources, both needed
///
/// The API's page is the history at the moment the room opened; the socket's
/// frames are every change after that. Neither alone is enough — a list built
/// only from frames would open empty for a listener who joined late, and one
/// that only read the endpoint would never show the message that just arrived.
///
/// # Why the unread flag lives here
///
/// A message arriving is the list's news, not the tab bar's: the shell draws a
/// dot because this manager says there is something new, and it says so only
/// while the chat is not the screen in front of the person. [setVisible] is how
/// the shell tells it which one is in front — once on every tab change, not
/// once per frame.
final class ChatListManager implements Disposable {
  new({
    required ChatMessagePageReader http,
    required this._socket,
    required Id broadcastId,
    required this._userId,
    required this._logger,
  }) : _broadcastId = broadcastId,
       feed = ChatFeedDataSource(http: http, broadcastId: broadcastId) {
    // Subscriptions first, then the read they are piped into: a page read
    // before the handlers exist would miss every message that lands between the
    // response and the first listener.
    initialize = Command.createSyncNoParamNoResult(
      _subscribe,
      errorFilterFn: menoExceptionFilter,
    )..pipeToCommand(feed.updateDataCommand);
  }

  final ChatSocketChannel _socket;
  final Id _broadcastId;
  final Id _userId;
  final MenoLogger _logger;

  /// The room's history. Never replaced, only mutated — the list watches it.
  final ChatFeedDataSource feed;

  /// Whether a message arrived while the chat was not on screen.
  final hasUnread = ValueNotifier<bool>(false);

  StreamSubscription<Message>? _onNewMessage;
  StreamSubscription<Message>? _onEditedMessage;
  StreamSubscription<DeletedMessageSignal>? _onDeletedMessage;
  StreamSubscription<void>? _onReconnected;

  bool _isInitialized = false;

  /// Whether the chat is the screen in front of the person right now.
  bool _isVisible = false;

  /// Subscribes to the room's frames and loads the history once.
  late final Command<void, void> initialize;

  /// Whether anything has been asked for yet — what a widget reads to tell
  /// "loading" from "empty".
  bool get updateWasCalled => feed.updateWasCalled;

  /// Tells the manager whether its list is being looked at.
  ///
  /// Coming back to the chat clears the dot: the person is now reading the very
  /// messages it was announcing.
  void setVisible(bool visible) {
    _isVisible = visible;
    if (visible) markRead();
  }

  /// Marks everything as read, whatever the visibility.
  void markRead() {
    if (!hasUnread.value) return;
    hasUnread.value = false;
  }

  void _subscribe() {
    if (_isInitialized) return;
    _isInitialized = true;

    _onNewMessage = _socket.onNewMessage.listen(_handleNewMessage);
    _onEditedMessage = _socket.onEditedMessage.listen(_handleEditedMessage);
    _onDeletedMessage = _socket.onDeletedMessage.listen(_handleDeletedMessage);
    _onReconnected = _socket.onReconnected.listen(_handleReconnected);
  }

  void _handleNewMessage(Message message) {
    if (message.broadcastId != _broadcastId) return;

    feed.addMessage(message);

    // One's own message is not unread news, and neither is anything that lands
    // while the chat is the screen being looked at.
    if (_isVisible || message.senderId == _userId) return;
    hasUnread.value = true;
  }

  void _handleEditedMessage(Message message) {
    if (message.broadcastId != _broadcastId) return;
    feed.updateMessage(message);
  }

  void _handleDeletedMessage(DeletedMessageSignal signal) {
    if (signal.broadcastId != _broadcastId) return;
    feed.removeMessage(Id.fromString(signal.messageId));
  }

  /// Reads the newest page again after a reconnect.
  ///
  /// The socket the client holds is a new one, and frames sent while it was
  /// down reached nobody: the history is the only place those messages still
  /// exist. A refresh keeps the cursor arbitration in one place, so a burst of
  /// reconnects costs one read rather than one per event.
  void _handleReconnected(void _) {
    if (!feed.updateWasCalled) return;
    _logger.i(
      'ChatListManager: socket reconnected, refreshing the chat',
      tag: MenoLogTag.chat.label,
    );
    unawaited(feed.refresh());
  }

  @override
  FutureOr<dynamic> onDispose() async {
    await _onNewMessage?.cancel();
    await _onEditedMessage?.cancel();
    await _onDeletedMessage?.cancel();
    await _onReconnected?.cancel();

    _onNewMessage = null;
    _onEditedMessage = null;
    _onDeletedMessage = null;
    _onReconnected = null;
    _isInitialized = false;

    initialize.dispose();
    hasUnread.dispose();
    await feed.onDispose();
  }
}
