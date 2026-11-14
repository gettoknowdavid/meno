import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_shared/manager/feed_data_source.dart';
import 'package:meno_mobile/features/notification/model/_model.dart';
import 'package:meno_mobile/features/notification/services/notifications_http_service.dart';

/// This account's inbox, one cursor page at a time, grouped for the list.
///
/// # Cursors, not pages
///
/// `GET /notifications` answers with an opaque cursor and says whether another
/// page exists, so [fetchPage] hands that cursor back and [hasNextPage] is
/// nothing more than "the server gave us one" — the same contract every other
/// list uses.
///
/// # Proxies, not rows
///
/// A card offers two actions and draws one local override (whether it is read),
/// so the list holds [NotificationProxy]s. A refresh updates the proxy
/// instead of replacing it, which is what keeps a card that is animating out
/// alive and a scroll position where the reader left it.
final class NotificationsFeedSource
    extends PagedFeedDataSource<NotificationProxy> {
  /// Creates a feed over the notifications service.
  new(this._http);

  final NotificationsHttpService _http;

  /// Proxies by notification id, so a refresh finds the card it already has.
  final _proxyCache = <String, NotificationProxy>{};

  /// The list's rows: headings interleaved with the notifications under them.
  final groupedItems = ValueNotifier<List<NotificationListItem>>([]);

  Timer? _pruneTimer;
  bool _isDisposed = false;

  @override
  bool itemsAreEqual(NotificationProxy item1, NotificationProxy item2) =>
      item1.notification.id == item2.notification.id;

  /// Reads [cursor]'s page — `null` asks for the newest one.
  ///
  /// The API answers newest first, and a row without an id cannot be deleted or
  /// marked read, so it is left out rather than handed to a card that cannot
  /// act on it.
  @override
  Future<FeedPage<NotificationProxy>> fetchPage(String? cursor) async {
    final page = await _http.getNotifications(cursor: cursor);

    return FeedPage(
      items: [
        for (final notification in page.items)
          if (notification.id != null) _proxyFor(notification),
      ],
      // The server states whether another page exists; a cursor with no page
      // behind it would make the list ask once more for nothing.
      nextCursor: page.hasNextPage ? page.nextCursor : null,
    );
  }

  /// Keeps the grouped rows in step with the flat list, whatever changed it.
  @override
  void refreshItemCount() {
    super.refreshItemCount();
    groupedItems.value = _buildGrouped();
    _schedulePrune();
  }

  /// The proxy for [notification], updated in place when one already exists.
  NotificationProxy _proxyFor(Notification notification) {
    final id = notification.id!;
    final cached = _proxyCache[id];

    if (cached != null) {
      cached.notification = notification;
      return cached;
    }

    final proxy = NotificationProxy(notification);

    // A deleted row leaves the list the moment the write is accepted; the card
    // that asked for it is the one that watches it.
    proxy.delete.listen((_, _) {
      removeObject(proxy);
      _proxyCache.remove(id);
    });

    _proxyCache[id] = proxy;
    return proxy;
  }

  /// Drops proxies the list no longer holds.
  ///
  /// Delayed, because a row that just left is still animating out and its card
  /// still reads it; disposing on the same frame is what makes a removed card
  /// throw.
  void _schedulePrune() {
    _pruneTimer?.cancel();
    _pruneTimer = Timer(const Duration(milliseconds: 400), () {
      if (_isDisposed) return;

      final activeIds = items.map((proxy) => proxy.notification.id).toSet();
      final staleIds = _proxyCache.keys
          .where((id) => !activeIds.contains(id))
          .toList();

      for (final id in staleIds) {
        _proxyCache.remove(id)?.dispose();
      }
    });
  }

  /// Groups the list into today, this week and older, newest first.
  List<NotificationListItem> _buildGrouped() {
    final now = DateTime.now();
    final today = DateTime(now.year, now.month, now.day);
    final weekAgo = today.subtract(const Duration(days: 7));

    final currentItems = [...items];
    currentItems.sort((a, b) {
      final aDate = a.notification.createdAt ?? DateTime(0);
      final bDate = b.notification.createdAt ?? DateTime(0);
      return bDate.compareTo(aDate);
    });

    final todayItems = <NotificationProxy>[];
    final thisWeekItems = <NotificationProxy>[];
    final olderItems = <NotificationProxy>[];

    for (final proxy in currentItems) {
      final date = proxy.notification.createdAt ?? DateTime(0);
      final day = DateTime(date.year, date.month, date.day);

      if (!day.isBefore(today)) {
        todayItems.add(proxy);
      } else if (!day.isBefore(weekAgo)) {
        thisWeekItems.add(proxy);
      } else {
        olderItems.add(proxy);
      }
    }

    return [
      if (todayItems.isNotEmpty) ...[
        const NotificationSectionHeader('Today'),
        ...todayItems.map(NotificationListEntry.new),
      ],
      if (thisWeekItems.isNotEmpty) ...[
        const NotificationSectionHeader('This week'),
        ...thisWeekItems.map(NotificationListEntry.new),
      ],
      if (olderItems.isNotEmpty) ...[
        const NotificationSectionHeader('Older'),
        ...olderItems.map(NotificationListEntry.new),
      ],
    ];
  }

  @override
  FutureOr<dynamic> onDispose() {
    _isDisposed = true;
    _pruneTimer?.cancel();
    groupedItems.dispose();

    for (final proxy in _proxyCache.values) {
      proxy.dispose();
    }
    _proxyCache.clear();

    return super.onDispose();
  }
}
