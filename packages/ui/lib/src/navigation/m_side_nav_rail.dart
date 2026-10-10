import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// A single destination shown by an [MSideNavRail].
///
/// The rail is presentation only: it draws the icon and label and reports taps,
/// leaving routing to the caller.
class MSideNavRailDestination {
  /// Creates a rail destination.
  ///
  /// [icon] is the leading glyph and [label] is the text shown beside it when
  /// the rail is extended.
  const new({required this.icon, required this.label});

  /// The glyph drawn for this destination.
  final Widget icon;

  /// The label shown for this destination.
  final String label;
}

/// A vertical navigation rail for wide layouts.
///
/// Renders a [NavigationRail] with the design system's item styling. The caller
/// owns navigation, so pass [onDestinationSelected] to react to taps and supply
/// [leading] (for example a logo) and [trailing] (for example a settings
/// action) as needed.
///
/// Example usage:
/// ```dart
/// MSideNavRail(
///   selectedIndex: index,
///   onDestinationSelected: (i) => routeTo(i),
///   destinations: const [
///     MSideNavRailDestination(icon: Icon(MIcons.home_04), label: 'Home'),
///     MSideNavRailDestination(icon: Icon(MIcons.compass), label: 'Discover'),
///   ],
/// )
/// ```
class MSideNavRail extends StatelessWidget {
  /// Creates a side navigation rail.
  ///
  /// [destinations] must not be empty and [selectedIndex] must point at one of
  /// them.
  const new({
    required this.destinations,
    required this.selectedIndex,
    super.key,
    this.onDestinationSelected,
    this.leading,
    this.trailing,
  });

  /// The destinations shown in the rail, in display order.
  final List<MSideNavRailDestination> destinations;

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
                padding: const .only(
                  top: Insets.md,
                  bottom: Insets.xxl,
                ),
                child: Align(child: leading),
              ),
        destinations: [
          for (var i = 0; i < destinations.length; i++)
            NavigationRailDestination(
              label: MText(destinations[i].label),
              icon: _MRailItem(
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

/// Draws one rail item: an icon picker that highlights when selected.
class _MRailItem extends StatelessWidget {
  const new({
    required this.icon,
    required this.label,
    required this.selected,
    this.onTap,
  });

  final Widget icon;
  final String label;
  final bool selected;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final railTheme = Theme.of(context).navigationRailTheme;
    final iconTheme = selected
        ? railTheme.selectedIconTheme ?? const IconThemeData()
        : railTheme.unselectedIconTheme ?? const IconThemeData();

    return Semantics(
      label: label,
      selected: selected,
      button: true,
      child: InkWell(
        onTap: onTap,
        borderRadius: Corners.md,
        child: Container(
          height: Insets.xxxl,
          padding: const .symmetric(horizontal: Insets.md),
          decoration: BoxDecoration(
            color: selected ? colors.primaryContainer : null,
            borderRadius: Corners.md,
          ),
          child: Center(
            child: IconTheme(data: iconTheme, child: icon),
          ),
        ),
      ),
    );
  }
}
