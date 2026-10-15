import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/profile/manager/_manager.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/services/_services.dart';
import 'package:meno_mobile/features/profile/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';

/// The height the screen's tab bar occupies under the toolbar.
const double _kTabBarHeight = 32;

/// Somebody else's profile, addressed by their account id.
///
/// The page owns its manager: a scope is pushed for the visit, so leaving and
/// coming back reads the cache and refetches instead of showing whatever an
/// earlier visit left behind.
class UserProfilePage extends WatchingWidget {
  /// Creates the page for [userIdStr].
  const new({required this.userIdStr, super.key});

  /// The account id from the route.
  final String userIdStr;

  @override
  Widget build(BuildContext context) {
    pushScope(
      init: (getIt) {
        getIt.registerSingleton<UserProfileManager>(
          UserProfileManager(
            userId: userIdStr,
            http: di<ProfileHttpService>(),
            cache: di<ProfileCacheService>(),
            profiles: di<UserProfileRepository>(),
          ),
        );
      },
    );

    registerHandler<UserProfileManager, CommandError<void>?>(
      select: (manager) => manager.fetch.errors,
      handler: (ctx, e, _) => ctx.showErrorToast(errorMessage(e?.error)),
    );

    final manager = di<UserProfileManager>();

    // The proxy, not a value: watching it is what makes an optimistic follow
    // repaint this screen, and what keeps it in step with any card behind it
    // that already holds the same proxy.
    final profile = watch(manager.profile);
    final isFetching = watch(manager.fetch.isRunning).value;

    // A refused follow is the visitor's business, so it is reported where they
    // are looking; the proxy has already rolled the label back by then.
    registerHandler<UserProfileProxy, CommandError<void>?>(
      select: (m) => m.toggleFollow.errors,
      handler: (ctx, e, _) => ctx.showErrorToast(errorMessage(e?.error)),
    );

    callOnce((_) => manager.fetch.run());

    if (profile.isEmpty) {
      // Nothing cached and nothing answered yet: the page waits rather than
      // showing a name it does not have.
      if (isFetching) return const MLoadingPage();
      return const _EmptyPage();
    }

    return _Content(profile: profile, onRefresh: manager.fetch.runAsync);
  }
}

/// The profile itself: the collapsible header, then the tab bar.
class _Content extends WatchingWidget {
  const new({required this.profile, required this.onRefresh});

  final UserProfileProxy profile;
  final Future<void> Function() onRefresh;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    final recentBroadcastsFeed = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: BroadcastQuery.recentlyLive(creatorId: profile.id),
      );
    });

    final allBroadcastsFeed = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: BroadcastQuery(creatorId: profile.id),
      );
    });

    return DefaultTabController(
      length: 2,
      child: Scaffold(
        backgroundColor: colors.background,
        body: RefreshIndicator(
          onRefresh: onRefresh,
          child: NestedScrollView(
            physics: const AlwaysScrollableScrollPhysics(),
            headerSliverBuilder: (context, innerBoxIsScrolled) => [
              _ProfileAppBar(
                profile: profile,
                innerBoxIsScrolled: innerBoxIsScrolled,
              ),
            ],
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

/// What a visitor sees when there is no profile to show.
class _EmptyPage extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: MAppBar.secondary(
        title: 'No Profile',
        centerTitle: true,
        actions: const [Icon(MIcons.dots_horizontal), Spaces.horizontalLarge],
      ),
      body: const MEmptyState(),
    );
  }
}

/// The header of a visited profile: back button, name, overflow menu, the
/// header content and the pinned tab bar.
class _ProfileAppBar extends StatelessWidget {
  const new({required this.profile, required this.innerBoxIsScrolled});

  final UserProfileProxy profile;
  final bool innerBoxIsScrolled;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return MSliverAppBar(
      toolbarHeight: kToolbarHeight,
      backgroundColor: colors.background,
      surfaceTintColor: Colors.transparent,
      pinned: true,
      forceElevated: innerBoxIsScrolled,
      elevation: innerBoxIsScrolled ? 1 : 0,
      shadowColor: colors.onBackground.withValues(alpha: 0.08),
      title: MText(profile.fullName),
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
      flexibleSpace: UserProfileHeaderContent(profile: profile),
      bottom: PreferredSize(
        preferredSize: const .fromHeight(_kTabBarHeight),
        child: Container(
          color: colors.background,
          height: _kTabBarHeight,
          child: TabBar(
            padding: const .symmetric(horizontal: Insets.lg),
            labelStyle: textTheme.captionMedium,
            isScrollable: true,
            tabAlignment: .start,
            labelPadding: const .symmetric(horizontal: Insets.lg),
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

class _AllBroadcastsTab extends WatchingWidget {
  const new({required this.feed});

  final BroadcastFeedDataSource feed;

  @override
  Widget build(BuildContext context) {
    return FeedWidget(
      feedSource: feed,
      padding: const .all(Insets.lg),
      skeletonItem: BroadcastCard.skeletonRecentlyLiveTile,
      skeletonItemCount: 4,
      itemBuilder: (context, broadcast) {
        return BroadcastCard.rLiveTile(
          broadcast,
          key: ValueKey(broadcast.id),
          onTap: () => context.push(MRoutes.broadcastRoute(broadcast.id)),
        );
      },
    );
  }
}

class _RecentBroadcastsTab extends WatchingWidget {
  const new({required this.feed});

  final BroadcastFeedDataSource feed;

  @override
  Widget build(BuildContext context) {
    return FeedWidget(
      feedSource: feed,
      padding: const .all(Insets.lg),
      skeletonItem: BroadcastCard.skeletonRecentlyLiveTile,
      skeletonItemCount: 4,
      itemBuilder: (context, broadcast) {
        return BroadcastCard.rLiveTile(
          broadcast,
          key: ValueKey(broadcast.id),
          onTap: () => context.push(MRoutes.broadcastRoute(broadcast.id)),
        );
      },
    );
  }
}
