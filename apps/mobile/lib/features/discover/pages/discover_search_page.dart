import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/discover/discover.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_ui/meno_ui.dart';

/// Discover's search screen: recent terms before anything is typed, the found
/// accounts after.
///
/// The manager behind it is the account's, taken from the locator rather than
/// built here: it lives as long as the account does, so leaving this screen and
/// coming back keeps the term and its results, and what was searched for is
/// stored under the account that typed it.
class DiscoverSearchPage extends WatchingWidget {
  /// Creates the page.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    return const _SearchView();
  }
}

class _SearchView extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final manager = di<DiscoverSearchManager>();

    final searchTextController = createOnce(() {
      final controller = TextEditingController(text: manager.query.value);
      manager.query.listen((newValue, _) {
        if (controller.text != newValue) {
          controller.text = newValue;
          controller.selection = .collapsed(offset: newValue.length);
        }
      });
      return controller;
    });

    // Reading the last item asks the feed for another page; this listener is
    // the other half, so a list that stops short of the trigger — or a scroll
    // that arrives after a page landed — still continues.
    final scrollController = createOnce(() {
      final controller = ScrollController();
      controller.addListener(() {
        if (!controller.hasClients) return;
        final max = controller.position.maxScrollExtent;
        final current = controller.position.pixels;
        if (max - current <= 120.0) manager.fetchMore.run();
      });
      return controller;
    });

    return MScaffold(
      padding: .zero,
      appBar: AppBar(
        automaticallyImplyLeading: false,
        leading: const SizedBox.shrink(),
        flexibleSpace: Align(
          alignment: Alignment.bottomCenter,
          child: DiscoverSearchBar(
            padding: const .fromLTRB(16, 8, 16, 4),
            showCancelButton: true,
            onSubmitted: manager.search.run,
            controller: searchTextController,
          ),
        ),
      ),
      body: CustomScrollView(
        controller: scrollController,
        physics: const AlwaysScrollableScrollPhysics(),
        slivers: const [_SearchBody()],
      ),
    );
  }
}

class _SearchBody extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final query = watchValue<DiscoverSearchManager, String>((m) => m.query);

    if (query.trim().isEmpty) return const _RecentSearchesWidget();

    return const _SearchResults();
  }
}

class _RecentSearchesWidget extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final manager = di<DiscoverSearchManager>();

    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final recents = watchValue<DiscoverSearchManager, List<String>>(
      (m) => m.recentSearches,
    );

    if (recents.isEmpty) return const SliverToBoxAdapter(child: SizedBox());

    return SliverMainAxisGroup(
      slivers: [
        SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: MText('Recent searches', style: textTheme.subheadingBold),
          ),
        ),
        SliverList.separated(
          itemCount: recents.length,
          separatorBuilder: (_, _) => const SizedBox(height: 24),
          itemBuilder: (context, i) {
            final term = recents[i];
            return ListTile(
              visualDensity: .compact,
              minTileHeight: 24,
              contentPadding: const .symmetric(horizontal: 16),
              iconColor: colors.inActive,
              leading: const Icon(MIcons.clock_rewind, size: 20),
              horizontalTitleGap: 8,
              title: MText(term, style: textTheme.bodyRegular),
              trailing: MIconButton(
                onPressed: () => manager.removeRecentSearch.run(term),
                color: colors.onBackground,
                icon: const Icon(MIcons.x_close),
              ),
              onTap: () => manager.search.run(term),
            );
          },
        ),
        SliverToBoxAdapter(
          child: Center(
            child: MTextButton(
              label: 'Clear recent searches',
              onPressed: manager.clearRecentSearches.run,
            ),
          ),
        ),
      ],
    );
  }
}

class _SearchResults extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final manager = di<DiscoverSearchManager>();

    final isSearching = watchValue<DiscoverSearchManager, bool>(
      (m) => m.isSearching,
    );
    final isFetchingMore = watchValue<DiscoverSearchManager, bool>(
      (m) => m.feed.isFetching,
    );
    final hasMore = watchValue<DiscoverSearchManager, bool>((m) => m.hasMore);
    final count = watchValue<DiscoverSearchManager, int>(
      (m) => m.feed.itemCount,
    );
    final pageError = watch(manager.feed.pageError).value;

    if (isSearching) {
      return const SliverFillRemaining(
        hasScrollBody: false,
        child: Center(child: MLoadingIndicator.box()),
      );
    }

    if (count == 0 && !isFetchingMore) {
      return const SliverFillRemaining(
        hasScrollBody: false,
        child: _NoResultsWidget(),
      );
    }

    return SliverMainAxisGroup(
      slivers: [
        SliverPadding(
          padding: const EdgeInsets.fromLTRB(16, 28, 16, 32),
          sliver: SliverGrid.builder(
            gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
              crossAxisCount: 2,
              mainAxisSpacing: 24,
              crossAxisSpacing: 24,
              childAspectRatio: 159.50 / 192,
            ),
            itemCount: count,
            itemBuilder: (context, i) {
              // Reading through the feed is what asks for the next page once
              // the grid is within three of its end.
              final profile = manager.feed.getItemAtIndex(i);
              return ProfileCard(
                profile: profile,
                onTap: () =>
                    context.push(MRoutes.userProfileRoute(profile.id)),
              );
            },
          ),
        ),
        SliverToBoxAdapter(
          child: Column(
            children: [
              Spaces.verticalXLarge,
              if (pageError != null)
                MTextButton(label: 'Retry', onPressed: manager.retryPage)
              else
                MPagedLoadingIndicator(
                  isLoading: isFetchingMore,
                  hasMore: hasMore,
                ),
              Spaces.verticalXLarge,
            ],
          ),
        ),
      ],
    );
  }
}

class _NoResultsWidget extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Assets.images.liveForYou.image(
            height: 152,
            width: 152,
            package: 'meno_ui',
          ),
          Spaces.verticalXLarge,
          MText(
            'No Results',
            style: textTheme.heading3Bold,
            textAlign: TextAlign.center,
          ),
          Spaces.verticalMicro,
          MText(
            'Try a new search',
            style: textTheme.bodyRegular,
            color: colors.inActiveContainer,
          ),
        ],
      ),
    );
  }
}
