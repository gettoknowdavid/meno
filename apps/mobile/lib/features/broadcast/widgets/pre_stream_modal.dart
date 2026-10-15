import 'package:cached_network_image/cached_network_image.dart';
import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/broadcast/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

class PreStreamModal extends WatchingWidget {
  const new({required this.id, super.key});

  final String id;

  static Future<void> show(BuildContext context, String id) {
    final activeId = di<LiveScopeManager>().activeBroadcastId.value;
    if (activeId == id) return context.push(MRoutes.liveBroadcastRoute(id));
    return context.push(MRoutes.preStreamRoute(id));
  }

  @override
  Widget build(BuildContext context) {
    callOnce((_) => di<StreamManager>().fetchBroadcast.run(id));

    final isLoading = watchValue<StreamManager, bool>(
      (manager) => manager.fetchBroadcast.isRunning,
    );

    final broadcast = watchValue<StreamManager, Broadcast>(
      (manager) => manager.broadcast,
    );

    return MModal(
      title: 'Stream',
      padding: const .fromLTRB(16, 0, 16, 0),
      builder: (context) {
        if (isLoading) return const _LoadingSheet();
        return _LoadedSheet(broadcast: broadcast);
      },
    );
  }
}

class _LoadedSheet extends WatchingWidget {
  const new({required this.broadcast})
    : super(key: const Key('pre-stream-modal-loaded'));

  final Broadcast broadcast;

  @override
  Widget build(BuildContext context) {
    registerHandler<BroadcastEditorManager, void>(
      handler: (context, result, cancel) async {
        context.pop();
        await context.push('/live-session-initialization');
      },
    );

    return DraggableScrollableSheet(
      initialChildSize: 0.22,
      minChildSize: 0.22,
      maxChildSize: 0.85,
      expand: false,
      builder: (context, scrollController) => CustomScrollView(
        controller: scrollController,
        slivers: [
          SliverToBoxAdapter(child: _TopSection(broadcast: broadcast)),
          const SliverToBoxAdapter(child: Spaces.verticalXLarge),
          SliverToBoxAdapter(
            child: PreStreamDescriptionSection(
              description: broadcast.description,
            ),
          ),
          const SliverToBoxAdapter(child: Spaces.verticalXLarge),
          SliverToBoxAdapter(
            child: PreStreamRecentBroadcastsSection(
              creatorId: broadcast.hostId,
            ),
          ),
        ],
      ),
    );
  }
}

class _LoadingSheet extends StatelessWidget {
  const new() : super(key: const Key('pre-stream-modal-loading'));

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: MediaQuery.sizeOf(context).height * 0.20,
      child: const _TopSection(broadcast: .empty, isLoading: true),
    );
  }
}

class _TopSection extends StatelessWidget {
  const new({required this.broadcast, this.isLoading = false});

  final Broadcast broadcast;
  final bool isLoading;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    return LimitedBox(
      maxHeight: 142,
      child: Skeletonizer(
        enabled: isLoading,
        child: Row(
          children: [
            PreStreamArtwork(url: broadcast.imageUrl),
            Spaces.horizontalLarge,
            Expanded(
              child: Column(
                crossAxisAlignment: .start,
                mainAxisAlignment: .center,
                children: [
                  MText(
                    broadcast.title,
                    style: textTheme.subheadingMedium,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                  const SizedBox(height: 6),
                  const MBadge.live(),
                  const SizedBox(height: 6),
                  MText(
                    broadcast.hostName,
                    style: textTheme.captionRegular,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  Spaces.verticalMedium,
                  PreStreamActionButtons(broadcast: broadcast),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class PreStreamArtwork extends StatelessWidget {
  const new({super.key, this.url});

  final String? url;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    Widget imageWidget = SizedBox.square(
      dimension: 142,
      child: Center(
        child: SizedBox(
          height: 142 * 0.4,
          child: switch (colors.brightness) {
            .dark => Assets.images.logoLight.svg(),
            _ => Assets.images.logoDark.svg(),
          },
        ),
      ),
    );

    if (url != null) {
      imageWidget = CachedNetworkImage(
        imageUrl: url!,
        height: 142,
        width: 142,
        imageBuilder: (context, imageProvider) => DecoratedBox(
          decoration: BoxDecoration(
            borderRadius: Corners.lg,
            border: Border.all(color: colors.outlineVariant1),
            image: DecorationImage(image: imageProvider, fit: BoxFit.cover),
          ),
        ),
        placeholder: (context, url) => const MShimmer(borderRadius: 16),
      );
    }

    return imageWidget;
  }
}

class PreStreamActionButtons extends WatchingWidget {
  const new({required this.broadcast, super.key});

  final Broadcast broadcast;

  @override
  Widget build(BuildContext context) {
    final stream = di<StreamManager>();

    final isJoining = watchValue<StreamManager, bool>(
      (manager) => manager.isRunning,
    );

    return SizedBox(
      height: 32,
      child: Row(
        children: [
          Expanded(
            child: MPrimaryButton(
              label: 'Join',
              loading: isJoining,
              onPressed: () => stream.joinBroadcast.run(broadcast.id),
            ),
          ),
          Spaces.horizontalSmall,
          Expanded(
            child: MSecondaryButton(label: 'Share', onPressed: () {}),
          ),
        ],
      ),
    );
  }
}

class PreStreamDescriptionSection extends StatelessWidget {
  const new({required this.description, super.key});

  final String? description;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Row(
          children: [
            const Icon(MIcons.menu_03, size: Insets.lg),
            Spaces.horizontalSmall,
            MText('Description', style: textTheme.subheadingMedium),
          ],
        ),
        Spaces.verticalLarge,
        MText(description ?? 'No description'),
      ],
    );
  }
}

class PreStreamRecentBroadcastsSection extends WatchingWidget {
  const new({required this.creatorId, super.key});

  final String creatorId;

  @override
  Widget build(BuildContext context) {
    final query = BroadcastQuery(
      type: BroadcastsType.recentlyLive,
      creatorId: creatorId,
    );

    final params = query.toQueryParameters();

    final feedSource = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: query,
      );
    });

    return MSection(
      title: 'Recent Broadcasts',
      padding: .zero,
      maxContentHeight: 170,
      onSeeAll: () =>
          context.pushNamed(MRoutes.broadcasts, queryParameters: params),
      builder: (context) => FeedWidget(
        feedSource: feedSource,
        horizontalItemExtent: 148,
        layout: .horizontalList,
        padding: .zero,
        itemBuilder: (context, broadcast) {
          // if (broadcast == null) return const SizedBox.shrink();
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
