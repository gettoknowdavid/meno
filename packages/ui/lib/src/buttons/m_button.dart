import 'dart:ui' show clampDouble, lerpDouble;

import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/buttons/m_button_icon_placement.dart';
import 'package:meno_ui/src/loading_indicator/loading_indicator.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/tokens/spacing.dart';
/// The shared base of every Meno button.
///
/// A button is a label with an optional icon. The base owns those inputs and
/// the loading state; [buildButton] picks the Material widget and style that
/// renders them.
///
/// Example usage:
/// ```dart
/// class MyButton extends MButton {
///   const MyButton({
///     required super.label,
///     required super.onPressed,
///     super.key,
///   });
///
///   @override
///   Widget buildButton(BuildContext context, Widget child) {
///     return ElevatedButton(onPressed: effectiveOnPressed, child: child);
///   }
/// }
/// ```
abstract class MButton extends StatelessWidget {
  /// Creates a button with a text label and no icon.
  const new({
    required this.label,
    required this.onPressed,
    this.iconPlacement = MButtonIconPlacement.left,
    this.loading = false,
    this.style,
    super.key,
  }) : icon = null;

  /// Creates a button that renders [icon] alongside its label.
  const new icon({
    required this.label,
    required this.icon,
    required this.onPressed,
    this.iconPlacement = MButtonIconPlacement.left,
    this.loading = false,
    this.style,
    super.key,
  });

  /// The text label drawn inside the button.
  final String label;

  /// The icon drawn next to [label], or null for a label-only button.
  final Widget? icon;

  /// Where [icon] sits relative to [label].
  final MButtonIconPlacement iconPlacement;

  /// Called when the button is tapped, unless it is currently disabled.
  final VoidCallback? onPressed;

  /// Whether the button is in its loading state, which swaps the content for a
  /// loading indicator.
  final bool loading;

  /// An optional style merged on top of the flavour's own style.
  final ButtonStyle? style;

  /// The callback to hand to the underlying Material button.
  ///
  /// Null while [loading], so a button cannot be tapped while its request is
  /// still in flight.
  VoidCallback? get effectiveOnPressed => loading ? null : onPressed;

  @override
  Widget build(BuildContext context) {
    final Widget content = icon == null
        ? MText(label)
        : _MButtonWithIcon(
            icon: icon!,
            label: label,
            iconPlacement: iconPlacement,
          );

    return buildButton(
      context,
      loading ? const MLoadingIndicator.four() : content,
    );
  }

  /// Builds the Material button that renders [child].
  ///
  /// Subclasses choose the widget and merge their own style over [style].
  Widget buildButton(BuildContext context, Widget child);
}

/// Draws a button's icon and label in a row.
///
/// The gap between the two shrinks as the ambient text scale grows, so a
/// scaled-up label keeps its share of the available width.
class _MButtonWithIcon extends StatelessWidget {
  const new({
    required this.icon,
    required this.label,
    required this.iconPlacement,
  });

  final Widget icon;
  final String label;
  final MButtonIconPlacement iconPlacement;

  /// The label size the gap is calibrated against.
  static const double _referenceFontSize = 14;

  /// The gap at the default text scale.
  static const double _maxGap = Insets.sm;

  /// The gap at twice the default text scale.
  static const double _minGap = Insets.xs;

  @override
  Widget build(BuildContext context) {
    final scale = MediaQuery.textScalerOf(context).scale(_referenceFontSize);
    final growth = clampDouble(scale / _referenceFontSize, 1, 2) - 1;
    final gap = SizedBox(width: lerpDouble(_maxGap, _minGap, growth));

    return Row(
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        if (iconPlacement == MButtonIconPlacement.left) ...[icon, gap],
        Flexible(child: MText(label)),
        if (iconPlacement == MButtonIconPlacement.right) ...[gap, icon],
      ],
    );
  }
}
