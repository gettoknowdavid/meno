import 'dart:async';

import 'package:meno_mobile/_shared/manager/feed_data_source.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

/// One broadcast list, read from the API and kept current by the socket.
///
/// # Cursors, not page numbers
///
/// `/broadcasts` answers with an opaque cursor and says whether another page
/// exists. The feed stores the last cursor it was given and hands it back, and
/// overrides [hasNextPage] accordingly — the base class's page arithmetic is
/// for endpoints that count, and this one deliberately does not: a page number
/// in a list that reorders as broadcasts start and end skips and repeats rows.
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
///
/// # What `homeInvalidated` does to it
///
/// Prepend and remove cover the broadcasts this client was told about by id.
/// `homeInvalidated` is the other half: a mutation this client was not told
/// about — a broadcast somebody else took live, an edit, a soft delete —
/// leaves the list quietly stale. Every feed refetches its first page on that
/// frame, so a section is never more than one event behind. The `isRunning`
/// guard collapses a burst into the refresh already in flight rather than
/// queueing one per frame.
class BroadcastFeedDataSource extends PagedFeedDataSource<Broadcast> {
  new({
    required this._http,
    required this._query,
    this.socket,
    super.fetchMoreEnabled = true,
  }) {
    _bindSocketService(socket);
  }

  final BroadcastHttpService _http;

  /// The socket the feed listens on, kept so [updateQuery] can hand the new
  /// filters to it.
  final BroadcastSocketService? socket;

  BroadcastQuery _query;

  /// The cursor of the last page that was read, or null before the first one.
  ///
  /// The base class's pages are an implementation detail of endpoints that
  /// count; this is what actually says whether there is more to read.
  String? _cursor;

  StreamSubscription<Broadcast>? _newBroadcastSub;
  StreamSubscription<EndedBroadcast>? _endedBroadcastSub;
  StreamSubscription<HomeInvalidatedDto>? _homeInvalidatedSub;

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
      ..addAll(
        page.items.map((summary) => summary.toBroadcast()).where(_belongsTo),
      );
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
      if (!_belongsTo(broadcast)) continue;
      if (items.any((existing) => itemsAreEqual(existing, broadcast))) continue;
      items.add(broadcast);
    }
    _cursor = page.nextCursor;

    flushItemCount();
  }

  /// Replaces the filters and refreshes from the first page.
  ///
  /// The socket subscriptions are rebuilt with them, because which events
  /// belong in a list is a property of its filters: a feed switched from "now
  /// live" to "recently live" that kept its old subscriptions would file a
  /// broadcast that just started into a list about ones that ended. The old
  /// subscriptions are cancelled without being awaited — an event that slips
  /// through in that moment lands in a list that [updateDataCommand] is about
  /// to replace anyway.
  void updateQuery(BroadcastQuery newQuery) {
    _query = newQuery;
    _cursor = null;

    unawaited(_unbindSocketService());
    _bindSocketService(socket);

    updateDataCommand.run();
  }

  /// Whether [broadcast] belongs in this list.
  ///
  /// `recentlyLive` is the one section the endpoint cannot express on its own.
  /// Ending a broadcast sets `status = inactive` and `end_time`, and a draft or
  /// a scheduled one is `inactive` too, so `status=inactive` is the closest the
  /// wire gets to "has ended" — and it is closer than `status=ended`, a value
  /// this API never writes. What it carries that does not belong here is
  /// dropped once the page has arrived.
  ///
  /// Paging is unaffected: the cursor walks the endpoint's own order, so a page
  /// that kept nothing still moves the list forward.
  bool _belongsTo(Broadcast broadcast) => switch (_query.type) {
    BroadcastsType.recentlyLive => broadcast.state.hasEnded,
    _ => true,
  };

  /// Subscribes to the events this feed's filters make relevant.
  void _bindSocketService(BroadcastSocketService? socket) {
    if (socket == null) return;

    // Every list is affected by a mutation it was not told about, whatever its
    // filters — including the unfiltered one, which is why this is bound above
    // the type switch rather than inside it.
    _homeInvalidatedSub = socket.onHomeInvalidated.listen(
      _onHomeInvalidated,
    );

    final type = _query.type;
    if (type == null) return;

    switch (type) {
      case BroadcastsType.forYou:
      case BroadcastsType.subscriptions:
      case BroadcastsType.nowLive:
        _newBroadcastSub = socket.onNewBroadcast.listen(_onNewBroadcast);
        _endedBroadcastSub = socket.onEndedBroadcast.listen(_onEndedBroadcast);

      case BroadcastsType.recentlyLive:
        _endedBroadcastSub = socket.onEndedBroadcast.listen(_onRecentlyEnded);
    }
  }

  /// Stops listening to the socket without touching the items.
  Future<void> _unbindSocketService() async {
    await _newBroadcastSub?.cancel();
    _newBroadcastSub = null;
    await _endedBroadcastSub?.cancel();
    _endedBroadcastSub = null;
    await _homeInvalidatedSub?.cancel();
    _homeInvalidatedSub = null;
  }

  /// Refetch from the first page, because something changed that this client
  /// was not told about by id.
  void _onHomeInvalidated(HomeInvalidatedDto _) {
    // A refresh in flight will read the same state; queueing another would be
    // one request per event in a burst of them.
    if (updateDataCommand.isRunning.value) return;
    updateDataCommand.run();
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

  /// Convenience: update only the keyword filter and refresh.
  void updateKeywords(String? keywords) {
    if (keywords == null || keywords.trim().isEmpty) return;
    return updateQuery(_query.updateKeywords(keywords.trim()));
  }

  @override
  void reset() {
    _cursor = null;
    super.reset();
  }

  @override
  FutureOr<dynamic> onDispose() async {
    await _unbindSocketService();

    return await super.onDispose();
  }
}
