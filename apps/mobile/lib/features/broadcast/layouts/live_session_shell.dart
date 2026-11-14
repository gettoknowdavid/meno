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
/// # Entering and leaving
///
/// Visibility is the manager's [LiveSnapshot.phase], never a container lookup:
/// the shell is drawn for every phase that still has a room (including one that
/// failed to connect, which must be able to say so) and steps out for `idle`
/// and `ended`. When the broadcast ends, whether by the host or by the server,
/// the shell hands the client to the ended screen with the broadcast it
/// retained.
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
    final snapshot = watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );

    // The unread dot is the chat's own news, so it is read from the room's
    // chat manager rather than inferred here. A fallback notifier is watched
    // when the room has no chat yet, so the number of watches does not change
    // between builds — watch_it keys its subscriptions by call order.
    final chatList = di<ActiveBroadcastManager>().session?.chatList;
    final hasUnread = watch(chatList?.hasUnread ?? _noUnread).value;

    if (snapshot.phase == LivePhase.idle || snapshot.phase == LivePhase.ended) {
      final endedId = snapshot.endedSummary?.id;
      callOnceAfterThisBuild((_) {
        if (!context.mounted) return;
        if (endedId != null && endedId.isValid) {
          context.go(MRoutes.endedBroadcastRoute(endedId));
        } else {
          context.go(MRoutes.home);
        }
      });
      return const SizedBox.shrink();
    }

    return PopScope(
      onPopInvokedWithResult: (_, _) => context.go(MRoutes.home),
      child: Scaffold(
        appBar: _AppBar(
          controller: controller,
          showChatDot: hasUnread && controller.index != _chatTabIndex,
          onTabChanged: onTabTap,
        ),
        // The invitations float over the room: a co-host may be asked while
        // watching, and the answer must not cost them the tab they are on.
        body: Stack(
          children: [
            Positioned.fill(
              child: MTabBarView(
                controller: controller,
                children: widget.children,
                onPageChanged: (_) => FocusScope.of(context).unfocus(),
              ),
            ),
            const Positioned(
              top: Insets.sm,
              left: Insets.lg,
              right: Insets.lg,
              child: CohostInvitationList(),
            ),
          ],
        ),
      ),
    );
  }

  void _handleTabChanged() {
    _syncChatVisibility();
    if (controller.index == widget.navigationShell.currentIndex) return;
    widget.navigationShell.goBranch(controller.index);
  }

  /// Tells the chat whether it is the screen being looked at.
  ///
  /// Called from the tab controller rather than from the chat tab itself:
  /// the chat is built before it is shown, and what decides whether a message
  /// is unread news is which tab is in front, not which widget exists.
  void _syncChatVisibility() {
    di<ActiveBroadcastManager>().session?.chatList?.setVisible(
      controller.index == _chatTabIndex,
    );
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

/// Where the chat sits in the room's tab bar.
const int _chatTabIndex = 1;

/// The notifier watched while the room has no chat to report on.
final ValueNotifier<bool> _noUnread = ValueNotifier<bool>(false);

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
