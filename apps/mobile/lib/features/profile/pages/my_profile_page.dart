import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno/_routing/_routing.dart';
import 'package:meno/_shared/_shared.dart';
import 'package:meno/features/broadcast/broadcast.dart';
import 'package:meno/features/profile/profile.dart';
import 'package:meno/features/profile/widgets/_widgets.dart';
import 'package:meno_design_system/meno_design_system.dart';

const _kTabBarHeight = 32.0;

class MyProfilePage extends WatchingStatefulWidget {
  const MyProfilePage({super.key});

  @override
  State<MyProfilePage> createState() => _MyProfilePageState();
}

class _MyProfilePageState extends State<MyProfilePage> {
  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    final scrollCtrl = createOnce(ScrollController.new);

    final userId = watchValue((UserManager m) => m.currentUserId).value;
    final profile = watchValue((MyProfileManager m) => m.profile);
    final isEmpty = profile.isEmpty;

    return DefaultTabController(
      length: 3,
      child: Scaffold(
        key: ValueKey(userId),
        backgroundColor: colors.background,
        body: RefreshIndicator(
          onRefresh: di<MyProfileManager>().fetch.runAsync,
          child: NestedScrollView(
            controller: scrollCtrl,
            headerSliverBuilder: (context, innerBoxIsScrolled) => [
              _ProfileAppBar(
                profile: profile,
                innerBoxIsScrolled: innerBoxIsScrolled,
              ),
            ],
            body: TabBarView(
              children: [
                if (isEmpty) ...[
                  const SizedBox.shrink(),
                  const SizedBox.shrink(),
                  const SizedBox.shrink(),
                ] else ...[
                  const ProfileRecentBroadcastsTab(),
                  const ProfileAllBroadcastsTab(),
                  const ProfileFavouritesTab(),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _ProfileTitle extends StatelessWidget {
  const _ProfileTitle({required this.profile});

  final Profile profile;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final name = profile.fullName.getOrElse((_) => 'Unknown User');

    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () => SwitchAccountModal.show(context),
      child: SizedBox(
        height: 30,
        child: Row(
          mainAxisSize: .min,
          children: [
            Container(
              width: 3,
              margin: const .symmetric(vertical: 2),
              color: colors.error,
            ),
            Spaces.horizontalMicro,
            MText(
              name,
              style: textTheme.heading3Bold,
              color: colors.onBackground,
            ),
            Spaces.horizontalSmall,
            const Icon(MIcons.chevron_down, size: 20),
          ],
        ),
      ),
    );
  }
}

class _ProfileAppBar extends StatelessWidget {
  const _ProfileAppBar({
    required this.profile,
    required this.innerBoxIsScrolled,
  });

  final Profile profile;
  final bool innerBoxIsScrolled;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return SliverOverlapAbsorber(
      handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
      sliver: DynamicSliverAppBar(
        toolbarHeight: kToolbarHeight + 8,
        backgroundColor: colors.background,
        surfaceTintColor: Colors.transparent,
        pinned: true,
        forceElevated: innerBoxIsScrolled,
        elevation: innerBoxIsScrolled ? 1 : 0,
        shadowColor: colors.onBackground.withValues(alpha: 0.08),
        title: _ProfileTitle(profile: profile),
        actions: [
          MIconButton(
            icon: const Icon(MIcons.settings),
            color: colors.primary,
            onPressed: () => context.push(R.settings),
          ),
          Spaces.horizontalLarge,
        ],
        flexibleSpace: MyProfileHeaderContent(profile: profile),
        bottom: PreferredSize(
          preferredSize: const .fromHeight(_kTabBarHeight),
          child: Container(
            color: colors.background,
            height: _kTabBarHeight,
            child: TabBar(
              padding: const .symmetric(horizontal: 16),
              labelStyle: textTheme.captionMedium,
              isScrollable: true,
              tabAlignment: .start,
              labelPadding: const .symmetric(horizontal: 16),
              tabs: const [
                Tab(text: 'Recent broadcasts'),
                Tab(text: 'All broadcasts'),
                Tab(text: 'Favorites'),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class ProfileRecentBroadcastsTab extends WatchingStatefulWidget {
  const ProfileRecentBroadcastsTab({super.key});

  @override
  State<ProfileRecentBroadcastsTab> createState() =>
      _ProfileRecentBroadcastsTabState();
}

class _ProfileRecentBroadcastsTabState extends State<ProfileRecentBroadcastsTab>
    with AutomaticKeepAliveClientMixin {
  @override
  Widget build(BuildContext context) {
    super.build(context);

    final feedSource = createOnce(() {
      final creatorId = di<UserManager>().currentUserId.value;
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: BroadcastQuery.recentlyLive(creatorId: creatorId),
      );
    });

    return FeedWidget(
      feedSource: feedSource,
      padding: const .all(16),
      topSlivers: [
        SliverOverlapInjector(
          handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
        ),
      ],
      skeletonItem: BroadcastCard.skeletonRecentlyLiveTile,
      skeletonItemCount: 4,
      itemBuilder: (context, broadcast) {
        if (broadcast == null) return const SizedBox.shrink();
        return BroadcastCard.rLiveTile(
          broadcast,
          key: ValueKey(broadcast.id.getOrCrash()),
          onTap: () => context.push(R.broadcast(broadcast.id.getOrCrash())),
        );
      },
    );
  }

  @override
  bool get wantKeepAlive => true;
}

class ProfileAllBroadcastsTab extends WatchingStatefulWidget {
  const ProfileAllBroadcastsTab({super.key});

  @override
  State<ProfileAllBroadcastsTab> createState() =>
      _ProfileAllBroadcastsTabState();
}

class _ProfileAllBroadcastsTabState extends State<ProfileAllBroadcastsTab>
    with AutomaticKeepAliveClientMixin {
  @override
  Widget build(BuildContext context) {
    super.build(context);

    final feedSource = createOnce(() {
      final creatorId = di<UserManager>().currentUserId.value;
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: BroadcastQuery(creatorId: creatorId),
      );
    });

    return FeedWidget(
      feedSource: feedSource,
      padding: const .all(16),
      topSlivers: [
        SliverOverlapInjector(
          handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
        ),
      ],
      skeletonItem: BroadcastCard.skeletonRecentlyLiveTile,
      skeletonItemCount: 4,
      itemBuilder: (context, broadcast) {
        if (broadcast == null) return const SizedBox.shrink();
        return BroadcastCard.rLiveTile(
          broadcast,
          key: ValueKey(broadcast.id.getOrCrash()),
          onTap: () => context.push(R.broadcast(broadcast.id.getOrCrash())),
        );
      },
    );
  }

  @override
  bool get wantKeepAlive => true;
}

class ProfileFavouritesTab extends WatchingStatefulWidget {
  const ProfileFavouritesTab({super.key});

  @override
  State<ProfileFavouritesTab> createState() => _ProfileFavouritesTabState();
}

class _ProfileFavouritesTabState extends State<ProfileFavouritesTab>
    with AutomaticKeepAliveClientMixin {
  @override
  Widget build(BuildContext context) {
    super.build(context);

    final favourites = watchValue((FavouritesManager m) => m.favourites);

    if (favourites.isEmpty) {
      return const MenoEmptyWidget(title: 'No favourite broadcasts yet');
    }

    return CustomScrollView(
      slivers: [
        SliverOverlapInjector(
          handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
        ),
        SliverPadding(
          padding: const .all(Insets.lg),
          sliver: SliverList.separated(
            separatorBuilder: (_, __) => Spaces.verticalLarge,
            itemCount: favourites.length,
            itemBuilder: (context, index) {
              final favourite = favourites[index];
              if (favourite == null) return const SizedBox.shrink();
              return MRecentlyLiveListTile(
                title: favourite.title,
                creator: favourite.creatorName,
                imageUrl: favourite.imageUrl,
                onTap: () => context.push(R.broadcast(favourite.broadcastId)),
              );
            },
          ),
        ),
      ],
    );
  }

  @override
  bool get wantKeepAlive => true;
}
