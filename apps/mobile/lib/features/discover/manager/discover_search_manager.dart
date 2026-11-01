import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/discover/manager/discover_broadcasts_feed.dart';
import 'package:meno_mobile/features/discover/services/_services.dart';

/// The broadcast search behind Discover: what was typed, what it found, and
/// what was searched for before.
///
/// # What it searches
///
/// Broadcasts, through `GET /broadcasts?q=` — matched against the title and
/// description text the API indexes, so typing a title that is on screen in the
/// browsing tabs finds that broadcast. Drafts and broadcasts that have never
/// been on air are not offered by the API in a keyword list, whatever this
/// account may own.
///
/// # Why it lives in the account scope
///
/// What somebody searched for is theirs, and the history is stored under their
/// id. The manager is built when the account becomes active and disposed with
/// it, so a search typed under one account can never be read — or written —
/// under the next (audit M05). It is not a per-page manager: leaving the search
/// screen and coming back to it keeps the term and its result while the account
/// lasts, which is what a search screen is expected to do.
///
/// # Paging
///
/// Reading more is the feed's [DiscoverBroadcastsFeed.requestNextPageCommand],
/// exposed rather than wrapped: a second command around it would have its own
/// `isRunning`, and the list would have to watch two.
final class DiscoverSearchManager implements Disposable {
  /// Creates the manager over its search service and this account's history.
  new({required BroadcastHttpService http, required this._local}) {
    _feed = DiscoverBroadcastsFeed(http: http, keywords: '');

    search = .createAsyncNoResult<String>(
      _search,
      errorFilterFn: menoExceptionFilter,
      debugName: 'discover.search',
    );

    removeRecentSearch = .createAsyncNoResult<String>(
      (term) async {
        await _local.removeRecentSearch(term);
        if (_disposed) return;
        _adoptRecentSearches();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'discover.removeRecentSearch',
    );

    clearRecentSearches = .createAsyncNoParamNoResult(
      () async {
        await _local.clearRecentSearches();
        if (_disposed) return;
        _adoptRecentSearches();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'discover.clearRecentSearches',
    );

    fetchMore = _feed.requestNextPageCommand;

    // The footer says whether another page exists, and that answer only becomes
    // true or false once a read has finished — which is exactly when the feed
    // stops fetching.
    _fetching = _feed.isFetching.listen((isFetching, _) {
      if (!isFetching) _adoptPagingState();
    });

    // The item count moves when a page lands, which is the moment the cursor
    // that answers "is there more" is already set — before the command that
    // read it reports itself finished. Without this the footer would keep
    // offering "load more" for a page the list has already drawn.
    _items = _feed.itemCount.listen((_, _) => _adoptPagingState());

    // The history is this account's, and it is adopted when the manager is
    // built rather than when a screen opens: a screen that has never been
    // opened has no reason to be the thing that starts listening.
    //
    // One notifier covers every key on the device, so a write to something else
    // is a notification to re-read this account's own key and nothing more.
    _storageChanges = _local.changes.listen((_) => _adoptRecentSearches());
    _adoptRecentSearches();
  }

  final DiscoverLocalService _local;

  late final DiscoverBroadcastsFeed _feed;

  /// The shortest term worth asking the API for.
  ///
  /// A keyword list needs something to match, and a two-letter term matches
  /// most of a catalogue. The API itself accepts any non-blank `q`; the floor
  /// is the client's, so a half-typed word is not an error on screen.
  static const int minSearchLength = 3;

  /// The term as the screen holds it — the search bar's text and what the body
  /// decides to draw is read from this, not from the last submitted term.
  final query = ValueNotifier<String>('');

  /// The terms searched before, most recent first.
  final recentSearches = ValueNotifier<List<String>>(const []);

  /// Whether another page of results exists.
  final hasMore = ValueNotifier<bool>(false);

  late final Command<String, void> search;
  late final Command<String, void> removeRecentSearch;
  late final Command<void, void> clearRecentSearches;

  /// Reads the next page of results.
  ///
  /// The feed's own command: the list asks for it when it draws an item within
  /// three of the end, and the scroll listener asks for it directly.
  late final Command<void, void> fetchMore;

  ListenableSubscription? _storageChanges;
  ListenableSubscription? _fetching;
  ListenableSubscription? _items;

  bool _disposed = false;

  /// The feed drawing the results — its item count, items and page failure.
  PagedFeedDataSource<Broadcast> get feed => _feed;

  /// Whether the first page of a term is on its way.
  ValueListenable<bool> get isSearching => search.isRunning;

  /// Tries the results page that failed, on the user's instruction.
  Future<void> retryPage() => _feed.retryPage();

  @override
  FutureOr<dynamic> onDispose() async {
    // Disposal is asked for twice when a scope is torn down while a screen that
    // registered itself for the same manager is on its way out; the second ask
    // finds nothing left to do rather than disposing a command again.
    if (_disposed) return;
    _disposed = true;
    _storageChanges?.cancel();
    _fetching?.cancel();
    _items?.cancel();

    search.dispose();
    removeRecentSearch.dispose();
    clearRecentSearches.dispose();

    // Disposes [fetchMore] with it: the manager exposes the feed's command
    // rather than wrapping it, and one command has one owner.
    await _feed.onDispose();

    query.dispose();
    recentSearches.dispose();
    hasMore.dispose();
  }

  /// Searches [term], or empties the results when it is too short to ask for.
  ///
  /// A term under [minSearchLength] matches too much to be worth a round trip,
  /// so none is sent. The list is cleared instead, and the screen says it found
  /// nothing — which is the truth about a term that searched for nothing.
  Future<void> _search(String term) async {
    final keywords = term.trim();
    query.value = keywords;

    if (keywords.length < minSearchLength) {
      _feed.clear();
      hasMore.value = false;
      return;
    }

    await _feed.search(keywords);
    if (_disposed) return;

    _adoptPagingState();

    // Only a search that found something is worth remembering: a history full
    // of terms that matched nothing is a list of dead ends.
    if (_feed.items.isEmpty) return;
    await _local.addRecentSearch(keywords);
    if (_disposed) return;
    _adoptRecentSearches();
  }

  /// Reads the history from the device.
  ///
  /// Synchronous: preferences keep an in-memory mirror, so this cannot fail and
  /// cannot produce a frame with no history in it.
  void _adoptRecentSearches() {
    if (_disposed) return;
    recentSearches.value = _local.recentSearches;
  }

  void _adoptPagingState() {
    if (_disposed) return;
    hasMore.value = _feed.hasNextPage;
  }
}
