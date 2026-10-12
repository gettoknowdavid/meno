import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno/_core/_core.dart';
import 'package:meno/_routing/_routing.dart';
import 'package:meno/_shared/_shared.dart';
import 'package:meno/features/broadcast/broadcast.dart';
import 'package:meno/features/profile/profile.dart';
import 'package:meno/features/profile/widgets/_widgets.dart';
import 'package:meno_design_system/meno_design_system.dart';

const _kTabBarHeight = 32.0;

class UserProfilePage extends WatchingWidget {
  const UserProfilePage({required this.userIdStr, super.key});

  final String userIdStr;

  @override
  Widget build(BuildContext context) {
    pushScope(
      init: (getIt) => getIt.registerLazySingleton(() {
        final uId = Id.fromString(userIdStr);
        return UserProfileManager(userId: uId, http: di<ProfileHttpService>());
      }, onCreated: (instance) => instance.fetch.run()),
    );

    registerHandler(
      handler: (context, errors, cancel) {
        final error = errors?.error;
        if (error == null) return;
        if (error is MenoException) {
          context.showErrorSnackBar(error.message);
        } else {
          context.showErrorSnackBar(error.toString());
        }
        context.pop();
      },
      select: (UserProfileManager manager) => manager.fetch.errors,
    );

    final proxy = watchValue((UserProfileManager m) => m.proxy);
    final isFetching = watchValue((UserProfileManager m) => m.fetch.isRunning);

    if (isFetching) return const LoadingPage();
    if (!isFetching && proxy == null) return const _EmptyPage();
    return _Content(proxy: proxy!);
  }
}

class _Content extends WatchingWidget {
  const _Content({required this.proxy});

  final UserProfileProxy proxy;

  @override
  Widget build(BuildContext context) {
    watch(proxy);

    final colors = MColorScheme.of(context);

    final recentBroadcastsFeed = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: BroadcastQuery.recentlyLive(creatorId: proxy.id),
      );
    });

    final allBroadcastsFeed = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: BroadcastQuery(creatorId: proxy.id),
      );
    });

    return DefaultTabController(
      length: 2,
      child: Scaffold(
        backgroundColor: colors.background,
        body: RefreshIndicator(
          onRefresh: () async {},
          child: NestedScrollView(
            physics: const AlwaysScrollableScrollPhysics(),
            headerSliverBuilder: (context, innerBoxIsScrolled) {
              return [
                _ProfileAppBar(
                  proxy: proxy,
                  innerBoxIsScrolled: innerBoxIsScrolled,
                ),
              ];
            },
            body: TabBarView(
              children: [
                _RecentBroadcastsTab(feed: recentBroadcastsFeed),
                _AllBroadcastsTab(feed: allBroadcastsFeed),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _ProfileAppBar extends StatelessWidget {
  const _ProfileAppBar({required this.proxy, required this.innerBoxIsScrolled});

  final UserProfileProxy proxy;
  final bool innerBoxIsScrolled;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return DynamicSliverAppBar(
      toolbarHeight: kToolbarHeight,
      backgroundColor: colors.background,
      surfaceTintColor: Colors.transparent,
      pinned: true,
      forceElevated: innerBoxIsScrolled,
      elevation: innerBoxIsScrolled ? 1 : 0,
      shadowColor: colors.onBackground.withValues(alpha: 0.08),
      title: MText(proxy.fullName),
      titleTextStyle: textTheme.bodyMedium,
      leading: MIconButton(
        icon: const Icon(MIcons.chevron_left),
        color: colors.primary,
        onPressed: context.pop,
      ),
      centerTitle: true,
      actions: [
        MIconButton(
          icon: const Icon(MIcons.dots_horizontal),
          color: colors.primary,
          onPressed: () => ProfilePageOptionsModal.show(context),
        ),
        Spaces.horizontalLarge,
      ],
      flexibleSpace: SafeArea(
        bottom: false,
        child: UserProfileHeaderContent(proxy: proxy),
      ),
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
            ],
          ),
        ),
      ),
    );
  }
}

class _RecentBroadcastsTab extends WatchingWidget {
  const _RecentBroadcastsTab({required this.feed});

  final BroadcastFeedDataSource feed;

  @override
  Widget build(BuildContext context) {
    return FeedWidget(
      feedSource: feed,
      padding: const .all(16),
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
}

class _AllBroadcastsTab extends WatchingWidget {
  const _AllBroadcastsTab({required this.feed});

  final BroadcastFeedDataSource feed;

  @override
  Widget build(BuildContext context) {
    return FeedWidget(
      feedSource: feed,
      padding: const .all(16),
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
}

class _EmptyPage extends StatelessWidget {
  const _EmptyPage();

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: MAppBar.secondary(
        title: 'No Profile',
        centerTitle: true,
        actions: const [Icon(MIcons.dots_horizontal), Spaces.horizontalLarge],
      ),
      body: const MenoEmptyWidget(),
    );
  }
}
