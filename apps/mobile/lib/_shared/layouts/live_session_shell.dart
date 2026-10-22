import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// The chrome a live broadcast sits inside: one tab per room screen, above a
/// navigation shell that owns the pages themselves.
///
/// # Why the tabs and the shell are two things
///
/// `go_router`'s `StatefulShellRoute` keeps one branch per tab, so a room's
/// chat, bible and notes each hold their own state. The [TabController] here is
/// only the *presentation* of that: it drives the tab bar and the swipeable
/// page view, and a movement in either is mirrored into the other. Keeping them
/// in step is this widget's whole job — a shell that let them drift would swipe
/// to one screen and highlight another.
///
/// # Leaving
///
/// A session ends for two reasons: the host ended it, or the room did. Both
/// arrive on the manager's `state`, and both leave the client on the page that
/// says what happened, because the room it was in no longer exists. Stepping
/// out of the room is not this widget's business: the session lives in a scope,
/// so changing the location only changes which screen is drawn.
class LiveSessionShell extends WatchingStatefulWidget {
  const new _({
    required this.navigationShell,
    required this.children,
    super.key,
  });

  /// The shell builder `go_router` calls with the branches it built.
  static Widget builder(
    BuildContext context,
    StatefulNavigationShell navigationShell,
    List<Widget> children,
  ) => LiveSessionShell._(
    key: const ValueKey<String>('LiveSessionShell'),
    navigationShell: navigationShell,
    children: children,
  );

  final StatefulNavigationShell navigationShell;
  final List<Widget> children;

  @override
  State<LiveSessionShell> createState() => LiveSessionShellState();
}

class LiveSessionShellState extends State<LiveSessionShell>
    with SingleTickerProviderStateMixin {
  late final TabController controller;

  @override
  void initState() {
    super.initState();
    controller = TabController(
      length: widget.children.length,
      vsync: this,
      initialIndex: widget.navigationShell.currentIndex,
    );
    controller.addListener(_handleTabChanged);

    if (controller.index != widget.navigationShell.currentIndex) {
      controller.index = widget.navigationShell.currentIndex;
    }
  }

  @override
  void dispose() {
    controller.removeListener(_handleTabChanged);
    controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final manager = di<LiveSessionManager>();

    registerHandler<LiveSessionManager, LiveSessionState>(
      select: (manager) => manager.state,
      handler: (context, state, _) {
        if (state is LiveSessionEnded) {
          context.go(MRoutes.endedBroadcastRoute(manager.broadcast.value.id));
        }
      },
    );

    return PopScope(
      onPopInvokedWithResult: (_, _) => context.go(MRoutes.home),
      child: Scaffold(
        appBar: _AppBar(
          controller: controller,
          showChatDot: false,
          onTabChanged: onTabTap,
        ),
        body: MTabBarView(
          controller: controller,
          children: widget.children,
          onPageChanged: (_) => FocusScope.of(context).unfocus(),
        ),
      ),
    );
  }

  void _handleTabChanged() {
    if (controller.index == widget.navigationShell.currentIndex) return;
    widget.navigationShell.goBranch(controller.index);
  }

  void onTabTap(int tappedIndex) {
    if (controller.index != tappedIndex) {
      controller.animateTo(tappedIndex);
    } else {
      if (widget.navigationShell.currentIndex != tappedIndex) {
        widget.navigationShell.goBranch(tappedIndex);
      }
    }
  }
}

/// The room's tab bar, in the design system's normal (underline) style.
///
/// It is deliberately not the chip bar used elsewhere: the four room screens
/// are a fixed, equal-width set, and the normal bar is the one the design
/// draws for them.
class _AppBar extends StatelessWidget implements PreferredSizeWidget {
  const new({
    required this.controller,
    required this.showChatDot,
    this.onTabChanged,
  });

  final TabController controller;
  final bool showChatDot;
  final void Function(int)? onTabChanged;

  @override
  Size get preferredSize => const Size.fromHeight(kToolbarHeight);

  @override
  Widget build(BuildContext context) {
    Widget icon = const SizedBox.shrink();
    if (showChatDot) {
      icon = Center(
        child: Container(
          width: 8,
          height: 8,
          decoration: const BoxDecoration(color: Colors.red, shape: .circle),
        ),
      );
    }
    return SafeArea(
      child: Padding(
        padding: const .symmetric(horizontal: Insets.lg),
        child: MTabBar.normal(
          controller: controller,
          tabs: [
            const MenoTab(text: 'Broadcast'),
            MenoTab(text: 'Chat', icon: icon),
            const MenoTab(text: 'Live Bible'),
            const MenoTab(text: 'Live Notes'),
          ],
          padding: .zero,
          isScrollable: false,
          onTap: onTabChanged,
        ),
      ),
    );
  }
}
