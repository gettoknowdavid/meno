import 'dart:async';

import 'package:meno_mobile/_shared/manager/feed_data_source.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

/// One broadcast list, read from the API and kept current by the socket.
///
/// # Cursors, not page numbers
///
/// `/broadcasts` answers with an opaque cursor and says whether another page
/// exists. [fetchPage] hands that cursor back and moves to the next one, and
/// [hasNextPage] is nothing more than "the server gave us a cursor" — the base
/// class's pagination is the same contract every feed uses, so a new list
/// cannot invent its own page arithmetic.
///
/// # What the socket does to the list
///
/// | Feed          | A broadcast starts     | A broadcast ends          |
/// |---------------|------------------------|---------------------------|
/// | now live      | prepended              | removed                   |
/// | for you       | prepended              | removed                   |
/// | recently live | —                      | refetched (real summary)  |
/// | anything else | —                      | —                         |
///
/// A socket is optional: a feed built without one is purely HTTP-backed, which
/// is what a profile's or a search result's list is.
///
/// # Filters are part of the feed, not decoration
///
/// An event is only applied when it belongs in this list: a creator's feed must
/// not acquire another creator's broadcast just because one started (audit
/// M09). When the event does not carry enough to decide, the feed refetches
/// once instead of guessing.
///
/// # What `homeInvalidated` does to it
///
/// Prepend and remove cover the broadcasts this client was told about by id.
/// `homeInvalidated` is the other half: a mutation this client was not told
/// about — a broadcast somebody else took live, an edit, a soft delete —
/// leaves the list quietly stale. Every feed refetches its first page on that
/// frame, and the base class collapses a burst into the refresh already in
/// flight rather than queueing one per frame.
class BroadcastFeedDataSource extends PagedFeedDataSource<Broadcast> {
  new({
    required this._http,
    required this._query,
    this.socket,
    super.fetchMoreEnabled = true,
  }) {
    unawaited(_rebind());
  }

  final BroadcastHttpService _http;

  /// The socket the feed listens on, kept so [updateQuery] can hand the new
  /// filters to it.
  final BroadcastSocketService? socket;

  BroadcastQuery _query;

  /// The subscriptions this feed currently owns.
  ///
  /// Replaced as a whole on every rebind: detaching into a local first is what
  /// lets the old handles be cancelled without cancelling the new ones.
  List<StreamSubscription<void>> _handles = const [];

  /// Whether a refetch for a burst of frames is already waiting to run.
  bool _refreshQueued = false;

  /// The filters the feed is currently built from.
  BroadcastQuery get query => _query;

  @override
  bool itemsAreEqual(Broadcast a, Broadcast b) => a.id == b.id;

  /// Reads [cursor]'s page — `null` asks for the first — keeping only what this
  /// list's filters accept.
  ///
  /// `recentlyLive` is the one section the endpoint cannot express on its own:
  /// ending a broadcast sets `status = inactive` and `end_time`, and a draft or
  /// a scheduled one is `inactive` too, so `status=inactive` is the closest the
  /// wire gets to "has ended". What it carries that does not belong here is
  /// dropped here. A page that keeps nothing is not the end of the list: the
  /// base class keeps walking, within its page budget, until something does,
  /// which is what stops an empty filtered page from stranding pagination
  /// (audit H12).
  @override
  Future<FeedPage<Broadcast>> fetchPage(String? cursor) async {
    final page = await _http.list(_query.withCursor(cursor));

    return FeedPage(
      items: [
        for (final summary in page.items)
          if (_matches(summary.toBroadcast())) summary.toBroadcast(),
      ],
      // The server states whether another page exists; a cursor with no page
      // behind it would make the list ask once more for nothing.
      nextCursor: page.hasNextPage ? page.nextCursor : null,
    );
  }

  /// Replaces the filters and refreshes from the first page.
  ///
  /// The socket subscriptions are rebuilt with them, because which events
  /// belong in a list is a property of its filters: a feed switched from "now
  /// live" to "recently live" that kept its old subscriptions would file a
  /// broadcast that just started into a list about ones that ended.
  void updateQuery(BroadcastQuery newQuery) {
    _query = newQuery;

    // Bumps the generation, so a page or refresh that was already in flight
    // lands nowhere instead of adding the old query's rows.
    invalidateQuery();

    unawaited(_rebind());
    unawaited(refresh());
  }

  /// Convenience: update only the keyword filter and refresh.
  void updateKeywords(String? keywords) {
    if (keywords == null || keywords.trim().isEmpty) return;
    updateQuery(_query.updateKeywords(keywords.trim()));
  }

  /// Whether [broadcast] belongs in this list.
  bool _matches(Broadcast broadcast) {
    final status = _query.status;
    if (status != null && broadcast.status != status) return false;

    final creatorId = _query.creatorId;
    if (creatorId != null && broadcast.creator?.id.getOrNull != creatorId) {
      return false;
    }

    return switch (_query.type) {
      BroadcastsType.recentlyLive => broadcast.state.hasEnded,
      _ => true,
    };
  }

  /// Whether [broadcast] says who created it — the one field [_matches] cannot
  /// decide without.
  bool _namesCreator(Broadcast broadcast) =>
      broadcast.creator?.id.isValid ?? false;

  /// Replaces the subscriptions, detaching the old set before cancelling it.
  ///
  /// The swap happens synchronously, so a second rebind can never cancel the
  /// handles it just built; the cancels follow and only reach the set that was
  /// already dropped.
  Future<void> _rebind() async {
    final old = _handles;
    _handles = const [];

    final service = socket;
    if (service != null) {
      _handles = [
        service.onHomeInvalidated.listen((_) => _refreshOnce()),
        ..._handlesFor(service, _query.type),
      ];
    }

    for (final handle in old) {
      await handle.cancel();
    }
  }

  /// The event subscriptions one section needs, if any.
  ///
  /// A feed with no section — a creator's list, a search result — still
  /// listens: a broadcast that starts is news for it too, and whether it
  /// belongs is what the predicates in [_onNewBroadcast] decide. Only
  /// `recentlyLive` wants a different answer, because a broadcast that starts
  /// is not one that ended.
  List<StreamSubscription<void>> _handlesFor(
    BroadcastSocketService service,
    BroadcastsType? type,
  ) => switch (type) {
    BroadcastsType.forYou ||
    BroadcastsType.subscriptions ||
    BroadcastsType.nowLive => [
      service.onNewBroadcast.listen(_onNewBroadcast),
      service.onEndedBroadcast.listen(_onEndedBroadcast),
    ],

    BroadcastsType.recentlyLive => [
      // The signal carries an id and a reason and nothing else, so there is no
      // card to build from it — doing that once produced rows with an empty
      // title and no host (audit §3 M10). The ended broadcast reaches this list
      // by refetching, which is also where its real summary comes from.
      service.onEndedBroadcast.listen((_) => _refreshOnce()),
    ],

    null => [
      service.onNewBroadcast.listen(_onNewBroadcast),
      service.onEndedBroadcast.listen(_onEndedBroadcast),
    ],
  };

  /// Refetches once for a burst of frames.
  ///
  /// The deferral is deliberate: a socket burst delivers several events in one
  /// turn, and reading per event would be several requests for one change. The
  /// base class then collapses this with any refresh already in flight.
  void _refreshOnce() {
    if (_refreshQueued) return;
    _refreshQueued = true;

    Timer.run(() {
      _refreshQueued = false;
      if (_isDisposed) return;
      unawaited(refresh());
    });
  }

  bool _isDisposed = false;

  void _onNewBroadcast(Broadcast broadcast) {
    if (items.any((existing) => itemsAreEqual(existing, broadcast))) return;

    // A frame that does not say who created it cannot be placed in a
    // creator-filtered list. One refetch decides, rather than a wrong insert.
    if (_query.creatorId != null && !_namesCreator(broadcast)) {
      _refreshOnce();
      return;
    }
    if (!_matches(broadcast)) return;

    addItemAtStart(broadcast);
  }

  void _onEndedBroadcast(EndedSignal ended) {
    if (_query.type == BroadcastsType.recentlyLive) {
      _refreshOnce();
      return;
    }

    final index = items.indexWhere((item) => item.id == ended.broadcastId);
    if (index == -1) return;

    removeObject(items[index]);
  }

  @override
  FutureOr<dynamic> onDispose() async {
    _isDisposed = true;

    final old = _handles;
    _handles = const [];
    for (final handle in old) {
      await handle.cancel();
    }

    await super.onDispose();
  }
}
