import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/model/_model.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/profile/manager/_manager.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';

/// The height the screen's tab bar occupies under the toolbar.
const double _kTabBarHeight = 32;

/// The signed-in account's profile.
///
/// The record itself lives in [MyProfileManager], which is registered once and
/// follows the active account, so this screen only has to read it: whatever is
/// on disk for the account shows immediately and the fetch that follows
/// replaces it. Reading is started here rather than at startup because a
/// signed-in account that never opens this screen has no reason to be asked
/// for.
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

    final isFetching = watchValue<MyProfileManager, bool>(
      (manager) => manager.fetch.isRunning,
    );

    // The vault already knows who signed in, so the title can greet them while
    // the record is still on its way instead of saying "Unknown User".
    final title = switch (profile.fullName) {
      final name when name.isNotEmpty => name,
      _ when signedIn.fullName.isNotEmpty => signedIn.fullName,
      _ => 'Unknown User',
    };

    // Opening the screen reads; so does switching accounts while it is open —
    // the manager has already put the new account's cached profile in place,
    // and this is what replaces it with its record.
    callOnce((_) => manager.fetch.run());

    registerHandler<UserManager, String?>(
      select: (auth) => auth.currentUserId,
      handler: (ctx, _, _) => di<MyProfileManager>().fetch.run(),
    );

    if (profile.isEmpty && !isFetching) {
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

    return DefaultTabController(
      length: 3,
      child: Scaffold(
        // Switching accounts rebuilds the screen, so the tabs of the previous
        // account are not left selected for the new one.
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
                isSkeleton: profile.isEmpty,
                innerBoxIsScrolled: innerBoxIsScrolled,
              ),
            ],
            body: const TabBarView(
              children: [
                // Every tab is a feed owned by another feature — recent
                // broadcasts, all broadcasts, favourites — so each shows the
                // Meno mark until that feature exists.
                ProfileTabPlaceholder(injectOverlap: true),
                ProfileTabPlaceholder(injectOverlap: true),
                ProfileTabPlaceholder(injectOverlap: true),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The screen's collapsible header: the account's name and switcher, the
/// header content, and the tab bar that stays pinned under the toolbar.
class _ProfileAppBar extends StatelessWidget {
  const new({
    required this.title,
    required this.profile,
    required this.isSkeleton,
    required this.innerBoxIsScrolled,
  });

  final String title;
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
                Tab(text: 'Favorites'),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The account's name in the toolbar, and the way to another account.
class _ProfileTitle extends StatelessWidget {
  const new({required this.title});

  final String title;

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
              title,
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
