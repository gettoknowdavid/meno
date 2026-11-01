import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// The "All" tab: the live strip over the recently-live one.
///
/// Both strips are the same feeds Home draws, read through the same
/// [BroadcastFeedDataSource], so a broadcast that starts or ends reaches this
/// tab the way it reaches Home. Each strip is capped at eight rows and does not
/// paginate: the whole list is a tap away on its own tab.
class AllBroadcastsWidget extends StatelessWidget {
  /// Creates the tab.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    return const SingleChildScrollView(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Spaces.verticalXLarge,
          _NowLiveSection(key: Key('DiscoverNowLiveSection')),
          Spaces.verticalXXLarge,
          _RecentlyLiveSection(key: Key('DiscoverRecentlyLiveSection')),
          Spaces.verticalXXLarge,
        ],
      ),
    );
  }
}

class _NowLiveSection extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final feedSource = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: const BroadcastQuery.nowLive(limit: 8),
        fetchMoreEnabled: false,
      );
    });

    return MSection(
      title: 'Now Live',
      maxContentHeight: 376,
      titleIcon: Assets.images.flame.image(
        height: 24,
        width: 24,
        package: 'meno_ui',
      ),
      onSeeAll: () => context.go(MRoutes.discoverNowLiveTab),
      builder: (context) => FeedWidget(
        feedSource: feedSource,
        horizontalItemExtent: 148,
        gridCrossAxisSpacing: 24,
        gridMainAxisSpacing: 24,
        skeletonItem: BroadcastCard.skeletonLive,
        skeletonItemCount: 4,
        layout: .horizontalGrid,
        padding: const .symmetric(horizontal: 16),
        itemBuilder: (context, broadcast) {
          return BroadcastCard.nLive(
            broadcast,
            key: ValueKey(broadcast.id),
            onTap: () => PreStreamModal.show(context, broadcast.id),
          );
        },
      ),
    );
  }
}

class _RecentlyLiveSection extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final feedSource = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: const BroadcastQuery.recentlyLive(limit: 8),
        fetchMoreEnabled: false,
      );
    });

    return MSection(
      title: 'Recently Live',
      maxContentHeight: 376,
      titleIcon: Assets.images.highVoltage.image(
        height: 24,
        width: 24,
        package: 'meno_ui',
      ),
      onSeeAll: () => context.go(MRoutes.discoverRecentlyLiveTab),
      builder: (context) => FeedWidget(
        feedSource: feedSource,
        horizontalItemExtent: 148,
        gridCrossAxisSpacing: 24,
        gridMainAxisSpacing: 24,
        skeletonItem: BroadcastCard.skeletonRecentlyLive,
        skeletonItemCount: 4,
        layout: .horizontalGrid,
        padding: const .symmetric(horizontal: 16),
        itemBuilder: (context, broadcast) {
          return BroadcastCard.rLive(
            broadcast,
            key: ValueKey(broadcast.id),
            onTap: () => context.push(MRoutes.broadcastRoute(broadcast.id)),
          );
        },
      ),
    );
  }
}
