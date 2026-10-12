import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/navigation/m_destination.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/m_icons.dart';
import 'package:meno_ui/src/tokens/shadows.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The width of a single bottom-bar tile.
const double _kTileWidth = 62.5;

/// The height of a single bottom-bar tile.
const double _kTileHeight = 56;

/// The gap between a tile's glyph and its label.
const double _kIconLabelGap = 6;

/// The thickness of the hairline drawn above the bar.
const double _kHairline = 0.8;

/// The diameter of the filled [MMicrophone] control.
const double _kMicSize = 56;

/// A customizable bottom navigation bar for your Flutter app.
///
/// The bar draws one [MBottomNavItem] per [destinations] entry and keeps the
/// tap and selection wiring presentation-only: pass [onTap] and it reports the
/// tapped index, leaving routing to the caller.
///
/// It is built to accept any widget, not just icons and labels:
/// - [destinations] carry a [MDestination] whose `icon` is a plain widget, so a
///   tab can render a badge, an avatar or anything else.
/// - [centerItem] injects an arbitrary widget in the middle of the bar (index
///   `2`), for example the [MMicrophone] action seen in the design.
/// - [trailing] appends an arbitrary widget after the destinations, for example
///   a broadcast or "go live" action.
///
/// All styling is resolved from [NavigationBarThemeData], so the bar restyles
/// with the theme rather than hard-coding colors.
///
/// Example usage:
/// ```dart
/// MBottomNavigationBar(
///   currentIndex: _selectedIndex,
///   destinations: const [
///     MDestination(icon: Icon(MIcons.home_04), label: 'Home'),
///     MDestination(icon: Icon(MIcons.compass), label: 'Discover'),
///     MDestination(icon: Icon(MIcons.file_02), label: 'Notes'),
///     MDestination(icon: Icon(MIcons.user), label: 'Profile'),
///   ],
///   centerItem: MMicrophone(onTap: () => startBroadcast()),
///   onTap: (index) => goTo(index),
/// )
/// ```
class MBottomNavigationBar extends StatelessWidget {
  /// Creates an [MBottomNavigationBar] widget.
  ///
  /// - [currentIndex]: The index of the selected destination.
  /// - [destinations]: The destinations shown, in display order.
  /// - [onTap]: Called with the tapped destination's index.
  /// - [centerItem]: An arbitrary widget inserted at the middle of the bar.
  /// - [trailing]: An arbitrary widget appended after the destinations.
  const new({
    required this.currentIndex,
    required this.destinations,
    super.key,
    this.onTap,
    this.centerItem,
    this.trailing,
  });

  /// The index of the currently selected destination.
  final int currentIndex;

  /// The destinations shown in the bar, in display order.
  final List<MDestination> destinations;

  /// A callback invoked with the index of the tapped destination.
  final ValueChanged<int>? onTap;

  /// An arbitrary widget inserted in the middle of the bar.
  ///
  /// Injected before the destination at index `2`, so it sits in the centre of
  /// a four-destination bar. Use it for a microphone, a compose button or any
  /// other accent control.
  final Widget? centerItem;

  /// An arbitrary widget appended after the destinations.
  ///
  /// Use it for actions that are not tabs, such as a broadcast entry.
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final navigationBarTheme = Theme.of(context).navigationBarTheme;
    final colors = MColorScheme.of(context);
    final size = MediaQuery.sizeOf(context);

    return Container(
      width: size.width,
      padding: const .symmetric(horizontal: Insets.md, vertical: Insets.lg),
      decoration: BoxDecoration(
        color: navigationBarTheme.backgroundColor,
        border: Border(
          top: BorderSide(width: _kHairline, color: colors.outlineVariant1),
        ),
      ),
      child: Row(mainAxisAlignment: .spaceAround, children: _buildTiles()),
    );
  }

  /// Builds the tiles, splicing in [centerItem] and [trailing] around the
  /// destinations.
  List<Widget> _buildTiles() {
    final tiles = <Widget>[];
    final itemCount = destinations.length;

    for (var i = 0; i < itemCount; i++) {
      if (i == 2 && centerItem != null) {
        // `heightFactor: 1` keeps the slot at its child's height so the bar
        // hugs its content instead of stretching to the scaffold.
        tiles.add(Center(heightFactor: 1, child: centerItem));
      }
      tiles.add(
        MBottomNavItem(
          icon: destinations[i].icon,
          label: destinations[i].label,
          selected: i == currentIndex,
          onTap: () => onTap?.call(i),
        ),
      );
    }

    if (trailing != null) {
      tiles.add(Center(heightFactor: 1, child: trailing));
    }

    return tiles;
  }
}

/// A single, selectable item in a [MBottomNavigationBar].
///
/// Draws one destination as a fixed [tile][_kTileWidth] containing the glyph
/// above its label. The glyph and label colors resolve from
/// [NavigationBarThemeData] in the selected and unselected states, so an item
/// restyles with the theme.
///
/// Because [icon] is a plain [Widget], a custom item can be anything; the
/// [MBottomNavigationBar.centerItem] and `trailing` slots are fully free-form
/// as well.
class MBottomNavItem extends StatelessWidget {
  /// Creates a bottom-bar item.
  ///
  /// - [icon]: The widget drawn above the label.
  /// - [label]: The text shown beneath the icon.
  /// - [selected]: Whether this item is the active one.
  /// - [onTap]: Called when the item is tapped.
  const new({
    required this.icon,
    required this.label,
    required this.selected,
    required this.onTap,
    super.key,
  });

  /// The widget drawn above the label.
  final Widget icon;

  /// The text shown beneath the icon.
  final String label;

  /// Whether this item is currently selected.
  final bool selected;

  /// Called when the item is tapped.
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final navigationBarTheme = Theme.of(context).navigationBarTheme;
    const selectedState = <WidgetState>{WidgetState.selected};
    const unselectedState = <WidgetState>{};

    final iconTheme =
        (selected
            ? navigationBarTheme.iconTheme?.resolve(selectedState)
            : navigationBarTheme.iconTheme?.resolve(unselectedState)) ??
        const IconThemeData();

    final labelStyle = selected
        ? navigationBarTheme.labelTextStyle?.resolve(selectedState)
        : navigationBarTheme.labelTextStyle?.resolve(unselectedState);

    return Center(
      heightFactor: 1,
      child: InkWell(
        onTap: onTap,
        child: Container(
          width: _kTileWidth,
          height: _kTileHeight,
          color: navigationBarTheme.backgroundColor,
          child: Column(
            mainAxisSize: .min,
            mainAxisAlignment: .center,
            children: [
              IconTheme(data: iconTheme, child: icon),
              const SizedBox(height: _kIconLabelGap),
              // Kept to one line: the tile has a fixed height, and a wrapping
              // label would overflow it.
              MText(
                label,
                style: labelStyle,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// A filled, circular microphone action for the bottom bar.
///
/// Drawn from the primary color with the brand-tinted [Shadows.mic] lift, so it
/// reads as the bar's accent control. Pair it with
/// [MBottomNavigationBar.centerItem].
///
/// Example usage:
/// ```dart
/// MMicrophone(onTap: () => startBroadcast());
/// ```
class MMicrophone extends StatelessWidget {
  /// Creates a microphone action.
  ///
  /// - [onTap]: The callback invoked when the control is tapped.
  const new({super.key, this.onTap});

  /// The callback invoked when the control is tapped.
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final visualDensity = theme.visualDensity;

    return InkWell(
      onTap: onTap,
      borderRadius: Corners.circle,
      child: Container(
        height: _kMicSize,
        width: _kMicSize,
        decoration: BoxDecoration(
          shape: .circle,
          color: colorScheme.primary,
          boxShadow: Shadows.mic,
        ),
        constraints: visualDensity.effectiveConstraints(
          const BoxConstraints(minWidth: _kMicSize, minHeight: _kMicSize),
        ),
        child: Icon(MIcons.microphone, color: colorScheme.onPrimary),
      ),
    );
  }
}
