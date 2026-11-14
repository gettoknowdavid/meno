import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/discover/discover.dart';
import 'package:meno_ui/meno_ui.dart';

/// The chrome Discover's tabs sit inside: the header, the search field and the
/// chip bar, above a navigation shell that owns the three screens.
///
/// # Why the tabs and the shell are two things
///
/// `go_router`'s `StatefulShellRoute` keeps one branch per tab, so the all,
/// now-live and recently-live lists each hold their own scroll position and
/// their own feed. The [TabController] here is only the *presentation* of that:
/// it drives the chip bar and the swipeable page view, and a movement in either
/// is mirrored into the other. The router is the authority — the selected chip 
/// is read from [Filter.fromLocation] rather than remembered — so a deep link, 
/// a "See all" from Home, and a tap all land on the same tab.
///
/// A tab is a place, not a page index: tapping a chip navigates, which is what
/// keeps the address naming the screen the person is looking at.
class DiscoverShell extends WatchingStatefulWidget {
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
  ) => DiscoverShell._(
    key: const ValueKey<String>('DiscoverShell'),
    navigationShell: navigationShell,
    children: children,
  );

  /// The shell whose branches are the tabs.
  final StatefulNavigationShell navigationShell;

  /// One navigator per tab, in [Filter] order.
  final List<Widget> children;

  @override
  State<DiscoverShell> createState() => _DiscoverShellState();
}

class _DiscoverShellState extends State<DiscoverShell>
    with SingleTickerProviderStateMixin {
  late final TabController _tabController;

  StatefulNavigationShell get _shell => widget.navigationShell;

  @override
  void initState() {
    super.initState();
    _tabController = TabController(
      initialIndex: _shell.currentIndex,
      length: Filter.values.length,
      vsync: this,
    );

    // Keep TabController → shell in sync when the user swipes.
    _tabController.addListener(_onTabChanged);
  }

  @override
  void didUpdateWidget(DiscoverShell oldWidget) {
    super.didUpdateWidget(oldWidget);
    // Keep shell → TabController in sync when navigation is driven
    // programmatically (e.g. context.go, a deep link, "See all" from Home).
    if (_tabController.index != _shell.currentIndex) {
      _tabController.index = _shell.currentIndex;
    }
  }

  void _onTabChanged() {
    // addListener fires on animation ticks too — only act on settled index.
    if (_tabController.indexIsChanging) return;
    if (_tabController.index == _shell.currentIndex) return;

    _shell.goBranch(_tabController.index);
  }

  @override
  void dispose() {
    _tabController.removeListener(_onTabChanged);
    _tabController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MScaffold(
      padding: .zero,
      appBar: AppBar(
        title: const MHeader(
          title: MText('Discover'),
          padding: .zero,
          addTopMargin: true,
        ),
        bottom: PreferredSize(
          preferredSize: const .fromHeight(110),
          child: Column(
            crossAxisAlignment: .start,
            children: [
              const DiscoverSearchBar(readOnly: true),
              Spaces.verticalXLarge,
              MChipTabBar(
                controller: _tabController,
                tabs: [
                  for (final (index, filter) in Filter.values.indexed)
                    MChipTab(
                      label: filter.label,
                      index: index,
                      controller: _tabController,
                      icon: index == 1
                          ? Assets.images.flame.image(
                              width: 16,
                              height: 16,
                              package: 'meno_ui',
                            )
                          : null,
                    ),
                ],
              ),
              Spaces.verticalMicro,
            ],
          ),
        ),
      ),
      body: TabBarView(controller: _tabController, children: widget.children),
    );
  }
}
