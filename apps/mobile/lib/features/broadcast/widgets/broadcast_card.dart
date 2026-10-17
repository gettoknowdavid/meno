import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

class BroadcastCard extends StatelessWidget {
  const new _(
    this.broadcast, {
    required this.onTap,
    required this.type,
    super.key,
  });

  const new nLive(Broadcast broadcast, {required VoidCallback onTap, Key? key})
    : this._(broadcast, onTap: onTap, type: .nLive, key: key);

  const new rLive(Broadcast broadcast, {required VoidCallback onTap, Key? key})
    : this._(broadcast, onTap: onTap, type: .rLive, key: key);

  const new rLiveTile(
    Broadcast broadcast, {
    required VoidCallback onTap,
    Key? key,
  }) : this._(broadcast, onTap: onTap, type: .rLiveTile, key: key);

  /// Skeleton placeholder for live/forYou/nowLive horizontal cards.
  ///
  /// Matches the shape of [BroadcastCard.nLive] — wrap with [Skeletonizer]
  /// in FeedWidget and it shimmers automatically.
  static const Widget skeletonLive = _BroadcastCardSkeleton(
    type: _SkeletonType.nLive,
  );

  /// Skeleton placeholder for recentlyLive horizontal cards.
  ///
  /// Matches the shape of [BroadcastCard.rLive].
  static const Widget skeletonRecentlyLive = _BroadcastCardSkeleton(
    type: _SkeletonType.rLive,
  );

  /// Skeleton placeholder for recentlyLive horizontal cards.
  ///
  /// Matches the shape of [BroadcastCard.rLiveTile].
  static const Widget skeletonRecentlyLiveTile = _BroadcastCardSkeleton(
    type: _SkeletonType.rLiveTile,
  );

  final Broadcast broadcast;
  final VoidCallback onTap;
  final _CardType type;

  @override
  Widget build(BuildContext context) {
    final title = broadcast.title;
    final creator = broadcast.hostName;
    final image = broadcast.image;
    final liveCount = broadcast.totalParticipants;
    final endTime = broadcast.endTime;

    return switch (type) {
      _CardType.nLive => MCard.live(
        key: key,
        title: title.orEmpty,
        host: creator.orEmpty,
        imageUrl: image?.getUrl,
        liveCount: liveCount,
        onTap: onTap,
      ),
      _CardType.rLive => MCard.recentlyLive(
        key: key,
        title: title.orEmpty,
        host: creator.orEmpty,
        imageUrl: image?.getUrl,
        onTap: onTap,
      ),
      _CardType.rLiveTile => MRecentlyLiveListTile(
        key: key,
        title: title.orEmpty,
        creator: creator.orEmpty,
        imageUrl: image?.getUrl,
        endTime: endTime,
        onTap: onTap,
      ),
    };
  }
}

enum _CardType { nLive, rLive, rLiveTile }

enum _SkeletonType { nLive, rLive, rLiveTile }

/// Shape-accurate skeleton for [BroadcastCard].
///
/// Sized to match the 148-wide cards used on the home page horizontal strips.
/// [Skeletonizer] shimmers over every child automatically — no custom painting
/// needed. Strings must be non-empty so the text widgets have measurable size.
class _BroadcastCardSkeleton extends StatelessWidget {
  const new({required this.type});

  final _SkeletonType type;

  @override
  Widget build(BuildContext context) {
    final broadcast = fakeBroadcastList[0];
    return switch (type) {
      _SkeletonType.nLive => Skeletonizer(
        child: MCard.live(
          key: key,
          title: broadcast.title.orEmpty,
          host: broadcast.hostName.orEmpty,
          imageUrl: broadcast.image?.getUrl,
          liveCount: broadcast.liveParticipantsCount,
        ),
      ),
      _SkeletonType.rLive => Skeletonizer(
        child: MCard.recentlyLive(
          key: key,
          title: broadcast.title.orEmpty,
          host: broadcast.hostName.orEmpty,
          imageUrl: broadcast.image?.getUrl,
        ),
      ),
      _SkeletonType.rLiveTile => Skeletonizer(
        child: MRecentlyLiveListTile(
          key: key,
          title: broadcast.title.orEmpty,
          creator: broadcast.hostName.orEmpty,
          endTime: broadcast.endTime,
          imageUrl: broadcast.image?.getUrl,
        ),
      ),
    };
  }
}
