import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/navigation/m_destination.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The height of a single rail item.
const double _kItemHeight = Insets.xxxl;

/// A vertical navigation rail for wide layouts.
///
/// Renders a [NavigationRail] from a shared list of [MDestination]s, styled the
/// same way as the bottom bar items so the compact bar and the wide rail read
/// as one component. The rail is presentation only: it reports taps through
/// [onDestinationSelected] and leaves routing to the caller.
///
/// Both ends of the rail accept an arbitrary widget, so app-specific chrome
/// (a logo, a settings entry, a broadcast button) lives with the caller:
/// - [leading] is pinned above the destinations.
/// - [trailing] is pinned below them.
///
/// Example usage:
/// ```dart
/// MSideNavRail(
///   selectedIndex: index,
///   onDestinationSelected: (i) => routeTo(i),
///   destinations: const [
///     MDestination(icon: Icon(MIcons.home_04), label: 'Home'),
///     MDestination(icon: Icon(MIcons.compass), label: 'Discover'),
///   ],
///   leading: Assets.images.menoPurple.image(height: 24),
/// )
/// ```
class MSideNavRail extends StatelessWidget {
  /// Creates a side navigation rail.
  ///
  /// - [destinations]: The destinations shown, in display order.
  /// - [selectedIndex]: The index of the selected destination.
  /// - [onDestinationSelected]: Called with the tapped destination's index.
  /// - [leading]: An optional widget pinned above the destinations.
  /// - [trailing]: An optional widget pinned below the destinations.
  const new({
    required this.destinations,
    required this.selectedIndex,
    super.key,
    this.onDestinationSelected,
    this.leading,
    this.trailing,
  });

  /// The destinations shown in the rail, in display order.
  final List<MDestination> destinations;

  /// The index of the selected destination.
  final int selectedIndex;

  /// Called with the tapped destination's index.
  final ValueChanged<int>? onDestinationSelected;

  /// An optional widget pinned above the destinations, typically a logo.
  final Widget? leading;

  /// An optional widget pinned below the destinations.
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    return Container(
      padding: const .symmetric(horizontal: Insets.md),
      decoration: BoxDecoration(
        color: colors.background,
        border: Border(right: BorderSide(color: colors.outlineVariant1)),
      ),
      child: NavigationRail(
        selectedIndex: selectedIndex,
        onDestinationSelected: onDestinationSelected,
        minWidth: 56,
        minExtendedWidth: 204,
        leading: leading == null
            ? null
            : Padding(
                padding: const .only(top: Insets.md, bottom: Insets.xxl),
                child: Align(child: leading),
              ),
        destinations: [
          for (var i = 0; i < destinations.length; i++)
            NavigationRailDestination(
              label: MText(destinations[i].label),
              icon: MRailItem(
                icon: destinations[i].icon,
                label: destinations[i].label,
                selected: selectedIndex == i,
                onTap: () => onDestinationSelected?.call(i),
              ),
            ),
        ],
        trailing: trailing == null
            ? null
            : Padding(
                padding: const .only(top: Insets.xl),
                child: trailing,
              ),
      ),
    );
  }
}

/// A single, selectable item in a [MSideNavRail].
///
/// Shows just the glyph when compact, and reveals the [label] beside it when
/// [extended] is true. The selected state fills the item with the primary
/// container color and resolves its icon color from [NavigationRailThemeData].
///
/// Because [icon] is a plain [Widget], a rail item can render anything.
class MRailItem extends StatelessWidget {
  /// Creates a rail item.
  ///
  /// - [icon]: The widget drawn as the item's glyph.
  /// - [label]: The text revealed when [extended] is true.
  /// - [selected]: Whether this item is the active one.
  /// - [onTap]: Called when the item is tapped.
  /// - [extended]: Whether to show the label beside the icon.
  const new({
    required this.icon,
    required this.label,
    required this.selected,
    this.onTap,
    this.extended = false,
    super.key,
  });

  /// The widget drawn as the item's glyph.
  final Widget icon;

  /// The text revealed beside the icon when [extended] is true.
  final String label;

  /// Whether this item is currently selected.
  final bool selected;

  /// Called when the item is tapped.
  final VoidCallback? onTap;

  /// Whether to lay the item out as a row with its label visible.
  final bool extended;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final navigationRailTheme = Theme.of(context).navigationRailTheme;

    final iconWidget = IconTheme(
      data: (selected
              ? navigationRailTheme.selectedIconTheme
              : navigationRailTheme.unselectedIconTheme) ??
          const IconThemeData(),
      child: icon,
    );

    return Semantics(
      label: label,
      selected: selected,
      button: true,
      child: Center(
        child: InkWell(
          onTap: onTap,
          borderRadius: Corners.md,
          child: Container(
            height: _kItemHeight,
            padding: const .symmetric(horizontal: Insets.md),
            decoration: BoxDecoration(
              color: selected ? colors.primaryContainer : null,
              borderRadius: Corners.md,
            ),
            child: !extended
                ? Center(child: iconWidget)
                : Row(
                    children: [
                      iconWidget,
                      Spaces.horizontalSmall,
                      MText(
                        label,
                        style: selected
                            ? navigationRailTheme.selectedLabelTextStyle
                            : navigationRailTheme.unselectedLabelTextStyle,
                      ),
                    ],
                  ),
          ),
        ),
      ),
    );
  }
}
