import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/services/permission_prompts.dart';
import 'package:meno_mobile/app/layouts/authenticated_shell.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:responsive_framework/responsive_framework.dart';

/// The breakpoints the app lays out against.
///
/// `MenoApp` installs them with `ResponsiveBreakpoints.builder`, and layouts
/// such as [RootShell] query them instead of hard-coding widths. [MOBILE] is
/// the split between the compact bottom bar and the wide side rail.
const List<Breakpoint> menoBreakpoints = [
  Breakpoint(start: 0, end: 450, name: PHONE),
  Breakpoint(start: 451, end: 600, name: MOBILE),
  Breakpoint(start: 601, end: 800, name: TABLET),
  Breakpoint(start: 801, end: 1920, name: DESKTOP),
];

/// The app's top-level tabs, in branch order.
///
/// A single list of [MDestination]s feeds both the bottom bar and the side
/// rail so the two presentations can never drift apart. The order must match
/// the router's `StatefulShellBranch` order.
const List<MDestination> _tabs = [
  MDestination(icon: Icon(MIcons.home_01), label: 'Home'),
  MDestination(icon: Icon(MIcons.compass), label: 'Discover'),
  MDestination(icon: Icon(MIcons.file_02), label: 'Notes'),
  MDestination(icon: Icon(MIcons.user), label: 'Profile'),
];

/// The root navigation layout: bottom bar on phones, side rail on wider
/// screens.
///
/// Presentation only. Taps are reported to the router through
/// [StatefulNavigationShell.goBranch], so branch state, deep links and the
/// redirect guard stay owned by go_router. The tab content is wrapped in
/// [AuthenticatedShell], which is where the live banner lives.
///
/// Example (from the router's `StatefulShellRoute`):
/// ```dart
/// builder: (_, _, navigationShell) =>
///     RootShell(navigationShell: navigationShell),
/// ```
class RootShell extends StatelessWidget {
  /// Creates the root shell for [navigationShell].
  const new({required this.navigationShell, super.key});

  /// The router's shell, whose branches this layout switches between.
  final StatefulNavigationShell navigationShell;

  @override
  Widget build(BuildContext context) {
    final index = navigationShell.currentIndex;
    final useSideNavRail = ResponsiveBreakpoints.of(context).largerThan(MOBILE);

    final content = AuthenticatedShell(child: navigationShell);

    Widget body = Row(
      children: [
        MSideNavRail(
          selectedIndex: index,
          onDestinationSelected: _onTabSelected,
          destinations: _tabs,
        ),
        Expanded(child: content),
      ],
    );
    Widget? bottomNavigationBar;

    if (!useSideNavRail) {
      body = content;
      bottomNavigationBar = MBottomNavigationBar(
        currentIndex: index,
        onTap: _onTabSelected,
        destinations: _tabs,
        centerItem: MMicrophone(
          onTap: () async {
            final isMicPerm = await PermissionPrompts.broadcast(context);
            if (isMicPerm && context.mounted) {
              await context.push(MRoutes.broadcastEditor);
            }
          },
        ),
      );
    }

    return MScaffold(
      padding: .zero,
      body: body,
      bottomNavigationBar: bottomNavigationBar,
    );
  }

  /// Switches to [index], resetting that branch when its tab is tapped again.
  void _onTabSelected(int index) => navigationShell.goBranch(
    index,
    initialLocation: index == navigationShell.currentIndex,
  );
}
