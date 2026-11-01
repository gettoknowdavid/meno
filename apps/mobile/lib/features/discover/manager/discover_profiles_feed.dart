import 'package:meno_mobile/_shared/manager/feed_data_source.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/services/_services.dart';

/// The accounts one keyword search found, read a page at a time.
///
/// # What it reads
///
/// `GET /users?q=` — the API's only keyword search. There is no broadcast
/// search: `GET /broadcasts` filters by creator, status and order, and has no
/// term at all, so a search screen that claimed to find broadcasts would be
/// showing rows that did not match what was typed (audit, Discover). Live and
/// recently-live browsing stays on the broadcast feeds.
///
/// # Why a feed rather than a page list
///
/// The endpoint answers with a keyset cursor, and [PagedFeedDataSource] is the
/// one place this app turns that into a list: it drops an answer that belongs
/// to a query which has since been replaced, collapses a second refresh into
/// the one already in flight, and never reads past a page that failed. A
/// search that re-implemented any of that would get one of them wrong — the
/// old screen kept a page number, which this endpoint does not have.
final class DiscoverProfilesFeed extends PagedFeedDataSource<Profile> {
  /// Creates a feed over [_http], reading [_keywords].
  ///
  /// The term is part of the feed so that a page read in flight and the term it
  /// was read for cannot disagree: [search] replaces both together.
  new({
    required this._http,
    required this._keywords,
    super.fetchMoreEnabled = true,
  });

  final ProfileHttpService _http;

  String _keywords;

  /// The term this feed is currently built from.
  String get keywords => _keywords;

  @override
  bool itemsAreEqual(Profile a, Profile b) => a.id == b.id;

  @override
  Future<FeedPage<Profile>> fetchPage(String? cursor) async {
    final page = await _http.search(_keywords, cursor: cursor);

    return FeedPage(
      items: page.items,
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
