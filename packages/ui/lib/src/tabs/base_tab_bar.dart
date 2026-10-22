import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/spacer/meno_spacer.dart';
import 'package:meno_ui/src/tabs/meno_tab.dart';
import 'package:meno_ui/src/tabs/meno_tab_type.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_tab_bar_theme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/spacing.dart';
import 'package:skeletonizer/skeletonizer.dart';

/// {@template base_tab_bar}
/// An abstract base class for creating different styles of tab bars.
///
/// This class provides common properties and logic for displaying a list
/// of tabs and managing the active tab using a [TabController].
///
/// Subclasses can extend this class to implement specific visual styles
/// for the tab bar.
/// {@endtemplate}
abstract class BaseTabBar extends StatefulWidget {
  /// Creates a new [BaseTabBar].
  const new({
    required this.tabs,
    required this.controller,
    this.type = MenoTabType.contained,
    this.isScrollable = true,
    this.height = 32,
    this.onTap,
    this.padding = const .symmetric(horizontal: Insets.lg),
    super.key,
  });

  /// The list of tabs to display in the tab bar. Each item is a [MenoTab].
  final List<MenoTab> tabs;

  /// The [TabController] that manages the currently active tab index.
  final TabController controller;

  /// The visual style of the tab bar, either [MenoTabType.normal] or
  /// [MenoTabType.contained].
  final MenoTabType type;

  /// Whether the tab bar should be scrollable horizontally if the tabs
  /// exceed the available width.
  final bool isScrollable;

  /// The height of each tab in the tab bar.
  final double height;

  /// An optional callback that's called when the [TabBar] is tapped.
  ///
  /// The callback is applied to the index of the tab where the tap occurred.
  final void Function(int)? onTap;

  /// Padding
  final EdgeInsetsGeometry? padding;

  @override
  State<BaseTabBar> createState() => _BaseTabBarState();
}

class _BaseTabBarState extends State<BaseTabBar> {
  /// The index the visible selection was last built for.
  ///
  /// A [TabController] notifies on every animation frame while it moves. The
  /// tabs only look different once the index settles, so the bar listens for
  /// that instead of rebuilding four labels sixty times a swipe.
  late int _selectedIndex;

  @override
  void initState() {
    super.initState();
    _selectedIndex = widget.controller.index;
    widget.controller.addListener(_handleTabChanged);
  }

  @override
  void didUpdateWidget(BaseTabBar oldWidget) {
    super.didUpdateWidget(oldWidget);

    if (widget.controller != oldWidget.controller) {
      oldWidget.controller.removeListener(_handleTabChanged);
      widget.controller.addListener(_handleTabChanged);
    }
    _selectedIndex = widget.controller.index;
  }

  @override
  void dispose() {
    widget.controller.removeListener(_handleTabChanged);
    super.dispose();
  }

  void _handleTabChanged() {
    if (!mounted || widget.controller.index == _selectedIndex) return;
    setState(() => _selectedIndex = widget.controller.index);
  }

  @override
  Widget build(BuildContext context) {
    final isNormal = widget.type == MenoTabType.normal;

    final theme = MTabBarTheme.of(context);
    final styles = isNormal ? theme.normal : theme.contained;

    // Resolved once for the whole bar: every tab paints from the same two
    // states, so resolving per child would repeat the same work four times.
    const selectedState = <WidgetState>{WidgetState.selected};
    const unselectedState = <WidgetState>{};

    final selectedBackgroundColor = styles.backgroundColor.resolve(
      selectedState,
    );
    final unselectedBackgroundColor = styles.backgroundColor.resolve(
      unselectedState,
    );
    final selectedLabelColor = styles.labelColor.resolve(selectedState);
    final unselectedLabelColor = styles.labelColor.resolve(unselectedState);
    final selectedIconTheme = styles.iconTheme.resolve(selectedState);
    final unselectedIconTheme = styles.iconTheme.resolve(unselectedState);

    final length = widget.tabs.length;

    return TabBar(
      tabs: [
        for (var index = 0; index < length; index++)
          Tab(
            key: widget.tabs[index].key,
            height: widget.height,
            child: _TabChild(
              data: widget.tabs[index],
              selected: index == _selectedIndex,
              isNormal: isNormal,
              height: widget.height,
              margin: !isNormal && index != length - 1
                  ? const .only(right: Insets.lg)
                  : null,
              selectedBackgroundColor: selectedBackgroundColor,
              unselectedBackgroundColor: unselectedBackgroundColor,
              selectedLabelColor: selectedLabelColor,
              unselectedLabelColor: unselectedLabelColor,
              selectedIconTheme: selectedIconTheme,
              unselectedIconTheme: unselectedIconTheme,
            ),
          ),
      ],
      controller: widget.controller,
      onTap: widget.onTap,
      padding: widget.padding,
      isScrollable: widget.isScrollable,
      dividerColor: styles.dividerColor,
      dividerHeight: styles.dividerHeight,
      indicatorSize: .tab,
      indicatorColor: Colors.transparent,
      indicatorWeight: 0,
      splashFactory: NoSplash.splashFactory,
      tabAlignment: widget.isScrollable ? .start : .fill,
      labelPadding: .zero,
      indicator: styles.indicator,
    );
  }
}

/// A private widget used to render the individual tab content.
///
/// This widget handles the visual styling of a single tab based on its
/// selected state and the overall tab bar type. The colours it paints with
/// are resolved by the bar, so the tab itself only picks the selected or
/// unselected half of each pair.
class _TabChild extends StatelessWidget {
  /// Creates a new [_TabChild].
  const new({
    required this.data,
    required this.selected,
    required this.isNormal,
    required this.height,
    required this.selectedBackgroundColor,
    required this.unselectedBackgroundColor,
    required this.selectedLabelColor,
    required this.unselectedLabelColor,
    required this.selectedIconTheme,
    required this.unselectedIconTheme,
    this.margin,
  });

  /// The data associated with this tab.
  final MenoTab data;

  /// Whether this tab is currently selected.
  final bool selected;

  /// Whether the tab bar uses the normal (underline) style.
  final bool isNormal;

  /// The height of the tab.
  final double height;

  /// Optional margin to apply to the tab.
  final EdgeInsetsGeometry? margin;

  /// The background of a selected tab.
  final Color? selectedBackgroundColor;

  /// The background of an unselected tab.
  final Color? unselectedBackgroundColor;

  /// The label colour of a selected tab.
  final Color? selectedLabelColor;

  /// The label colour of an unselected tab.
  final Color? unselectedLabelColor;

  /// The icon theme of a selected tab.
  final IconThemeData? selectedIconTheme;

  /// The icon theme of an unselected tab.
  final IconThemeData? unselectedIconTheme;

  /// Builds the widget for the individual tab.
  ///
  /// This method determines the background and label colours based on whether
  /// the tab is selected and the tab bar type. It then renders the tab content
  /// (icon and text) within an animated container.
  @override
  Widget build(BuildContext context) {
    final background = selected
        ? selectedBackgroundColor
        : unselectedBackgroundColor;
    final labelColor = selected ? selectedLabelColor : unselectedLabelColor;
    final iconTheme = selected ? selectedIconTheme : unselectedIconTheme;
    final icon = data.icon;

    final child = AnimatedContainer(
      duration: const Duration(milliseconds: 200),
      curve: Curves.easeInOut,
      height: height,
      margin: margin,
      padding: const .symmetric(vertical: 6),
      decoration: BoxDecoration(
        color: background,
        borderRadius: isNormal ? null : Corners.circle,
      ),
      child: Row(
        mainAxisSize: .min,
        mainAxisAlignment: .center,
        children: [
          if (icon != null) ...[
            IconTheme(data: iconTheme ?? const IconThemeData(), child: icon),
            const MSpacer.h(Insets.xs),
          ],
          MText.caption(data.text, color: labelColor),
        ],
      ),
    );

    if (!isNormal) return Skeleton.unite(child: child);
    return child;
  }
}
