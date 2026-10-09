import 'package:flutter/material.dart';
import 'package:meno_ui/meno_ui.dart';

/// A customizable divider with spacing control.
///
/// This widget provides a horizontal or vertical divider with optional spacing
/// around it. The color and thickness can be customized, defaulting to the
/// current [MColorScheme]'s divider color.
class MDivider extends StatelessWidget {
  /// Creates a new `MDivider` widget.
  ///
  /// * `vertical`: Render the divider vertically instead of the default
  ///   horizontal layout.
  /// * `topSpace`: The vertical space above the divider.
  /// * `bottomSpace`: The vertical space below the divider.
  /// * `start`: The horizontal indent of the divider.
  /// * `end`: The horizontal end indent of the divider.
  /// * `color`: The color of the divider (defaults to the theme's divider color).
  /// * `thickness`: The thickness of the divider line in logical pixels.
  const MDivider({
    super.key,
    this.vertical = false,
    this.topSpace,
    this.bottomSpace,
    this.start,
    this.end,
    this.color,
    this.thickness = 1,
  });

  /// Whether the divider is rendered vertically.
  final bool vertical;

  /// The vertical space above the divider.
  final double? topSpace;

  /// The vertical space below the divider.
  final double? bottomSpace;

  /// The horizontal indent of the divider.
  final double? start;

  /// The horizontal end indent of the divider.
  final double? end;

  /// The color of the divider.
  final Color? color;

  /// The thickness of the divider line in logical pixels.
  final double thickness;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final dividerColor = color ?? colors.outlineVariant1;
    final aligned = vertical
        ? CrossAxisAlignment.start
        : CrossAxisAlignment.center;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: aligned,
      children: [
        SizedBox(height: topSpace),
        Container(
          height: thickness,
          color: dividerColor,
          margin: EdgeInsets.zero,
        ),
        SizedBox(height: bottomSpace),
      ],
    );
  }
}
