import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

const _debounceTime = Duration(milliseconds: 400);

class BroadcastsPage extends WatchingWidget {
  const new({required this.queryParameters, super.key});

  final Map<String, String> queryParameters;

  @override
  Widget build(BuildContext context) {
    final query = BroadcastQuery.fromRouter(queryParameters);
    final type = query.type;

    if (type == null) {
      return const MErrorState(message: 'Broadcasts Type required');
    }

    final feedSource = createOnce(
      () => BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: query,
      ),
    );

    final keywords = createOnce(() => ValueNotifier<String?>(null));
    final debouncedQuery = createOnce(() => keywords.debounce(_debounceTime));

    registerHandler<ValueListenable<String?>, String?>(
      target: debouncedQuery,
      handler: (context, value, _) => feedSource.updateKeywords(value),
    );

    final feedLayout = switch (type) {
      .recentlyLive => FeedLayout.verticalList,
      _ => FeedLayout.verticalGrid,
    };

    final skeletonType = switch (type) {
      .recentlyLive => BroadcastCard.skeletonRecentlyLiveTile,
      _ => BroadcastCard.skeletonLive,
    };

    return MScaffold(
      appBar: AppBar(
        title: MText(type.title, overflow: .ellipsis, maxLines: 1),
        bottom: _SearchBar(onChanged: (input) => keywords.value = input),
        centerTitle: true,
        actions: const [UserAvatarWidget(), Spaces.horizontalLarge],
      ),
      padding: .zero,
      body: FeedWidget<Broadcast?>(
        feedSource: feedSource,
        layout: feedLayout,
        skeletonItem: skeletonType,
        skeletonItemCount: 4,
        gridCrossAxisSpacing: 16,
        gridMainAxisSpacing: 32,
        gridChildAspectRatio: 163.5 / 176,
        padding: const .all(16),
        itemBuilder: broadcastItemBuilder(
          feedSource: feedSource,
          onTap: (b) => context.push(MRoutes.broadcastRoute(b.id.getOrCrash)),
        ),
      ),
    );
  }

  Widget Function(BuildContext, Broadcast?) broadcastItemBuilder({
    required BroadcastFeedDataSource feedSource,
    required void Function(Broadcast) onTap,
  }) {
    return (context, item) {
      if (item == null) return const SizedBox.shrink();
      return switch (feedSource.query.type) {
        .recentlyLive => BroadcastCard.rLiveTile(
          item,
          onTap: () => onTap(item),
        ),
        _ => BroadcastCard.nLive(item, onTap: () => onTap(item)),
      };
    };
  }
}

class _SearchBar extends WatchingWidget implements PreferredSizeWidget {
  const new({required this.onChanged});

  final void Function(String) onChanged;

  @override
  Widget build(BuildContext context) {
    final focusNode = createOnce(FocusNode.new);

    return Padding(
      padding: const .fromLTRB(16, 0, 16, 2),
      child: MSearchBar(
        focusNode: focusNode,
        hintText: 'Search broadcasts',
        onChanged: onChanged,
      ),
    );
  }

  @override
  Size get preferredSize => const Size.fromHeight(56);
}
