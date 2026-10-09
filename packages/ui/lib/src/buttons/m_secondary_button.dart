import 'package:flutter/material.dart';
import 'package:meno_ui/src/buttons/m_button.dart';
import 'package:meno_ui/src/loading_indicator/loading_indicator.dart';
import 'package:meno_ui/src/theme/extensions/m_button_styles.dart';

/// The secondary action button: an outlined button in the brand colour.
///
/// It pairs with `MPrimaryButton` as the less prominent choice, showing a
/// loading indicator while [loading] is true.
///
/// Example usage:
/// ```dart
/// MSecondaryButton(
///   label: 'Cancel',
///   onPressed: cancel,
/// );
/// ```
///
/// For an icon button version:
/// ```dart
/// MSecondaryButton.icon(
///   label: 'Cancel',
///   icon: const Icon(Icons.cancel),
///   onPressed: cancel,
/// );
/// ```
class MSecondaryButton extends MButton {
  /// Creates a secondary button with a text label.
  const new({
    required super.label,
    required super.onPressed,
    super.key,
    super.iconPlacement,
    super.loading,
    super.style,
  });

  /// Creates a secondary button that renders an icon alongside its label.
  const new icon({
    required super.label,
    required super.icon,
    required super.onPressed,
    super.key,
    super.iconPlacement,
    super.loading,
    super.style,
  }) : super.icon();

  @override
  Widget buildButton(BuildContext context, Widget child) {
    return OutlinedButton(
      style: style?.merge(MButtonStyles.of(context).secondary),
      onPressed: effectiveOnPressed,
      child: loading ? const MLoadingIndicator.four(width: 56) : child,
    );
  }
}
