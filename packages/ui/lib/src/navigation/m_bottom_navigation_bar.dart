import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:meno_ui/src/internal/m_internal.dart';

/// A customizable bottom navigation bar widget for your Flutter app.
///
/// The [MBottomNavigationBar] widget is designed to provide a responsive
/// and customizable bottom navigation bar that supports multiple items.
///
/// Example:
/// ```dart
/// MBottomNavigationBar(
///   currentIndex: _selectedIndex,
///   items: [
///     BottomNavigationBarItem(
///       icon: Icon(Icons.home),
///       label: 'Home',
///     ),
///     BottomNavigationBarItem(
///       icon: Icon(Icons.search),
///       label: 'Search',
///     ),
///     BottomNavigationBarItem(
///       icon: Icon(Icons.favorite),
///       label: 'Favorites',
///     ),
///     BottomNavigationBarItem(
///       icon: Icon(Icons.person),
///       label: 'Profile',
///     ),
///   ],
///   onTap: (index) {
///     // Handle navigation to the selected tab.
///   },
/// )
/// ```
class MBottomNavigationBar extends StatefulWidget {
  /// Creates an [MBottomNavigationBar] widget.
  const new({
    required this.currentIndex,
    required this.items,
    super.key,
    this.onTap,
    this.customItem,
  });

  /// The index of the currently selected item.
  final int currentIndex;

  /// The list of [BottomNavigationBarItem]s that define each item in the bar.
  final List<BottomNavigationBarItem> items;

  /// A callback function called when an item is tapped.
  final ValueChanged<int>? onTap;

  /// Add a custom center item, e.g. a Microphone or Add button
  final Widget? customItem;

  @override
  State<MBottomNavigationBar> createState() => _MBottomNavigationBarState();
}

class _MBottomNavigationBarState extends State<MBottomNavigationBar> {
  @override
  Widget build(BuildContext context) {
    final isLight = Theme.of(context).brightness == Brightness.light;
    final theme = Theme.of(context).bottomNavigationBarTheme;
    final size = MediaQuery.sizeOf(context);

    return Container(
      width: size.width,
      padding: const .all(Insets.lg),
      decoration: BoxDecoration(
        color: theme.backgroundColor,
        border: Border(
          top: BorderSide(
            width: 0.80,
            color: MInternal.resolve(isLight, MColor.grey30, MColor.grey400),
          ),
        ),
      ),
      child: Row(
        mainAxisAlignment: .spaceAround,
        children: _buildNavigationTiles(),
      ),
    );
  }

  // Build the list of navigation tiles
  List<Widget> _buildNavigationTiles() {
    final tiles = <Widget>[];
    final itemCount = widget.items.length;

    for (var i = 0; i < itemCount; i++) {
      if (i == 2 && widget.customItem != null) {
        tiles.add(widget.customItem!);
      }
      tiles.add(
        MBottomBarNavigationItem(
          indexLabel: widget.items[i].label,
          onTap: () => widget.onTap?.call(i),
          item: widget.items[i],
          selected: i == widget.currentIndex,
        ),
      );
    }

    return tiles;
  }
}

/// A responsive and customizable item for a bottom navigation bar.
///
/// The [MBottomBarNavigationItem] widget is designed to create individual
/// items for a bottom navigation bar. It supports icons, labels, and tooltips
/// for each item.
class MBottomBarNavigationItem extends StatelessWidget {
  /// Creates a [MBottomBarNavigationItem] widget.
  const new({
    required this.onTap,
    required this.selected,
    super.key,
    this.item,
    this.indexLabel,
    this.customItem,
  }) : assert(
         item == null || customItem == null,
         'item and custom must not be provided together',
       );

  /// The [BottomNavigationBarItem] to display.
  final BottomNavigationBarItem? item;

  /// A label associated with the index for accessibility.
  final String? indexLabel;

  /// A callback function called when the item is tapped.
  final VoidCallback onTap;

  /// Indicates whether this item is currently selected.
  final bool selected;

  /// For adding any custom widget
  final Widget? customItem;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context).bottomNavigationBarTheme;

    final effectiveTooltip = item?.tooltip == ''
        ? null
        : item?.tooltip ?? item?.label;

    final content = (customItem != null)
        ? customItem!
        : Column(
            mainAxisSize: .min,
            mainAxisAlignment: .center,
            children: [
              IconTheme(
                data: selected
                    ? theme.selectedIconTheme!
                    : theme.unselectedIconTheme!,
                child: item!.icon,
              ),
              const SizedBox(height: 6),
              if (item!.label != null)
                Text(
                  item!.label!,
                  style: selected
                      ? theme.selectedLabelStyle
                      : theme.unselectedLabelStyle,
                ),
            ],
          );

    final Widget result = InkWell(
      onTap: onTap,
      child: Container(
        color: theme.backgroundColor,
        width: 62.5,
        constraints: const BoxConstraints(minHeight: _kMicSize),
        child: content,
      ),
    );

    var finalResult = result;

    if (effectiveTooltip != null) {
      finalResult = Tooltip(
        message: effectiveTooltip,
        preferBelow: false,
        verticalOffset: 50,
        excludeFromSemantics: true,
        child: result,
      );
    }

    return Semantics(
      selected: selected,
      container: true,
      child: Stack(
        children: <Widget>[
          finalResult,
          Semantics(label: indexLabel),
        ],
      ),
    );
  }
}

const _kMicSize = 56.0;
const _kConstraints = BoxConstraints(minWidth: _kMicSize, minHeight: _kMicSize);

/// A microphone icon widget with custom styling.
///
/// The [Microphone] widget displays a microphone icon with custom styling.
class Microphone extends StatelessWidget {
  /// Creates a [Microphone] widget.
  const new({super.key, this.onTap});

  /// Call back action when the microphone icon is tapped
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
        constraints: visualDensity.effectiveConstraints(_kConstraints),
        child: Icon(MIcons.microphone, color: colorScheme.onPrimary),
      ),
    );
  }
}
