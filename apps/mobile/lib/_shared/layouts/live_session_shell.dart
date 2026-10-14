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
/// only the *presentation* of that: it drives the chip bar and the swipeable
/// page view, and a movement in either is mirrored into the other. Keeping them
/// in step is this widget's whole job — a shell that let them drift would swipe
/// to one screen and highlight another.
///
/// # Leaving
///
/// A session ends for two reasons: the host ended it, or the room did. Both
/// arrive on the manager's `state`, and both leave the client on the page that
/// says what happened, because the room it was in no longer exists.
class LiveSessionShell extends WatchingStatefulWidget {
  const new({required this.navigationShell, required this.children, super.key});

  /// The shell builder `go_router` calls with the branches it built.
  static Widget builder(
    BuildContext context,
    StatefulNavigationShell navigationShell,
    List<Widget> children,
  ) => LiveSessionShell(
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
    final status = watchValue<LiveSessionManager, LiveStatus>(
      (manager) => manager.status,
    );

    final broadcast = watchValue<LiveSessionManager, Broadcast>(
      (manager) => manager.broadcast,
    );

    registerHandler<LiveSessionManager, LiveSessionState>(
      select: (manager) => manager.state,
      handler: (context, state, _) {
        if (state is LiveSessionEnded) {
          context.go(
            '${MRoutes.broadcasts}/${manager.broadcast.value.id}/ended',
          );
        }
      },
    );

    return PopScope(
      // Backing out of a live room leaves it rather than parking a dead session
      // behind the home tab.
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) context.go(MRoutes.home);
      },
      child: _LiveSessionScaffold(
        title: broadcast.title,
        status: status,
        controller: controller,
        children: widget.children,
      ),
    );
  }

  void _handleTabChanged() {
    if (controller.index == widget.navigationShell.currentIndex) return;
    widget.navigationShell.goBranch(controller.index);
  }
}

/// The room's header, its tabs, and the pages underneath.
class _LiveSessionScaffold extends StatelessWidget {
  const new({
    required this.title,
    required this.status,
    required this.controller,
    required this.children,
  });

  final String title;
  final LiveStatus status;
  final TabController controller;
  final List<Widget> children;

  /// The room's screens, in the order `routes.dart` mounts them.
  static const List<String> _tabs = [
    'Broadcast',
    'Chat',
    'Live Bible',
    'Live Notes',
  ];

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    return Scaffold(
      body: SafeArea(
        child: Column(
          children: [
            Padding(
              padding: const .symmetric(
                horizontal: Insets.lg,
                vertical: Insets.md,
              ),
              child: Row(
                children: [
                  Expanded(
                    child: MText(
                      title.isEmpty ? status.label : title,
                      style: textTheme.bodyMedium,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  if (status.isInitializing || status.showReconnecting)
                    const MLoadingIndicator.two(),
                ],
              ),
            ),
            MChipTabBar(
              controller: controller,
              tabs: [
                for (var index = 0; index < _tabs.length; index++)
                  MChipTab(
                    label: _tabs[index],
                    index: index,
                    controller: controller,
                  ),
              ],
            ),
            Expanded(
              child: MTabBarView(
                controller: controller,
                children: children,
                onPageChanged: (_) => FocusScope.of(context).unfocus(),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
