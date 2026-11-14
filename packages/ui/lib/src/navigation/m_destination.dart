import 'package:material_ui/material_ui.dart';

/// A single navigation destination.
///
/// One list of these drives both the bottom navigation bar and the side nav
/// rail, so the compact and wide presentations can never drift apart.
///
/// [icon] is a plain [Widget], not an [IconData], so a destination can render
/// anything: a glyph, a glyph with a badge, or a fully custom tile.
///
/// Example usage:
/// ```dart
/// const MDestination(icon: Icon(MIcons.home_04), label: 'Home');
/// ```
@immutable
final class MDestination {
  /// Creates a destination.
  ///
  /// - [icon]: The widget drawn for this destination.
  /// - [label]: The text shown beside or beneath the icon.
  const new({required this.icon, required this.label});

  /// The widget drawn for this destination.
  final Widget icon;

  /// The label shown for this destination.
  final String label;
}
