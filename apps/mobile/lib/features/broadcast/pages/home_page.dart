import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/bible/bible.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/notification/notification.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_ui/meno_ui.dart';

class HomePage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    // The seed runs from `main()`, so it can finish long before this page is
    // ever built — while the sign-in screen was still up. `executeImmediately`
    // calls the handler with the value the flag already holds, so the notice
    // lands on the first home frame either way; `acknowledgeDownloaded` clears
    // it, so returning to this tab does not repeat it.
    registerHandler<BibleSeedService, bool>(
      select: (seed) => seed.freshlySeeded,
      executeImmediately: true,
      handler: (context, freshlySeeded, cancel) {
        if (!freshlySeeded) return;
        // Showing a snackbar mutates the messenger's own state, which must not
        // happen during this build: the notice is queued for the end of it.
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (!context.mounted) return;
          di<BibleSeedService>().acknowledgeDownloaded();
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text(_kBibleSeededNotice)),
          );
        });
      },
    );

    final profile = watchValue<MyProfileManager, MyProfile>(
      (manager) => manager.profile,
    );

    // So, this is the check or flag I am using for now to determine which
    // section to show - the for-you feed or the now-live feed.
    //
    // For now, I am checking if the currently authenticated user is
    // following at least one account, if they are, then we show live broadcasts
    // from the for-you feed, else, we show the now-live feed.
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
            // Under the live banner, and never beside it: the scheduled card
            // hides itself while a room is open.
            const ScheduledBroadcastBanner(),
            Spaces.verticalXLarge,
            if (isNewUser)
              _HomeSection(
                title: 'Live For You',
                icon: Assets.images.sparkles.image(
                  height: 24,
                  width: 24,
                  package: 'meno_ui',
                ),
                query: const BroadcastQuery.forYou(),
                skeleton: BroadcastCard.skeletonLive,
                itemBuilder: (context, broadcast) => BroadcastCard.nLive(
                  broadcast,
                  key: ValueKey(broadcast.id),
                  onTap: () => PreStreamModal.show(context, broadcast.id),
                ),
              )
            else
              _HomeSection(
                title: 'Now Live',
                icon: Assets.images.flame.image(
                  height: 24,
                  width: 24,
                  package: 'meno_ui',
                ),
                query: const BroadcastQuery.nowLive(),
                skeleton: BroadcastCard.skeletonLive,
                itemBuilder: (context, broadcast) => BroadcastCard.nLive(
                  broadcast,
                  key: ValueKey(broadcast.id),
                  onTap: () => PreStreamModal.show(context, broadcast.id),
                ),
              ),
            Spaces.verticalXXLarge,
            _HomeSection(
              title: 'Recently Live',
              icon: Assets.images.highVoltage.image(
                height: 24,
                width: 24,
                package: 'meno_ui',
              ),
              query: const BroadcastQuery.recentlyLive(),
              skeleton: BroadcastCard.skeletonRecentlyLive,
              itemBuilder: (context, broadcast) => BroadcastCard.rLive(
                broadcast,
                key: ValueKey(broadcast.id),
                onTap: () async {
                  await context.push(MRoutes.broadcastRoute(broadcast.id));
                },
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// What the one-shot notice says once this launch has put the bundled
/// translations in the store.
///
/// The KJV is named because it is the translation the reader opens on and the
/// one a first run always writes; the run seeds every bundled translation.
const String _kBibleSeededNotice =
    'King James Version Bible downloaded successfully';

class _AppBar extends WatchingWidget implements PreferredSizeWidget {
  const new();

  @override
  Size get preferredSize => ToolBarHeights.home;

  @override
  Widget build(BuildContext context) {
    // One presentation source: the ready account's own profile manager. The
    // stored auth user is no longer consulted, so a name edit on the profile
    // screen reaches this chrome without a restart (M12).
    final profile = watchValue<MyProfileManager, MyProfile>(
      (manager) => manager.profile,
    );

    final isLoading = watchValue<MyProfileManager, bool>(
      (manager) => manager.isLoading,
    );

    // An account with nothing on disk yet greets with a bone rather than with
    // "Hello," trailing off: the name is on its way.
    final isGreetingPending = isLoading && profile.fullName.orEmpty.isEmpty;

    final unreadCount = watchValue<NotificationsManager, int>(
      (manager) => manager.unreadCount,
    );

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
                    if (isGreetingPending)
                      const MShimmer(height: 14, width: 96)
                    else
                      MText(
                        profile.fullName.orEmpty,
                        style: textTheme.subheadingMedium,
                      ),
                  ],
                ),
              ),
              Spaces.horizontalSmall,
              Row(
                children: [
                  Badge(
                    isLabelVisible: unreadCount > 0,
                    label: Text('$unreadCount'),
                    backgroundColor: colors.error,
                    textColor: colors.onError,
                    child: MIconButton(
                      icon: const Icon(MIcons.bell),
                      iconSize: 20,
                      onPressed: () => context.push(MRoutes.notifications),
                    ),
                  ),
                  const SizedBox(width: 24),
                  CurrentUserAvatar(onTap: () => context.go(MRoutes.myProfile)),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// One horizontal broadcast strip on Home.
///
/// The three sections were one shape with five values — heading, icon, query,
/// skeleton and card — and three copies of the same feed wiring. A section is
/// that shape and those values; the query is also what "See all" carries, so
/// the link cannot drift from the list it sits above.
class _HomeSection extends WatchingWidget {
  const new({
    required this.title,
    required this.icon,
    required this.query,
    required this.skeleton,
    required this.itemBuilder,
  });

  /// The heading, and the name of the section "See all" opens.
  final String title;

  /// The picture beside the heading.
  final Widget icon;

  /// What the strip lists.
  final BroadcastQuery query;

  /// The shape each card has while the first page is loading.
  final Widget skeleton;

  /// Builds one card, including what tapping it does.
  final Widget Function(BuildContext context, Broadcast broadcast) itemBuilder;

  @override
  Widget build(BuildContext context) {
    final feedSource = createOnce(() {
      return BroadcastFeedDataSource(
        http: di<BroadcastHttpService>(),
        socket: di<BroadcastSocketService>(),
        query: query,
        fetchMoreEnabled: false,
      );
    });

    return MSection(
      title: title,
      titleIcon: icon,
      onSeeAll: () => context.pushNamed(
        MRoutes.broadcasts,
        queryParameters: query.toRouterParams(),
      ),
      builder: (context) => FeedWidget(
        feedSource: feedSource,
        horizontalItemExtent: 148,
        layout: .horizontalList,
        skeletonItem: skeleton,
        skeletonItemCount: 4,
        padding: const .symmetric(horizontal: 16),
        itemBuilder: itemBuilder,
      ),
    );
  }
}
