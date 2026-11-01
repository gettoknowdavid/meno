import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/chat/model/_model.dart';
import 'package:meno_mobile/features/chat/services/chat_http_service.dart';

/// One broadcast's chat history, newest first, on the shared paged feed.
///
/// # Cursors, not pages
///
/// `GET /broadcasts/{id}/chat/messages` answers with an opaque cursor and says
/// whether another page exists, so [fetchPage] hands that cursor back and
/// [hasNextPage] is nothing more than "the server gave us one" — the same
/// contract every other list in the app uses, which is what let this feed drop
/// its own page arithmetic.
///
/// # Items are messages, not proxies
///
/// A chat row is a [Message] value like a broadcast card is a `Broadcast`:
/// nothing on it is locally overridden, so the list holds the entity itself.
/// An edit replaces the item in place and bumps [revision] — the count does not
/// change, and a count that does not change notifies nobody, so a row nobody
/// could see changing needs its own signal (the same reason the live snapshot
/// is a single value rather than scattered booleans).
final class ChatFeedDataSource extends PagedFeedDataSource<Message> {
  new({required this._http, required this._broadcastId});

  final ChatMessagePageReader _http;
  final Id _broadcastId;

  /// How many messages a page holds.
  static const int pageSize = 50;

  /// Bumped whenever an existing row's body changes.
  ///
  /// The list watches this beside [itemCount]: an edit is a repaint of one
  /// bubble, not a new row, and the base class only publishes the count.
  final revision = ValueNotifier<int>(0);

  @override
  bool itemsAreEqual(Message item1, Message item2) => item1.id == item2.id;

  /// Reads [cursor]'s page — `null` asks for the newest one.
  ///
  /// The API answers newest first (`Order::Desc` in the chat service) and the
  /// list is drawn reversed, so the newest message sits at the bottom of the
  /// screen. Messages a page repeats are dropped by the base class, which is
  /// what makes a refresh that overlaps the current page leave one row per
  /// message.
  @override
  Future<FeedPage<Message>> fetchPage(String? cursor) async {
    final page = await _http.messages(
      _broadcastId,
      cursor: cursor,
      limit: pageSize,
    );

    return FeedPage(
      items: page.items,
      // The server states whether another page exists; a cursor with no page
      // behind it would make the list ask once more for nothing.
      nextCursor: page.hasNextPage ? page.nextCursor : null,
    );
  }

  /// Whether the whole history has been read.
  bool get hasReachedEnd => !hasNextPage && updateWasCalled;

  /// Adds [message] at the top of the list, or refreshes it when it is there.
  ///
  /// A sender sees their own message twice on purpose: locally when the frame
  /// leaves, and again when the server echoes it back. Treating the echo as an
  /// update is what keeps one message one bubble.
  void addMessage(Message message) {
    final index = items.indexWhere((item) => item.id == message.id);
    if (index != -1) {
      _replace(index, message);
      return;
    }
    addItemAtStart(message);
  }

  /// Reflects an edit, if the message is on screen.
  void updateMessage(Message message) {
    final index = items.indexWhere((item) => item.id == message.id);
    if (index == -1) return;
    _replace(index, message);
  }

  /// Drops a deleted message, if the list is holding it.
  void removeMessage(Id messageId) {
    final index = items.indexWhere((item) => item.id == messageId);
    if (index == -1) return;
    removeObject(items[index]);
  }

  void _replace(int index, Message message) {
    if (items[index] == message) return;
    items[index] = message;
    revision.value++;
  }

  @override
  FutureOr<dynamic> onDispose() async {
    revision.dispose();
    await super.onDispose();
  }
}
