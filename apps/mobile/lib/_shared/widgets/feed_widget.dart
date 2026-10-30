import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/manager/feed_data_source.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

/// How a [FeedWidget] arranges its items.
enum FeedLayout {
  /// Vertical [ListView] — the default. Good for profile broadcast tiles.
  verticalList,

  /// Horizontal single-row [ListView] — good for home page preview strips.
  horizontalList,

  /// Vertical [GridView] with a fixed cross-axis count.
  verticalGrid,

  /// Horizontal [GridView] with a fixed main-axis count.
  horizontalGrid,
}

/// One list, drawn from a [PagedFeedDataSource].
///
/// The widget owns the states a feed passes through, so every caller does not:
/// the first load draws [skeletonItemCount] copies of [skeletonItem] wrapped in
/// a `Skeletonizer`, an empty feed draws [emptyWidget], a failed first page
/// draws an error state with a retry, and a feed with items draws them with
/// pull-to-refresh in the vertical layouts and a pagination footer under them.
///
/// Paging is not done here. Reading an item through
/// [PagedFeedDataSource.getItemAtIndex] is what asks for the next page — the
/// data source triggers it once the list is within three of the end — so this
/// widget builds `itemCount + 1` rows and never inspects scroll position.
///
/// Example usage:
/// ```dart
/// FeedWidget<Broadcast>(
///   feedSource: feedSource,
///   itemBuilder: (context, broadcast) => BroadcastCard(broadcast: broadcast),
/// )
/// ```
class FeedWidget<TItem> extends WatchingWidget {
  /// Creates a feed over [feedSource], drawing each item with [itemBuilder].
  ///
  /// [layout] picks how those items are arranged; the grid parameters apply to
  /// the grid layouts, and [horizontalItemExtent] to the horizontal list
  /// alone. [shrinkWrap], [physics] and [controller] are handed to the
  /// scrollable, so the widget can be embedded in another one.
  const new({
    required this.feedSource,
    required this.itemBuilder,
    this.layout = .verticalList,
    this.emptyWidget = const MEmptyState(),
    this.gridCrossAxisCount = 2,
    this.gridChildAspectRatio = 1.0,
    this.gridMainAxisSpacing = 12.0,
    this.gridCrossAxisSpacing = 12.0,
    this.horizontalItemExtent,
    this.padding,
    this.shrinkWrap = false,
    this.physics,
    this.skeletonItem = const SizedBox(width: 148, height: 200),
    this.skeletonItemCount = 8,
    this.controller,
    this.topSlivers,
    super.key,
  });

  /// The feed to draw. The widget listens to its notifiers and commands.
  final PagedFeedDataSource<TItem> feedSource;

  /// Builds each item. Fetching and paging stay the data source's business.
  final Widget Function(BuildContext context, TItem item) itemBuilder;

  /// How the items are arranged.
  final FeedLayout layout;

  /// Grid-only: number of columns.
  final int gridCrossAxisCount;

  /// Grid-only: child aspect ratio.
  final double gridChildAspectRatio;

  /// Grid-only: vertical spacing between children.
  final double gridMainAxisSpacing;

  /// Grid-only: horizontal spacing between children.
  final double gridCrossAxisSpacing;

  /// [FeedLayout.horizontalList] only: fixed width per item. Items size
  /// themselves when this is null.
  final double? horizontalItemExtent;

  /// Padding around the list or grid.
  final EdgeInsetsGeometry? padding;

  /// Pass `true` when embedding the feed in another scrollable, such as a
  /// [Column] inside a [SingleChildScrollView].
  final bool shrinkWrap;

  /// Overrides the scroll physics. The platform default is used when null.
  final ScrollPhysics? physics;

  /// Shown once the first page has answered with nothing.
  final Widget emptyWidget;

  /// The shape drawn in place of each item during the first load.
  final Widget skeletonItem;

  /// How many [skeletonItem]s the first load draws.
  final int skeletonItemCount;

  final ScrollController? controller;

  /// Optional slivers placed above the items. Vertical layouts only.
  final List<Widget>? topSlivers;

  @override
  Widget build(BuildContext context) {
    // The first page is fetched once, as a side effect of being built: a
    // rebuild driven by a watched notifier must not re-request it. `refresh`
    // rather than the command's `run`, so a request that is already in flight
    // is remembered instead of dropped. Its failure is drawn below and
    // announced by the handler after this, so the error is already read;
    // ignoring it keeps an unawaited request from becoming an unhandled one.
    callOnce((_) => feedSource.refresh().ignore());

    // Failures are announced where they happen as well as drawn as a state, so
    // a paging failure on a list that already has items is still visible.
    registerHandler(
      target: feedSource,
      select: (source) => source.commandErrors,
      handler: (context, error, _) {
        if (error?.error == null) return;
        context.showErrorToast(errorMessage(error?.error));
      },
    );

    final itemCount = watch(feedSource.itemCount).value;
    final isFetching = watch(feedSource.isFetching).value;
    // Watched, not read: a page failure is drawn in the footer, and a failure
    // that does not rebuild the list would never be shown at all.
    final pageError = watch(feedSource.pageError).value;

    // Nothing has been asked for yet, so the skeletons are what "loading" means
    // here — as opposed to a refresh of a list that is already on screen.
    final isInitialLoading = !feedSource.updateWasCalled && isFetching;

    final error = feedSource.commandErrors.value;
    if (error != null && itemCount == 0) {
      return MErrorState(
        message: errorMessage(error.error),
        onRetry: () => feedSource.refresh().ignore(),
      );
    }

    if (itemCount == 0 && feedSource.updateWasCalled) {
      // A feed whose filter rejected every page it read is not empty — it has
      // more, and asking for it is the user's move rather than a scroll that
      // can never happen on an empty list.
      if (feedSource.hasMoreFiltered) {
        return _LoadMorePrompt(
          isLoading: isFetching,
          onLoadMore: feedSource.requestNextPageCommand.runAsync,
        );
      }
      return emptyWidget;
    }

    final effectiveItemCount = isInitialLoading ? skeletonItemCount : itemCount;

    return switch (layout) {
      FeedLayout.horizontalList => _HorizontalList(
        feedSource: feedSource,
        itemBuilder: itemBuilder,
        skeletonItem: skeletonItem,
        itemCount: effectiveItemCount,
        isInitialLoading: isInitialLoading,
        itemExtent: horizontalItemExtent,
        padding: padding,
        shrinkWrap: shrinkWrap,
        physics: physics,
        controller: controller,
      ),
      FeedLayout.verticalList => _VerticalList(
        feedSource: feedSource,
        skeletonItem: skeletonItem,
        itemBuilder: itemBuilder,
        itemCount: effectiveItemCount,
        isInitialLoading: isInitialLoading,
        isFetching: isFetching,
        pageError: pageError,
        padding: padding,
        shrinkWrap: shrinkWrap,
        physics: physics,
        controller: controller,
        topSlivers: topSlivers,
      ),
      FeedLayout.verticalGrid => _GridList(
        feedSource: feedSource,
        skeletonItem: skeletonItem,
        itemBuilder: itemBuilder,
        itemCount: effectiveItemCount,
        isFetching: isFetching,
        pageError: pageError,
        crossAxisCount: gridCrossAxisCount,
        childAspectRatio: gridChildAspectRatio,
        mainAxisSpacing: gridMainAxisSpacing,
        crossAxisSpacing: gridCrossAxisSpacing,
        isInitialLoading: isInitialLoading,
        padding: padding,
        shrinkWrap: shrinkWrap,
        physics: physics,
        scrollDirection: .vertical,
        controller: controller,
        topSlivers: topSlivers,
      ),
      FeedLayout.horizontalGrid => _GridList(
        feedSource: feedSource,
        skeletonItem: skeletonItem,
        itemBuilder: itemBuilder,
        itemCount: effectiveItemCount,
        isFetching: isFetching,
        pageError: pageError,
        crossAxisCount: gridCrossAxisCount,
        childAspectRatio: gridChildAspectRatio,
        mainAxisSpacing: gridMainAxisSpacing,
        crossAxisSpacing: gridCrossAxisSpacing,
        isInitialLoading: isInitialLoading,
        padding: padding,
        shrinkWrap: shrinkWrap,
        physics: physics,
        scrollDirection: .horizontal,
        controller: controller,
        topSlivers: topSlivers,
      ),
    };
  }
}

/// Single-row horizontal [ListView]. No pull-to-refresh, no pagination footer.
class _HorizontalList<TItem> extends StatelessWidget {
  const new({
    required this.feedSource,
    required this.itemBuilder,
    required this.itemCount,
    required this.isInitialLoading,
    required this.skeletonItem,
    this.itemExtent,
    this.padding,
    this.shrinkWrap = false,
    this.physics,
    this.controller,
  });

  final PagedFeedDataSource<TItem> feedSource;
  final Widget Function(BuildContext, TItem) itemBuilder;
  final int itemCount;
  final double? itemExtent;
  final EdgeInsetsGeometry? padding;
  final bool shrinkWrap;
  final ScrollPhysics? physics;
  final bool isInitialLoading;
  final Widget skeletonItem;
  final ScrollController? controller;

  @override
  Widget build(BuildContext context) {
    return ListView.separated(
      scrollDirection: .horizontal,
      clipBehavior: .none,
      padding: padding ?? const .symmetric(horizontal: Insets.lg),
      shrinkWrap: shrinkWrap,
      physics: physics,
      controller: controller,
      separatorBuilder: (_, _) => const SizedBox(width: 24),
      itemCount: itemCount,
      itemBuilder: (context, index) {
        if (isInitialLoading) return Skeletonizer(child: skeletonItem);
        final item = feedSource.getItemAtIndex(index);
        return itemBuilder(context, item);
      },
    );
  }
}

/// Vertical scrolling [ListView] with pull-to-refresh and pagination footer.
class _VerticalList<TItem> extends StatelessWidget {
  const new({
    required this.feedSource,
    required this.itemBuilder,
    required this.itemCount,
    required this.isFetching,
    required this.pageError,
    required this.isInitialLoading,
    required this.skeletonItem,
    this.padding,
    this.shrinkWrap = false,
    this.physics,
    this.controller,
    this.topSlivers,
  });

  final PagedFeedDataSource<TItem> feedSource;
  final Widget Function(BuildContext, TItem) itemBuilder;
  final int itemCount;
  final bool isFetching;
  final FeedPageError? pageError;
  final EdgeInsetsGeometry? padding;
  final bool shrinkWrap;
  final ScrollPhysics? physics;
  final bool isInitialLoading;
  final Widget skeletonItem;
  final ScrollController? controller;
  final List<Widget>? topSlivers;

  @override
  Widget build(BuildContext context) {
    return RefreshIndicator(
      onRefresh: feedSource.refresh,
      child: CustomScrollView(
        shrinkWrap: shrinkWrap,
        physics: physics,
        controller: controller,
        slivers: [
          ...?topSlivers,
          SliverPadding(
            padding: padding ?? EdgeInsets.zero,
            sliver: SliverList.separated(
              itemCount: itemCount,
              separatorBuilder: (_, _) => Spaces.verticalLarge,
              itemBuilder: (context, index) {
                if (isInitialLoading) return Skeletonizer(child: skeletonItem);
                final item = feedSource.getItemAtIndex(index);
                return itemBuilder(context, item);
              },
            ),
          ),
          if (!isInitialLoading)
            SliverToBoxAdapter(
              child: _FeedFooter(
                isLoading: isFetching,
                hasMore: feedSource.hasNextPage,
                pageError: pageError,
                onRetry: feedSource.retryPage,
              ),
            ),
        ],
      ),
    );
  }
}

/// Two-column (or N-column) [GridView] with a pagination footer.
class _GridList<TItem> extends StatelessWidget {
  const new({
    required this.feedSource,
    required this.itemBuilder,
    required this.itemCount,
    required this.isFetching,
    required this.pageError,
    required this.crossAxisCount,
    required this.childAspectRatio,
    required this.mainAxisSpacing,
    required this.crossAxisSpacing,
    required this.isInitialLoading,
    required this.scrollDirection,
    required this.skeletonItem,
    this.padding,
    this.shrinkWrap = false,
    this.physics,
    this.controller,
    this.topSlivers,
  });

  final PagedFeedDataSource<TItem> feedSource;
  final Widget Function(BuildContext, TItem) itemBuilder;
  final int itemCount;
  final bool isFetching;
  final FeedPageError? pageError;
  final int crossAxisCount;
  final double childAspectRatio;
  final double mainAxisSpacing;
  final double crossAxisSpacing;
  final EdgeInsetsGeometry? padding;
  final bool shrinkWrap;
  final ScrollPhysics? physics;
  final bool isInitialLoading;
  final Axis scrollDirection;
  final Widget skeletonItem;
  final ScrollController? controller;
  final List<Widget>? topSlivers;

  @override
  Widget build(BuildContext context) {
    return RefreshIndicator(
      onRefresh: feedSource.refresh,
      child: CustomScrollView(
        controller: controller,
        scrollDirection: scrollDirection,
        shrinkWrap: shrinkWrap,
        physics: physics,
        slivers: [
          ...?topSlivers,
          SliverPadding(
            padding: padding ?? EdgeInsets.zero,
            sliver: SliverGrid(
              gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                crossAxisCount: crossAxisCount,
                childAspectRatio: childAspectRatio,
                mainAxisSpacing: mainAxisSpacing,
                crossAxisSpacing: crossAxisSpacing,
              ),
              delegate: SliverChildBuilderDelegate((context, index) {
                if (isInitialLoading) return Skeletonizer(child: skeletonItem);
                final item = feedSource.getItemAtIndex(index);
                return itemBuilder(context, item);
              }, childCount: itemCount),
            ),
          ),
          if (!isInitialLoading)
            SliverToBoxAdapter(
              child: _FeedFooter(
                isLoading: isFetching,
                hasMore: feedSource.hasNextPage,
                pageError: pageError,
                onRetry: feedSource.retryPage,
              ),
            ),
        ],
      ),
    );
  }
}

/// The row under a list: a failed page with its own retry, the progress row, or
/// the end of the list.
///
/// A page failure is not a refresh failure. The list is still on screen and
/// still correct; what failed is reading the next page, so the answer belongs
/// under the items with one retry — not as a full-screen error that would throw
/// away what the user is reading (audit M08).
class _FeedFooter extends StatelessWidget {
  const new({
    required this.isLoading,
    required this.hasMore,
    required this.pageError,
    required this.onRetry,
  });

  final bool isLoading;
  final bool hasMore;
  final FeedPageError? pageError;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final error = pageError;
    if (error != null) {
      final colors = MColorScheme.of(context);
      final textTheme = MTextTheme.of(context);

      return Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Insets.lg,
          vertical: Insets.lg,
        ),
        child: Column(
          children: [
            MText(
              error.message,
              style: textTheme.captionRegular,
              color: colors.onBackgroundVariant,
              textAlign: TextAlign.center,
            ),
            Spaces.verticalSmall,
            // Only a failure that could pass on a second attempt offers the
            // button: retrying a refusal would look like a broken control.
            if (error.isRetryable)
              MTextButton(label: 'Retry', onPressed: onRetry)
            else
              const SizedBox.shrink(),
          ],
        ),
      );
    }

    return MPagedLoadingIndicator(isLoading: isLoading, hasMore: hasMore);
  }
}

/// What an empty list shows when its filter rejected every page it read.
///
/// The scroll-triggered prefetch cannot fire on a list with no items, so the
/// only way to reach the rows behind those pages is to ask — this is that ask.
/// It says "load more" rather than "empty", because the two are different
/// facts and hiding the difference is what stranded the older rows (audit H12).
class _LoadMorePrompt extends StatelessWidget {
  const new({required this.isLoading, required this.onLoadMore});

  final bool isLoading;
  final VoidCallback onLoadMore;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Center(
      child: Padding(
        padding: const EdgeInsets.all(Insets.lg),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            MText(
              'Nothing matched on this page, but there is more to read.',
              style: textTheme.bodyRegular,
              color: colors.onBackgroundVariant,
              textAlign: TextAlign.center,
            ),
            Spaces.verticalSmall,
            MTextButton(
              label: 'Load more',
              onPressed: isLoading ? null : onLoadMore,
            ),
          ],
        ),
      ),
    );
  }
}
