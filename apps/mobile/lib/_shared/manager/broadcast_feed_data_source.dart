import 'dart:async';

import 'package:meno_mobile/_shared/manager/feed_data_source.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

/// One broadcast list, read from the API and kept current by the socket.
///
/// # Cursors, not page numbers
///
/// `/broadcasts` answers with an opaque cursor and says whether another page
/// exists. The feed stores the last cursor it was given and hands it back, and
/// overrides [hasNextPage] accordingly — the base class's page arithmetic is for
/// endpoints that count, and this one deliberately does not: a page number in a
/// list that reorders as broadcasts start and end skips and repeats rows.
///
/// # What the socket does to the list
///
/// | Feed          | A broadcast starts     | A broadcast ends          |
/// |---------------|------------------------|---------------------------|
/// | now live      | prepended              | removed                   |
/// | for you       | prepended              | removed                   |
/// | recently live | —                      | prepended (and dated)     |
/// | anything else | —                      | —                         |
///
/// A socket is optional: a feed built without one is purely HTTP-backed, which
/// is what a profile's or a search result's list is.
class BroadcastFeedDataSource extends PagedFeedDataSource<Broadcast> {
  new({
    required this._http,
    required this._query,
    BroadcastSocketService? socket,
    super.fetchMoreEnabled = true,
  }) {
    _bindSocketService(socket);
  }

  final BroadcastHttpService _http;

  BroadcastQuery _query;

  /// The cursor of the last page that was read, or null before the first one.
  ///
  /// The base class's pages are an implementation detail of endpoints that
  /// count; this is what actually says whether there is more to read.
  String? _cursor;

  StreamSubscription<Broadcast>? _newBroadcastSub;
  StreamSubscription<EndedBroadcast>? _endedBroadcastSub;

  /// The filters the feed is currently built from.
  BroadcastQuery get query => _query;

  @override
  bool get hasNextPage => _cursor != null;

  @override
  bool itemsAreEqual(Broadcast a, Broadcast b) => a.id == b.id;

  @override
  Future<void> updateFeedData() async {
    // A refresh always starts at the first page: the list may have changed
    // under us, and appending to a stale page is how a feed grows duplicates.
    final page = await _http.list(_query.withCursor(null));

    items
      ..clear()
      ..addAll(page.items.map((summary) => summary.toBroadcast()));
    _cursor = page.nextCursor;

    flushItemCount();
  }

  @override
  Future<void> requestNextPage() async {
    final cursor = _cursor;
    if (cursor == null) return;

    final page = await _http.list(_query.withCursor(cursor));

    // Deduplicated on the way in: a broadcast that started between the two
    // requests can appear in both pages, and the list is keyed by id.
    for (final summary in page.items) {
      final broadcast = summary.toBroadcast();
      if (items.any((existing) => itemsAreEqual(existing, broadcast))) continue;
      items.add(broadcast);
    }
    _cursor = page.nextCursor;

    flushItemCount();
  }

  /// Replaces the filters and refreshes from the first page.
  void updateQuery(BroadcastQuery newQuery) {
    _query = newQuery;
    _cursor = null;
    updateDataCommand.run();
  }

  /// Subscribes to the events this feed's filters make relevant.
  void _bindSocketService(BroadcastSocketService? socket) {
    final type = _query.type;
    if (socket == null || type == null) return;

    switch (type) {
      case BroadcastsType.forYou:
      case BroadcastsType.nowLive:
        _newBroadcastSub = socket.onNewBroadcast.listen(_onNewBroadcast);
        _endedBroadcastSub = socket.onEndedBroadcast.listen(_onEndedBroadcast);

      case BroadcastsType.recentlyLive:
        _endedBroadcastSub = socket.onEndedBroadcast.listen(_onRecentlyEnded);
    }
  }

  void _onNewBroadcast(Broadcast broadcast) {
    if (items.any((existing) => itemsAreEqual(existing, broadcast))) return;
    addItemAtStart(broadcast);
  }

  void _onEndedBroadcast(EndedBroadcast ended) {
    final index = items.indexWhere((item) => item.id == ended.broadcastId);
    if (index == -1) return;

    removeObject(items[index]);
  }

  void _onRecentlyEnded(EndedBroadcast event) {
    final broadcast = event.details;
    if (items.any((existing) => itemsAreEqual(existing, broadcast))) return;
    addItemAtStart(broadcast);
  }

  @override
  void reset() {
    _cursor = null;
    super.reset();
  }

  @override
  FutureOr<dynamic> onDispose() async {
    await _newBroadcastSub?.cancel();
    _newBroadcastSub = null;
    await _endedBroadcastSub?.cancel();
    _endedBroadcastSub = null;

    return await super.onDispose();
  }
}
