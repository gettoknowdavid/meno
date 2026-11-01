import 'package:meno_mobile/_shared/manager/feed_data_source.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

/// The broadcasts one keyword search found, read a page at a time.
///
/// # What it reads
///
/// `GET /broadcasts?q=` — the endpoint's keyword filter, matched against the
/// title and description text its search index covers. The same endpoint draws
/// the browsing tabs, so a result card and a tab card are the same row on the
/// wire and open the same way.
///
/// A search is a public read: the API leaves drafts and broadcasts that have
/// never been on air out of a keyword list, so this feed does not have to know
/// which rows are somebody's unannounced work.
///
/// # Why a feed rather than a page list
///
/// The endpoint answers with a keyset cursor, and [PagedFeedDataSource] is the
/// one place this app turns that into a list: it drops an answer that belongs
/// to a query which has since been replaced, collapses a second refresh into
/// the one already in flight, and never reads past a page that failed. A
/// search that re-implemented any of that would get one of them wrong — the
/// old screen kept a page number, which this endpoint does not have.
final class DiscoverBroadcastsFeed extends PagedFeedDataSource<Broadcast> {
  /// Creates a feed over [_http], reading [_keywords].
  ///
  /// The term is part of the feed so that a page read in flight and the term it
  /// was read for cannot disagree: [search] replaces both together.
  new({
    required this._http,
    required this._keywords,
    super.fetchMoreEnabled = true,
  });

  final BroadcastHttpService _http;

  String _keywords;

  /// The term this feed is currently built from.
  String get keywords => _keywords;

  @override
  bool itemsAreEqual(Broadcast a, Broadcast b) => a.id == b.id;

  @override
  Future<FeedPage<Broadcast>> fetchPage(String? cursor) async {
    // No status and no section: a search is not one tab of the catalogue but a
    // term over all of it, and the API decides which rows are public.
    final page = await _http.list(
      BroadcastQuery(keywords: _keywords).withCursor(cursor),
    );

    return FeedPage(
      items: [for (final summary in page.items) summary.toBroadcast()],
      // A cursor the server did not pair with another page would make the list
      // ask once more for nothing.
      nextCursor: page.hasNextPage ? page.nextCursor : null,
    );
  }

  /// Replaces the term and reads its first page.
  ///
  /// Bumping the generation first is what stops the answer to the previous term
  /// from landing in this list: a slow response to "ada" must not paint over
  /// "grace".
  Future<void> search(String keywords) async {
    _keywords = keywords;
    invalidateQuery();
    await refresh();
  }

  /// Empties the list without reading anything.
  ///
  /// What a term too short for the API leaves behind: there is no request worth
  /// sending, and the screen should not keep the previous term's rows.
  void clear() {
    invalidateQuery();
    flushItemCount();
  }
}
