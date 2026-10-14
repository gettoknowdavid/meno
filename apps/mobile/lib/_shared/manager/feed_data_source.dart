import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';

/// Base feed for finite data sets (non-paged)
abstract class FeedDataSource<TItem> implements Disposable {
  new({List<TItem>? initialItems}) : items = initialItems ?? [] {
    updateDataCommand = .createAsyncNoParamNoResult(() async {
      await updateFeedData();
      updateWasCalled = true;
      refreshItemCount();
    }, errorFilterFn: menoExceptionFilter);
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

/// Extends FeedDataSource with pagination support
abstract class PagedFeedDataSource<TItem> extends FeedDataSource<TItem> {
  new({
    super.initialItems,
    this.debounceDuration,
    this.fetchMoreEnabled = true,
  }) {
    requestNextPageCommand = .createAsyncNoParamNoResult(() async {
      await requestNextPage();
      refreshItemCount();
    }, errorFilterFn: menoExceptionFilter);

    // Merge both commands' isRunning into a single isFetching notifier.
    updateDataCommand.isRunning.addListener(_syncFetching);
    requestNextPageCommand.isRunning.addListener(_syncFetching);
  }

  final Duration? debounceDuration;
  final bool fetchMoreEnabled;

  int _currentPage = 1;
  int _totalPages = 1;
  Timer? _debounceTimer;

  bool get hasNextPage => _currentPage < _totalPages;

  int get nextPageIndex => _currentPage + 1;

  late final Command<void, void> requestNextPageCommand;

  /// Subclasses implement - fetch next page and append to items
  Future<void> requestNextPage();

  /// Update pagination state from response
  void updatePaginationState({
    required int currentPage,
    required int totalPages,
  }) {
    _currentPage = currentPage;
    _totalPages = totalPages;
  }

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
  /// Call this after [updateFeedData] and [requestNextPage] complete so the
  /// list always reflects a full page load without delay.
  void flushItemCount() {
    _debounceTimer?.cancel();
    _debounceTimer = null;
    super.refreshItemCount();
  }

  @override
  TItem getItemAtIndex(int index) {
    if (fetchMoreEnabled &&
        index >= items.length - 3 &&
        commandErrors.value == null &&
        hasNextPage &&
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
    _currentPage = 1;
    _totalPages = 1;
    super.reset();
  }

  @override
  FutureOr<dynamic> onDispose() {
    updateDataCommand.isRunning.removeListener(_syncFetching);
    requestNextPageCommand.isRunning.removeListener(_syncFetching);
    requestNextPageCommand.dispose();
    _isFetching.dispose();
    _debounceTimer?.cancel();
    _debounceTimer = null;
    return super.onDispose();
  }
}
