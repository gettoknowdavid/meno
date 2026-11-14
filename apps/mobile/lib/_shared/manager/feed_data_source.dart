import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';

/// How a feed reports a failure.
///
/// [menoExceptionFilter] sends an infrastructure failure — no connectivity, a
/// timeout, a bad response — straight to the global toast, and a failure with
/// only a global handler never reaches [Command.errors]. A feed is the one
/// caller that does have somewhere to put such a failure: the error state its
/// list draws, with a retry on it. So the feed asks for a local handler first
/// and keeps the toast as the fallback, which is what happens when the feed is
/// being read by code that is not drawing one.
ErrorReaction _feedErrorFilter(Object error, StackTrace stackTrace) {
  final reaction = menoExceptionFilter(error, stackTrace);
  return reaction == .globalHandler ? .firstLocalThenGlobalHandler : reaction;
}

/// Base feed for finite data sets (non-paged)
abstract class FeedDataSource<TItem> implements Disposable {
  new({List<TItem>? initialItems}) : items = initialItems ?? [] {
    updateDataCommand = .createAsyncNoParamNoResult(() async {
      await updateFeedData();
      updateWasCalled = true;
      refreshItemCount();
    }, errorFilterFn: _feedErrorFilter);
  }

  final List<TItem> items;
  final _itemCount = ValueNotifier<int>(0);

  ValueListenable<int> get itemCount => _itemCount;
  bool updateWasCalled = false;

  late final Command<void, void> updateDataCommand;

  ValueListenable<bool> get isFetching => updateDataCommand.isRunning;

  ValueListenable<CommandError<dynamic>?> get commandErrors =>
      updateDataCommand.errors;

  /// Subclasses implement - fetch data and populate items list
  Future<void> updateFeedData();

  /// Subclasses implement - compare items for deduplication
  bool itemsAreEqual(TItem item1, TItem item2);

  TItem getItemAtIndex(int index) {
    assert(index >= 0 && index < items.length, 'Index $index out of range');
    return items[index];
  }

  void refreshItemCount() => _itemCount.value = items.length;

  void addItemAtStart(TItem item) {
    items.insert(0, item);
    refreshItemCount();
  }

  void removeObject(TItem itemToRemove) {
    items.removeWhere((item) => itemsAreEqual(item, itemToRemove));
    refreshItemCount();
  }

  void reset() {
    items.clear();
    updateWasCalled = false;
    refreshItemCount();
  }

  @override
  FutureOr<dynamic> onDispose() {
    _itemCount.dispose();
    updateDataCommand.dispose();
  }
}

/// One page of a feed, as the feed receives it.
///
/// The cursor is opaque: a feed stores what it was given and hands it back, and
/// never builds one. `null` means the page it belongs to was the last one.
final class FeedPage<TItem> {
  const new({required this.items, this.nextCursor});

  /// The page's items, in the order the endpoint sent them.
  final List<TItem> items;

  /// The cursor that names the next page, or null when there is none.
  final String? nextCursor;

  /// A page with nothing on it and nothing after it.
  static FeedPage<T> empty<T>() => FeedPage<T>(items: const []);
}

/// A next-page failure, kept apart from the first-page error state.
///
/// A refresh failure means "this list has nothing to show", and it replaces the
/// list. A page failure means "what you have is fine, but I could not read
/// more", and it belongs under the list with a retry of its own (audit M08).
final class FeedPageError {
  const new({required this.error, required this.isRetryable});

  /// The failure as the feed received it.
  final Object error;

  /// Whether asking again could succeed.
  ///
  /// A 4xx is the server refusing this request; a 5xx, a 429 or a connection
  /// that never answered is a failure that a later attempt can get past.
  final bool isRetryable;

  /// The failure as a person should read it.
  String get message => errorMessage(error);
}

/// Extends [FeedDataSource] with cursor pagination.
///
/// # The contract
///
/// One method, [fetchPage], takes the cursor of the page to read (null for the
/// first) and answers with items plus the next cursor. Everything else — which
/// page is next, when to ask, what to do when an answer arrives late — is here,
/// in one place, so a new feed cannot invent its own arithmetic. There is no
/// page number anywhere: this API paginates by a keyset cursor, and a page
/// number in a list that reorders as rows start and end names a different page
/// each time it is asked (audit I01).
///
/// # Arbitration
///
/// A refresh and a page load never run together. Whichever starts second is
/// remembered as one pending refresh, so a burst of invalidations during a
/// request costs one extra read rather than one per event (audit H11).
///
/// # Generations
///
/// Every request captures the feed's generation and checks it before writing.
/// [invalidateQuery] bumps the generation, so an answer that arrives after the
/// filters changed is dropped instead of adding another query's rows.
abstract class PagedFeedDataSource<TItem> extends FeedDataSource<TItem> {
  new({
    super.initialItems,
    this.debounceDuration,
    this.fetchMoreEnabled = true,
  }) {
    requestNextPageCommand = .createAsyncNoParamNoResult(() async {
      await _loadNextPage();
      refreshItemCount();
    }, errorFilterFn: _feedErrorFilter);

    // Merge both commands' isRunning into a single isFetching notifier.
    updateDataCommand.isRunning.addListener(_syncFetching);
    requestNextPageCommand.isRunning.addListener(_syncFetching);
  }

  final Duration? debounceDuration;
  final bool fetchMoreEnabled;

  /// How many pages one user action may read while they keep coming up empty.
  ///
  /// Bounded on purpose: a feed whose filter matches nothing must stop and ask,
  /// not walk the whole table on the UI thread (audit H12).
  static const int maxFilteredPages = 5;

  /// The failure the last next-page read ended with, until a retry clears it.
  final pageError = ValueNotifier<FeedPageError?>(null);

  String? _cursor;
  int _generation = 0;
  bool _refreshInFlight = false;
  bool _pageInFlight = false;
  bool _pendingRefresh = false;
  bool _hasMoreFiltered = false;
  Timer? _debounceTimer;

  /// The cursor that names the next page, or null when there is none.
  String? get cursor => _cursor;

  /// Whether another page exists. This is the whole of the pagination state:
  /// a cursor the server gave this feed, and nothing inferred from a count.
  bool get hasNextPage => _cursor != null;

  /// Whether a bounded walk over filtered pages stopped before it found
  /// anything this list keeps.
  ///
  /// The list draws an explicit "load more" for this, because an empty state
  /// and "there may be more, just not in the pages read so far" are different
  /// things to say to a user.
  bool get hasMoreFiltered => _hasMoreFiltered;

  /// The query generation this feed is on.
  ///
  /// Bumped by [invalidateQuery]. A read that started under an older generation
  /// is discarded rather than written.
  int get generation => _generation;

  late final Command<void, void> requestNextPageCommand;

  /// Reads the page [cursor] names; `null` asks for the first one.
  ///
  /// This is the only pagination method a feed implements. The query it reads
  /// under is the feed's own, captured when the request starts.
  Future<FeedPage<TItem>> fetchPage(String? cursor);

  /// Re-reads from the first page, or asks for one trailing refresh when a
  /// request is already in flight.
  Future<void> refresh() async {
    if (_refreshInFlight || _pageInFlight) {
      _pendingRefresh = true;
      return;
    }
    await updateDataCommand.runAsync();
  }

  /// Tries the page that failed, once, on the user's instruction.
  Future<void> retryPage() async {
    pageError.value = null;
    await requestNextPageCommand.runAsync();
  }

  /// Records that the feed's query was replaced.
  ///
  /// In-flight reads are discarded, the cursor is dropped (the first page of a
  /// new query is not the next page of the old one) and any page failure is
  /// forgotten, because it belonged to the query that is gone.
  void invalidateQuery() {
    _generation += 1;
    _hasMoreFiltered = false;
    pageError.value = null;
    reset();
  }

  @override
  Future<void> updateFeedData() async {
    // A second entry point (pull-to-refresh while a page is loading) is the
    // arbitration case: remember it and let the running read finish.
    if (_refreshInFlight || _pageInFlight) {
      _pendingRefresh = true;
      return;
    }

    final generation = _generation;
    _refreshInFlight = true;
    try {
      final page = await fetchPage(null);
      if (generation != _generation) return;

      items
        ..clear()
        ..addAll(page.items);
      _cursor = page.nextCursor;
      _hasMoreFiltered = false;
      pageError.value = null;

      // A first page its own filter rejected is not an empty feed: keep reading
      // (bounded) until something survives or the cursor runs out.
      if (items.isEmpty) await _walkUntilVisible(generation);

      flushItemCount();
    } finally {
      _refreshInFlight = false;
      _runPendingRefresh();
    }
  }

  /// Reads one more page, appending what it holds.
  ///
  /// A page that adds nothing is not the end: a filtered feed whose pages are
  /// all rejected keeps reading until it finds one, within
  /// [maxFilteredPages].
  Future<void> _loadNextPage() async {
    // A failure needs the user's retry: paging again on the next rebuild is how
    // a broken page turns into a request loop.
    if (pageError.value != null) return;
    if (_pageInFlight || _refreshInFlight || _cursor == null) return;

    final generation = _generation;
    _pageInFlight = true;
    try {
      await _walkUntilVisible(generation);
    } on Object catch (error) {
      if (generation != _generation) return;
      pageError.value = _classify(error);
    } finally {
      _pageInFlight = false;
      _runPendingRefresh();
    }
  }

  /// Reads pages until one adds something, the cursor runs out, or the budget
  /// is spent.
  Future<void> _walkUntilVisible(int generation) async {
    var pages = 0;
    var progressed = false;

    while (!progressed && _cursor != null && pages < maxFilteredPages) {
      final requested = _cursor;
      final before = items.length;

      final page = await fetchPage(requested);
      if (generation != _generation) return;

      pages += 1;
      _append(page);
      progressed = items.length > before;

      // A cursor that does not move would read the same page forever.
      if (_cursor != null && _cursor == requested) {
        _hasMoreFiltered = !progressed;
        return;
      }
    }

    _hasMoreFiltered = !progressed && _cursor != null;
  }

  /// Adds [page]'s items, skipping ones the list already holds and moving the
  /// cursor to the page's own.
  void _append(FeedPage<TItem> page) {
    for (final item in page.items) {
      if (items.any((existing) => itemsAreEqual(existing, item))) continue;
      items.add(item);
    }
    _cursor = page.nextCursor;
  }

  FeedPageError _classify(Object error) => FeedPageError(
    error: error,
    isRetryable: switch (error) {
      MenoException(:final statusCode) =>
        statusCode == null || statusCode == 429 || statusCode >= 500,
      _ => false,
    },
  );

  /// Runs the refresh that was asked for while a read was in flight, once.
  void _runPendingRefresh() {
    if (!_pendingRefresh) return;
    if (_refreshInFlight || _pageInFlight) return;
    _pendingRefresh = false;

    // Off the current call stack: the command that owns this read has not
    // finished yet, and `run()` on a running command is dropped.
    Timer.run(() {
      if (_isDisposed) return;
      updateDataCommand.run();
    });
  }

  bool _isDisposed = false;

  /// Overrides base [refreshItemCount] to apply debouncing when
  /// [debounceDuration] is set.
  ///
  /// The initial load ([updateDataCommand]) and pagination
  /// ([requestNextPageCommand]) always flush immediately — debouncing only
  /// applies to socket-driven structural mutations (add/remove).
  @override
  void refreshItemCount() {
    final duration = debounceDuration;
    if (duration == null) {
      super.refreshItemCount();
      return;
    }
    _debounceTimer?.cancel();
    _debounceTimer = Timer(duration, super.refreshItemCount);
  }

  /// Immediately flushes any pending debounced count notification.
  /// Call this after a page load completes so the list always reflects a full
  /// page without delay.
  void flushItemCount() {
    _debounceTimer?.cancel();
    _debounceTimer = null;
    super.refreshItemCount();
  }

  @override
  TItem getItemAtIndex(int index) {
    // Reading an item three from the end is what asks for the next page
    // (audit §5 item 4). A failure stops it until the user retries.
    if (fetchMoreEnabled &&
        index >= items.length - 3 &&
        hasNextPage &&
        pageError.value == null &&
        !_refreshInFlight &&
        !requestNextPageCommand.isRunning.value) {
      requestNextPageCommand.run();
    }
    return super.getItemAtIndex(index);
  }

  @override
  ValueListenable<bool> get isFetching => _isFetching;

  final _isFetching = ValueNotifier<bool>(false);

  void _syncFetching() {
    _isFetching.value =
        updateDataCommand.isRunning.value ||
        requestNextPageCommand.isRunning.value;
  }

  @override
  void reset() {
    _cursor = null;
    _hasMoreFiltered = false;
    super.reset();
  }

  @override
  FutureOr<dynamic> onDispose() {
    _isDisposed = true;
    updateDataCommand.isRunning.removeListener(_syncFetching);
    requestNextPageCommand.isRunning.removeListener(_syncFetching);
    requestNextPageCommand.dispose();
    _isFetching.dispose();
    pageError.dispose();
    _debounceTimer?.cancel();
    _debounceTimer = null;
    return super.onDispose();
  }
}
