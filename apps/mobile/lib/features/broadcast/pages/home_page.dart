import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/manager/user_manager.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/broadcast/widgets/_widgets.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_ui/meno_ui.dart';

class HomePage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final profile = watchValue<MyProfileManager, MyProfile>(
      (manager) => manager.profile,
    );

    // So, this is the check or flag I am using for now to determine which
    // section to show - `_LiveForYouSection` or `_NowLiveSection`
    //
    // For now, I am checking if the currently authenticated user is
    // following at least one account, if they are, then we show live broadcasts
    // from the `_LiveForYouSection`, else, we show the `_NowLiveSection`
    final isNewUser = profile.stats.following > 0;

    return Scaffold(
      appBar: const _AppBar(),
      body: SingleChildScrollView(
        clipBehavior: .none,
        padding: const .only(bottom: 32),
        physics: const AlwaysScrollableScrollPhysics(
          parent: BouncingScrollPhysics(),
        ),
        child: Column(
          children: <Widget>[
            const LiveSessionBanner(),
            Spaces.verticalXLarge,
            if (isNewUser)
              const _LiveForYouSection()
            else
              const _NowLiveSection(),
            Spaces.verticalXXLarge,
            const _RecentlyLiveSection(),
          ],
        ),
      ),
    );
  }
}

class _AppBar extends WatchingWidget implements PreferredSizeWidget {
  const new();

  @override
  Size get preferredSize => ToolBarHeights.home;

  @override
  Widget build(BuildContext context) {
    final user = watchValue<UserManager, User>(
      (manager) => manager.currentUser,
    );

    const unreadCount = 0;

    // final unreadCount = watchValue<NotificationsManager, int>(
    //   (manager) => manager.unreadCount,
    // );

    final textTheme = MTextTheme.of(context);
    final colors = MColorScheme.of(context);

    return AppBar(
      flexibleSpace: SafeArea(
        child: Container(
          margin: const .only(top: Insets.sm),
          padding: const .symmetric(horizontal: 16, vertical: 6),
          height: 56,
          child: Row(
            children: [
              Expanded(
                child: Column(
                  mainAxisAlignment: .end,
                  crossAxisAlignment: .start,
                  mainAxisSize: .min,
                  children: [
                    MText('Hello,', style: textTheme.captionRegular),
                    MText(user.fullName, style: textTheme.subheadingMedium),
                  ],
                ),
              ),
              Spaces.horizontalSmall,
              Row(
                children: [
                  Badge(
                    isLabelVisible: unreadCount > 0,
                    label: const Text('$unreadCount'),
                    backgroundColor: colors.error,
                    textColor: colors.onError,
                    child: MIconButton(
                      icon: const Icon(MIcons.bell),
                      iconSize: 20,
                      onPressed: () => context.push(MRoutes.notifications),
                    ),
                  ),
                  const SizedBox(width: 24),
                  MAvatar(
                    radius: Insets.lg,
                    url: user.avatarUrl,
                    onTap: () => context.go(MRoutes.myProfile),
                    hasBorder: false,
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _LiveForYouSection extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final feedSource = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: const BroadcastQuery.forYou(),
        fetchMoreEnabled: false,
      );
    });

    final params = feedSource.query.toRouterParams();

    return MSection(
      title: 'Live For You',
      titleIcon: Assets.images.sparkles.image(
        height: 24,
        width: 24,
        package: 'meno_ui',
      ),
      onSeeAll: () =>
          context.pushNamed(MRoutes.broadcasts, queryParameters: params),
      builder: (context) => FeedWidget(
        feedSource: feedSource,
        horizontalItemExtent: 148,
        layout: .horizontalList,
        skeletonItem: BroadcastCard.skeletonLive,
        skeletonItemCount: 4,
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

class _NowLiveSection extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final feedSource = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: const BroadcastQuery.nowLive(),
        fetchMoreEnabled: false,
      );
    });

    return MSection(
      title: 'Now Live',
      titleIcon: Assets.images.flame.image(
        height: 24,
        width: 24,
        package: 'meno_ui',
      ),
      onSeeAll: () => context.pushNamed(
        MRoutes.broadcasts,
        queryParameters: feedSource.query.toRouterParams(),
      ),
      builder: (context) => FeedWidget(
        feedSource: feedSource,
        horizontalItemExtent: 148,
        layout: .horizontalList,
        skeletonItem: BroadcastCard.skeletonLive,
        skeletonItemCount: 4,
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
  const new();

  @override
  Widget build(BuildContext context) {
    final feedSource = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: const BroadcastQuery.recentlyLive(),
        fetchMoreEnabled: false,
      );
    });

    return MSection(
      title: 'Recently Live',
      titleIcon: Assets.images.highVoltage.image(
        height: 24,
        width: 24,
        package: 'meno_ui',
      ),
      onSeeAll: () => context.pushNamed(
        MRoutes.broadcasts,
        queryParameters: const BroadcastQuery.recentlyLive().toRouterParams(),
      ),
      builder: (context) => FeedWidget(
        feedSource: feedSource,
        horizontalItemExtent: 148,
        layout: .horizontalList,
        skeletonItem: BroadcastCard.skeletonRecentlyLive,
        skeletonItemCount: 4,
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
