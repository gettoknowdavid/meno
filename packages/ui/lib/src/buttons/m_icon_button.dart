import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The smallest square an icon button is allowed to occupy.
const double _kMinSize = Insets.xl;

const BoxConstraints _kConstraints = BoxConstraints(
  minWidth: _kMinSize,
  minHeight: _kMinSize,
);

/// A circular, icon-only button.
///
/// It can be drawn flat or filled, and sizes itself from the ambient icon
/// theme, so it follows the app's typography and density settings. When
/// disabled, it stops responding to taps without changing its appearance.
///
/// Example usage:
/// ```dart
/// MIconButton(
///   icon: const Icon(Icons.add),
///   isFilled: true,
///   fillColor: colors.primary,
///   onPressed: add,
/// );
/// ```
class MIconButton extends StatelessWidget {
  /// Creates an icon-only button.
  ///
  /// Parameters:
  /// - [icon]: The icon to display inside the button.
  /// - [key]: An optional key to identify the widget.
  /// - [size]: The diameter of the button. Defaults to 24.
  /// - [iconSize]: The size of the icon. Defaults to [size].
  /// - [color]: The colour of the icon. Defaults to the theme's icon colour.
  /// - [fillColor]: The background colour used when [isFilled] is true.
  /// - [isFilled]: Whether to paint a filled background. Defaults to false.
  /// - [onPressed]: The callback invoked when the button is pressed.
  /// - [padding]: The padding around the icon when the button is filled.
  /// - [constraints]: Optional constraints for the button.
  /// - [isDisabled]: Whether the button ignores taps. Defaults to false.
  const new({
    required this.icon,
    super.key,
    this.size = Insets.xl,
    this.iconSize,
    this.color,
    this.fillColor,
    this.isFilled = false,
    this.onPressed,
    this.padding,
    this.constraints,
    this.isDisabled = false,
  });

  /// The icon drawn inside the button.
  final Widget icon;

  /// The diameter of the button.
  final double size;

  /// The size of the icon, defaulting to [size].
  final double? iconSize;

  /// The colour of the icon, defaulting to the theme's icon colour.
  final Color? color;

  /// The background colour used when [isFilled] is true.
  final Color? fillColor;

  /// Whether the button paints a filled background.
  final bool isFilled;

  /// The callback invoked when the button is pressed.
  final VoidCallback? onPressed;

  /// The padding around the icon when the button is filled.
  final EdgeInsetsGeometry? padding;

  /// Optional constraints for the button.
  final BoxConstraints? constraints;

  /// Whether the button ignores taps.
  final bool isDisabled;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final effectiveSize = theme.iconTheme.size ?? size;
    final boxConstraints = theme.visualDensity.effectiveConstraints(
      _kConstraints,
    );

    return InkResponse(
      radius: math.max(Material.defaultSplashRadius, size),
      onTap: isDisabled ? null : onPressed,
      child: Container(
        height: size,
        width: size,
        alignment: Alignment.center,
        constraints: constraints ?? boxConstraints,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: isFilled ? fillColor : null,
        ),
        padding: isFilled ? padding : EdgeInsets.zero,
        child: SizedBox.square(
          dimension: effectiveSize,
          child: IconTheme.merge(
            data: IconThemeData(
              size: iconSize ?? effectiveSize,
              color: color ?? theme.iconTheme.color,
            ),
            child: icon,
          ),
        ),
      ),
    );
  }
}
