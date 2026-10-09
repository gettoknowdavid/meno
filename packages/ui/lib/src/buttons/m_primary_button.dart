import 'package:flutter/material.dart';
import 'package:meno_ui/src/buttons/m_button.dart';
import 'package:meno_ui/src/loading_indicator/loading_indicator.dart';
import 'package:meno_ui/src/theme/extensions/m_button_styles.dart';

/// The primary action button: a filled button in the brand colour.
///
/// It shows a loading indicator while [loading] is true and can be parked with
/// [disabled], which reads the same to the user but ignores taps.
///
/// Example usage:
/// ```dart
/// MPrimaryButton(
///   label: 'Submit',
///   onPressed: submit,
/// );
/// ```
///
/// For an icon button version:
/// ```dart
/// MPrimaryButton.icon(
///   label: 'Submit',
///   icon: const Icon(MIcons.send),
///   onPressed: submit,
/// );
/// ```
class MPrimaryButton extends MButton {
  /// Creates a primary button with a text label.
  const new({
    required super.label,
    required super.onPressed,
    super.key,
    super.iconPlacement,
    super.loading,
    super.style,
    this.disabled = false,
  });

  /// Creates a primary button that renders an icon alongside its label.
  const new icon({
    required super.label,
    required super.icon,
    required super.onPressed,
    super.key,
    super.iconPlacement,
    super.loading,
    super.style,
    this.disabled = false,
  }) : super.icon();

  /// Whether the button ignores taps. A disabled button still shows its
  /// content, so the label does not vanish mid-flow.
  final bool disabled;

  @override
  Widget buildButton(BuildContext context, Widget child) {
    return ElevatedButton(
      style: style?.merge(MButtonStyles.of(context).primary),
      onPressed: disabled ? null : effectiveOnPressed,
      child: loading ? const MLoadingIndicator.four(width: 56) : child,
    );
  }
}
