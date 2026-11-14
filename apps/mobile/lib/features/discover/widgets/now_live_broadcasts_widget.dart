import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

/// The "Now Live" tab: everything on air, newest first.
///
/// The feed is page-owned — [createOnce] builds it with the tab and disposes it
/// with the tab's state, which the discover shell keeps alive while the account
/// lasts. It paginates as the grid nears its end, so the tab doubles as the
/// "see all" list.
class NowLiveBroadcastsWidget extends WatchingWidget {
  /// Creates the tab.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final feedSource = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: const BroadcastQuery.nowLive(),
      );
    });

    return FeedWidget(
      feedSource: feedSource,
      gridCrossAxisSpacing: 24,
      gridMainAxisSpacing: 24,
      gridChildAspectRatio: 159.50 / 176,
      layout: .verticalGrid,
      skeletonItem: BroadcastCard.skeletonLive,
      padding: const .fromLTRB(16, 28, 16, 32),
      itemBuilder: (context, broadcast) {
        return BroadcastCard.nLive(
          broadcast,
          key: ValueKey(broadcast.id),
          onTap: () => PreStreamModal.show(context, broadcast.id),
        );
      },
    );
  }
}
