import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_ui/meno_ui.dart';

/// The height the screen's tab bar occupies under the toolbar.
const double _kTabBarHeight = 32;

/// The signed-in account's profile.
///
/// The record itself lives in [MyProfileManager], which is registered once,
/// follows the active account and reads that account's record as it becomes
/// active, so this screen only has to read it: whatever is on disk for the
/// account shows immediately and the read already on its way replaces it. The
/// read it starts itself is the one a retry or a pull to refresh asks for.
class MyProfilePage extends WatchingWidget {
  /// Creates the page.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    final scrollCtrl = createOnce(ScrollController.new);

    final manager = di<MyProfileManager>();

    final profile = watchValue<MyProfileManager, MyProfile>(
      (manager) => manager.profile,
    );

    final signedIn = watchValue<UserManager, User>(
      (manager) => manager.currentUser,
    );

    final isLoading = watchValue<MyProfileManager, bool>(
      (manager) => manager.isLoading,
    );

    final hasProfile = profile.id.isValid;

    // The vault already knows who signed in, so the title can greet them while
    // the record is still on its way instead of saying "Unknown User".
    final title = switch (profile.fullName) {
      final name when name.isValid => name,
      _ when signedIn.fullName.isValid => signedIn.fullName,
      _ => FullName('Unknown User'),
    };

    if (!hasProfile && !isLoading) {
      return MScaffold(
        padding: .zero,
        body: Center(
          child: MErrorState(
            message:
                "We couldn't load your profile. Please check your connection, "
                'then try again.',
            onRetry: manager.fetch.run,
          ),
        ),
      );
    }

    return _RefetchWhenShown(
      onShown: manager.refresh,
      child: DefaultTabController(
        length: 4,
        child: Scaffold(
          key: ValueKey(signedIn.id),
          backgroundColor: colors.background,
          body: RefreshIndicator(
            onRefresh: manager.fetch.runAsync,
            child: NestedScrollView(
              controller: scrollCtrl,
              headerSliverBuilder: (context, innerBoxIsScrolled) => [
                _ProfileAppBar(
                  title: title,
                  profile: profile,
                  isSkeleton: !profile.fullName.isValid,
                  innerBoxIsScrolled: innerBoxIsScrolled,
                ),
              ],
              // Each tab is its own scrollable, and a pull anywhere over one is
              // answered by that tab's own indicator — so the header read is
              // handed to every one of them. The page-level indicator above
              // only ever fires for a pull on the header itself.
              body: TabBarView(
                children: [
                  _RecentBroadcastsTab(onRefresh: manager.fetch.runAsync),
                  _AllBroadcastsTab(onRefresh: manager.fetch.runAsync),
                  _ScheduledBroadcastsTab(onRefresh: manager.fetch.runAsync),
                  _FavouritesTab(onRefresh: manager.fetch.runAsync),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _ProfileAppBar extends StatelessWidget {
  const new({
    required this.title,
    required this.profile,
    required this.isSkeleton,
    required this.innerBoxIsScrolled,
  });

  final FullName title;
  final MyProfile profile;
  final bool isSkeleton;
  final bool innerBoxIsScrolled;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return SliverOverlapAbsorber(
      handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
      sliver: MSliverAppBar(
        toolbarHeight: kToolbarHeight + 8,
        backgroundColor: colors.background,
        surfaceTintColor: Colors.transparent,
        pinned: true,
        forceElevated: innerBoxIsScrolled,
        elevation: innerBoxIsScrolled ? 1 : 0,
        shadowColor: colors.onBackground.withValues(alpha: 0.08),
        title: _ProfileTitle(title: title),
        actions: [
          MIconButton(
            icon: const Icon(MIcons.settings),
            color: colors.primary,
            onPressed: () => context.push(MRoutes.settings),
          ),
          Spaces.horizontalLarge,
        ],
        flexibleSpace: MyProfileHeaderContent(
          profile: profile,
          isSkeleton: isSkeleton,
        ),
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
                Tab(text: 'Scheduled broadcasts'),
                Tab(text: 'Favorites'),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _ProfileTitle extends StatelessWidget {
  const new({required this.title});

  final FullName title;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () => SwitchAccountSheet.show(context),
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
              title.orEmpty,
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

/// [_RecentBroadcastsTab] displays the user's recent broadcasts.
class _RecentBroadcastsTab extends WatchingStatefulWidget {
  const new({required this.onRefresh});

  /// Also run by a pull: this screen's header comes from the same API.
  final Future<void> Function() onRefresh;

  @override
  State<_RecentBroadcastsTab> createState() => __RecentBroadcastsTabState();
}

class __RecentBroadcastsTabState extends State<_RecentBroadcastsTab>
    with AutomaticKeepAliveClientMixin {
  @override
  Widget build(BuildContext context) {
    super.build(context);

    final feedSource = createOnce(() {
      final creatorId = di<UserManager>().currentUserId.value;
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: BroadcastQuery.recentlyLive(creatorId: creatorId?.getOrNull),
      );
    });

    return FeedWidget(
      feedSource: feedSource,
      onRefresh: widget.onRefresh,
      padding: const .all(Insets.lg),
      topSlivers: [
        SliverOverlapInjector(
          handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
        ),
      ],
      skeletonItem: BroadcastCard.skeletonRecentlyLiveTile,
      skeletonItemCount: 4,
      itemBuilder: (context, broadcast) {
        return BroadcastCard.rLiveTile(
          broadcast,
          key: ValueKey(broadcast.id),
          onTap: () async {
            await context.push(MRoutes.broadcastRoute(broadcast.id));
          },
        );
      },
    );
  }

  @override
  bool get wantKeepAlive => true;
}

/// [_AllBroadcastsTab] displays the user's all broadcasts feed.
class _AllBroadcastsTab extends WatchingStatefulWidget {
  const new({required this.onRefresh});

  /// Also run by a pull: this screen's header comes from the same API.
  final Future<void> Function() onRefresh;

  @override
  State<_AllBroadcastsTab> createState() => __AllBroadcastsTabState();
}

class __AllBroadcastsTabState extends State<_AllBroadcastsTab>
    with AutomaticKeepAliveClientMixin {
  @override
  Widget build(BuildContext context) {
    super.build(context);

    final feedSource = createOnce(() {
      final creatorId = di<UserManager>().currentUserId.value;
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: BroadcastQuery(creatorId: creatorId?.getOrNull),
        // A schedule has a tab of its own. Leaving it here meant a broadcast
        // that had not started yet sat among ones that had, with a relative
        // "A while back" and no end time to measure it by.
        excludeScheduled: true,
      );
    });

    return FeedWidget(
      feedSource: feedSource,
      onRefresh: widget.onRefresh,
      padding: const .all(Insets.lg),
      topSlivers: [
        SliverOverlapInjector(
          handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
        ),
      ],
      skeletonItem: BroadcastCard.skeletonRecentlyLiveTile,
      skeletonItemCount: 4,
      itemBuilder: (context, broadcast) {
        return BroadcastCard.rLiveTile(
          broadcast,
          key: ValueKey(broadcast.id),
          onTap: () async {
            await context.push(MRoutes.broadcastRoute(broadcast.id));
          },
        );
      },
    );
  }

  @override
  bool get wantKeepAlive => false;
}

/// [_ScheduledBroadcastsTab] displays the broadcasts waiting for their start.
///
/// One at a time, by the API's rule, so this list has right of way in showing
/// the account's next promise: tapping one opens its lobby, where the host can
/// go live, and the same three actions are on the home banner.
class _ScheduledBroadcastsTab extends WatchingStatefulWidget {
  const new({required this.onRefresh});

  /// Also run by a pull: this screen's header comes from the same API.
  final Future<void> Function() onRefresh;

  @override
  State<_ScheduledBroadcastsTab> createState() =>
      __ScheduledBroadcastsTabState();
}

class __ScheduledBroadcastsTabState extends State<_ScheduledBroadcastsTab>
    with AutomaticKeepAliveClientMixin {
  @override
  Widget build(BuildContext context) {
    super.build(context);

    final feedSource = createOnce(() {
      final creatorId = di<UserManager>().currentUserId.value;
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: BroadcastQuery.scheduled(creatorId: creatorId?.getOrNull),
      );
    });

    return FeedWidget(
      feedSource: feedSource,
      onRefresh: widget.onRefresh,
      padding: const .all(Insets.lg),
      topSlivers: [
        SliverOverlapInjector(
          handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
        ),
      ],
      skeletonItem: BroadcastCard.skeletonRecentlyLiveTile,
      skeletonItemCount: 2,
      itemBuilder: (context, broadcast) {
        return BroadcastCard.rLiveTile(
          broadcast,
          key: ValueKey(broadcast.id),
          // The row's job is to resolve the promise, not to open the lobby:
          // once the home banner is hidden this is the only place a schedule
          // can be cancelled, and the slot is held until it is. The same sheet
          // the banner opens, so both surfaces offer one set of answers.
          onTap: () => ScheduledBroadcastActionsModal.show(context, broadcast),
        );
      },
    );
  }

  @override
  bool get wantKeepAlive => false;
}

/// [_FavouritesTab] displays the user's favourite broadcasts.
class _FavouritesTab extends WatchingStatefulWidget {
  const new({required this.onRefresh});

  /// Also run by a pull: this screen's header comes from the same API.
  final Future<void> Function() onRefresh;

  @override
  State<_FavouritesTab> createState() => __FavouritesTabState();
}

class __FavouritesTabState extends State<_FavouritesTab>
    with AutomaticKeepAliveClientMixin {
  @override
  Widget build(BuildContext context) {
    super.build(context);

    final favourites = watchValue<FavouritesManager, List<FavouriteBroadcast>>(
      (manager) => manager.favourites,
    );

    if (favourites.isEmpty) {
      return const MEmptyState(title: 'No favourite broadcasts yet');
    }

    return RefreshIndicator(
      onRefresh: widget.onRefresh,
      child: CustomScrollView(
        // This tab has no feed of its own, so a short list must still be
        // pullable.
        physics: const AlwaysScrollableScrollPhysics(),
        slivers: [
          SliverOverlapInjector(
            handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
          ),
          SliverPadding(
            padding: const .all(Insets.lg),
            sliver: SliverList.separated(
              separatorBuilder: (_, _) => Spaces.verticalLarge,
              itemCount: favourites.length,
              itemBuilder: (ctx, index) {
                final favourite = favourites[index];
                return MRecentlyLiveListTile(
                  title: favourite.title.orEmpty,
                  creator: favourite.creatorName.orEmpty,
                  imageUrl: favourite.image?.getUrl,
                  onTap: () async {
                    await ctx.push(
                      MRoutes.broadcastRoute(favourite.broadcastId),
                    );
                  },
                );
              },
            ),
          ),
        ],
      ),
    );
  }

  @override
  bool get wantKeepAlive => false;
}

/// Runs [onShown] each time the page comes back on screen.
///
/// The profile is a shell branch, so it stays alive behind the other tabs and
/// nothing rebuilds it when it is selected again. go_router keeps an inactive
/// branch mounted but disables its tickers, which makes [TickerMode] the exact
/// signal for "this tab is the one being looked at": the account's counters are
/// read again on every return to it, so a follow that happened elsewhere in the
/// app — or somebody following this account — is on screen without a pull.
///
/// The first arrival is skipped: the read started when the account became ready
/// is already on its way.
class _RefetchWhenShown extends StatefulWidget {
  const new({required this.onShown, required this.child});

  final Future<void> Function() onShown;
  final Widget child;

  @override
  State<_RefetchWhenShown> createState() => _RefetchWhenShownState();
}

class _RefetchWhenShownState extends State<_RefetchWhenShown> {
  bool? _wasShown;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();

    final isShown = TickerMode.valuesOf(context).enabled;
    final wasShown = _wasShown;
    _wasShown = isShown;

    if (!isShown || wasShown != false) return;

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      widget.onShown().ignore();
    });
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
