import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

class BroadcastArtworkWidget extends StatelessWidget {
  const new({
    required this.image,
    this.radius = 48,
    this.outerBoxHeight = 144,
    this.outerBoxWidth = 152,
    this.boxPadding = const .symmetric(horizontal: 28, vertical: 16),
    super.key,
  });

  final ImageInput? image;
  final double radius;
  final double? outerBoxHeight;
  final double? outerBoxWidth;
  final EdgeInsetsGeometry? boxPadding;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: outerBoxHeight,
      width: outerBoxWidth,
      padding: boxPadding,
      alignment: Alignment.center,
      child: MAvatar(radius: radius, url: image?.getUrl),
    );
  }
}

class BroadcastCreatorWidget extends StatelessWidget {
  const new({required this.name, super.key});

  final String name;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return MText(
      name,
      color: colors.onDisabledContainer,
      style: textTheme.captionRegular,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      textAlign: TextAlign.center,
    );
  }
}

class BroadcastTitleWidget extends StatelessWidget {
  const new({
    required this.title,
    this.maxLines = 2,
    this.padding = const .symmetric(horizontal: Insets.md),
    super.key,
  });

  final String title;
  final int maxLines;
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    return Padding(
      padding: padding,
      child: MText(
        title,
        maxLines: 2,
        textAlign: .center,
        style: textTheme.subheadingBold,
        overflow: .ellipsis,
      ),
    );
  }
}

class BroadcastStatusWidget extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final status = watchValue<LiveSessionManager, LiveStatus>(
      (manager) => manager.status,
    );

    return switch (status) {
      .live => const MBadge.live(),
      .offAir => MBadge.offAir(context),
      .reconnecting => MBadge.reconnecting(context),
      _ => MBadge.offAir(context),
    };
  }
}

class BroadcastAboutTab extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final broadcast = watchValue<LiveSessionManager, Broadcast>(
      (manager) => manager.broadcast,
    );

    return SingleChildScrollView(
      padding: const .symmetric(horizontal: Insets.lg),
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          const MSpacer.v(Insets.xl),
          const Row(
            children: [
              Icon(MIcons.menu_03, size: Insets.lg),
              Spaces.horizontalSmall,
              MText.subheading('About Broadcast', weight: .bold),
            ],
          ),
          const MSpacer.v(Insets.lg),
          MText(
            broadcast.description?.getOrElse((_) => '') ?? '',
            style: MTextTheme.of(context).captionRegular.copyWith(height: 1.4),
          ),
        ],
      ),
    );
  }
}

class BroadcastListeningTab extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final list = watchValue<ParticipantsManager, List<Participant>>(
      (manager) => manager.displayedParticipants,
    );

    final hasMore = watchValue<ParticipantsManager, bool>(
      (manager) => manager.hasMore,
    );

    final isLoading = watchValue<ParticipantsManager, bool>(
      (manager) => manager.isLoading,
    );

    return Column(
      key: const Key('BroadcastListeningTab'),
      children: [
        Spaces.verticalXLarge,
        const ParticipantListHeaderWidget(),

        Spaces.verticalLarge,
        Expanded(child: ParticipantsGrid(list, isLoading: isLoading)),

        if (hasMore) ...[
          Spaces.verticalLarge,
          MSecondaryButton(
            label: 'Load More',
            onPressed: () => di<ParticipantsManager>().loadMore(),
          ),
        ],
      ],
    );
  }
}

class BroadcastOptionsButton extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    return IconButton.outlined(
      icon: const Icon(MIcons.dots_horizontal),
      iconSize: 20,
      color: colors.onBackground,
      style: IconButton.styleFrom(
        fixedSize: const .fromWidth(48),
        side: BorderSide(color: colors.outlineVariant3),
        shape: const RoundedRectangleBorder(borderRadius: Corners.lg),
      ),
      onPressed: () => BroadcastInfoModal.show(context),
    );
  }
}
