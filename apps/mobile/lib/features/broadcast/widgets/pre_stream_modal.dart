import 'package:cached_network_image/cached_network_image.dart';
import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

class PreStreamModal extends WatchingWidget {
  const new({required this.id, super.key});

  final Id id;

  /// Opens the lobby for [id], unless this account is already in a room.
  ///
  /// The decision comes from [LivePhase], never from "is there an active id":
  /// while a session is connecting, live, reconnecting or ending, a second
  /// entry would replace the room the person is in. The same broadcast means
  /// "return to it"; another one means leaving first, explicitly.
  static Future<void> show(BuildContext ctx, Id id) async {
    final snapshot = di<ActiveBroadcastManager>().snapshot.value;

    if (snapshot.isInRoom) {
      if (snapshot.broadcast?.id == id) {
        await ctx.push(MRoutes.liveBroadcast);
        return;
      }

      final leave = await PreStreamLeaveDialog.show(ctx);
      if (!(leave ?? false)) return;
      if (!di.isRegistered<ActiveBroadcastManager>()) return;
      await di<ActiveBroadcastManager>().exitRoom();
      if (!ctx.mounted) return;
    }

    await ctx.push(MRoutes.preStreamRoute(id));
  }

  @override
  Widget build(BuildContext context) {
    // The lobby owns its own details manager: it is created with the sheet and
    // disposed with it, so a page that closes never leaves one behind.
    final details = createOnce(
      () => BroadcastDetailsManager(
        http: di<BroadcastHttpService>(),
        userId: di<ActiveBroadcastManager>().accountId,
        broadcastId: id,
        logger: di<MenoLogger>(),
      ),
    );

    callOnce((_) => details.fetch.run());

    final isRunning = watch(details.fetch.isRunning).value;
    final proxy = watch(details.proxy).value;

    return MModal(
      title: 'Stream',
      padding: const .fromLTRB(16, 0, 16, 0),
      builder: (context) {
        if (proxy == null) {
          return isRunning ? const _LoadingSheet() : const _MissingSheet();
        }
        return _LoadedSheet(broadcast: proxy.broadcast);
      },
    );
  }
}

/// The API had nothing to say about this broadcast.
class _MissingSheet extends StatelessWidget {
  const new() : super(key: const Key('pre-stream-modal-missing'));

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: MediaQuery.sizeOf(context).height * 0.20,
      child: const Center(child: MText('This broadcast is not available.')),
    );
  }
}

class _LoadedSheet extends WatchingWidget {
  const new({required this.broadcast})
    : super(key: const Key('pre-stream-modal-loaded'));

  final Broadcast broadcast;

  @override
  Widget build(BuildContext context) {
    // The sheet closes and the transition page takes over as soon as the join
    // has produced a session. A failed join leaves the sheet where it is: the
    // command's own error handling has already said why.
    registerHandler<ActiveBroadcastManager, void>(
      select: (manager) => manager.join,
      handler: (context, _, _) async {
        if (!di<ActiveBroadcastManager>().snapshot.value.isInRoom) return;
        context.pop();
        await context.push(MRoutes.liveSessionInit);
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
              description: broadcast.description?.getOrNull,
            ),
          ),
          const SliverToBoxAdapter(child: Spaces.verticalXLarge),
          SliverToBoxAdapter(
            child: PreStreamRecentBroadcastsSection(
              creatorId: broadcast.hostId.getOrNull,
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
            PreStreamArtwork(url: broadcast.image?.getUrl),
            Spaces.horizontalLarge,
            Expanded(
              child: Column(
                crossAxisAlignment: .start,
                mainAxisAlignment: .center,
                children: [
                  MText(
                    broadcast.title.orEmpty,
                    style: textTheme.subheadingMedium,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                  const SizedBox(height: 6),
                  const MBadge.live(),
                  const SizedBox(height: 6),
                  MText(
                    broadcast.hostName.orEmpty,
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
    final live = di<ActiveBroadcastManager>();

    final isJoining = watchValue<ActiveBroadcastManager, bool>(
      (manager) => manager.join.isRunning,
    );

    return SizedBox(
      height: 32,
      child: Row(
        children: [
          Expanded(
            child: MPrimaryButton(
              label: 'Join',
              loading: isJoining,
              onPressed: () => live.join.run(
                LiveEnterRequest(
                  broadcastId: broadcast.id,
                  mode: LiveEnterMode.listen,
                ),
              ),
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

  final String? creatorId;

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
            onTap: () async {
              await context.push(MRoutes.broadcastRoute(broadcast.id));
            },
          );
        },
      ),
    );
  }
}
