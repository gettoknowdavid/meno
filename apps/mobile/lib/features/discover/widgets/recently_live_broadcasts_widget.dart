import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

/// The "Recently Live" tab: everything that has ended, newest first.
///
/// The endpoint has no "ended" status — ending a broadcast writes `inactive`,
/// which drafts and scheduled ones share — so the feed drops what has not
/// ended. The list is a grid of tiles rather than cards because a finished
/// broadcast is read as a row of history, not as something to join.
class RecentlyLiveBroadcastsWidget extends WatchingWidget {
  /// Creates the tab.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final feedSource = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: const BroadcastQuery.recentlyLive(),
      );
    });

    return FeedWidget(
      feedSource: feedSource,
      gridCrossAxisSpacing: 24,
      gridMainAxisSpacing: 24,
      gridChildAspectRatio: 159.50 / 176,
      layout: .verticalGrid,
      padding: const .fromLTRB(16, 28, 16, 32),
      skeletonItem: BroadcastCard.skeletonRecentlyLive,
      itemBuilder: (context, broadcast) {
        return BroadcastCard.rLive(
          broadcast,
          key: ValueKey(broadcast.id),
          onTap: () => context.push(MRoutes.broadcastRoute(broadcast.id)),
        );
      },
    );
  }
}
