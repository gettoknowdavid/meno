import 'package:flutter/foundation.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
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

/// One tab of the root shell.
///
/// A single list feeds both the bottom bar and the side rail so the two
/// presentations can never drift apart.
class _ShellTab {
  const new({
    required this.label,
    required this.icon,
    required this.activeIcon,
  });

  final String label;
  final IconData icon;
  final IconData activeIcon;
}

/// The app's top-level tabs, in branch order.
///
/// The order must match the `StatefulShellBranch` order in the router: the
/// shell maps a tab index straight onto `navigationShell.goBranch`.
const List<_ShellTab> _tabs = [
  _ShellTab(label: 'Home', icon: MIcons.home_01, activeIcon: MIcons.home),
  _ShellTab(
    label: 'Discover',
    icon: MIcons.compass,
    activeIcon: MIcons.compass,
  ),
  _ShellTab(label: 'Notes', icon: MIcons.file_02, activeIcon: MIcons.file_02),
  _ShellTab(
    label: 'Profile',
    icon: MIcons.user,
    activeIcon: MIcons.user_circle,
  ),
];

/// The root navigation layout: a bottom bar on phones, a side rail on wider
/// screens.
///
/// The shell is presentation only. It reports taps back to the router through
/// [StatefulNavigationShell.goBranch], so branch state, deep links and the
/// redirect guard all stay owned by go_router.
///
/// Example usage (from the router's `StatefulShellRoute`):
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

    return MScaffold(
      padding: .zero,
      body: useSideNavRail
          ? Row(
              children: [
                MSideNavRail(
                  selectedIndex: index,
                  onDestinationSelected: _onTabSelected,
                  destinations: [
                    for (final tab in _tabs)
                      MSideNavRailDestination(
                        icon: Icon(tab.icon),
                        label: tab.label,
                      ),
                  ],
                ),
                Expanded(child: navigationShell),
              ],
            )
          : navigationShell,
      bottomNavigationBar: useSideNavRail
          ? null
          : MBottomNavigationBar(
              currentIndex: index,
              onTap: _onTabSelected,
              items: [
                for (final tab in _tabs)
                  BottomNavigationBarItem(
                    icon: Icon(tab.icon),
                    activeIcon: Icon(tab.activeIcon),
                    label: tab.label,
                  ),
              ],
            ),
    );
  }

  /// Switches to [index], resetting that branch when its tab is tapped again.
  void _onTabSelected(int index) => navigationShell.goBranch(
    index,
    initialLocation: index == navigationShell.currentIndex,
  );

  @override
  void debugFillProperties(DiagnosticPropertiesBuilder properties) {
    super.debugFillProperties(properties);
    properties
      ..add(
        DiagnosticsProperty<StatefulNavigationShell>(
          'navigationShell',
          navigationShell,
        ),
      )
      ..add(IntProperty('currentIndex', navigationShell.currentIndex));
  }
}
