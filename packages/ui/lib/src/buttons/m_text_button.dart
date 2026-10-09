import 'package:flutter/material.dart';
import 'package:meno_ui/src/buttons/m_button.dart';
import 'package:meno_ui/src/theme/extensions/m_button_styles.dart';

/// The text button: a flat, label-only button for the least prominent action.
///
/// Example usage:
/// ```dart
/// MTextButton(
///   label: 'Skip',
///   onPressed: skip,
/// );
/// ```
///
/// For an icon button version:
/// ```dart
/// MTextButton.icon(
///   label: 'Skip',
///   icon: const Icon(Icons.skip_next),
///   onPressed: skip,
/// );
/// ```
class MTextButton extends MButton {
  /// Creates a text button with a text label.
  const new({
    required super.label,
    required super.onPressed,
    super.key,
    super.iconPlacement,
    super.style,
  });

  /// Creates a text button that renders an icon alongside its label.
  const new icon({
    required super.label,
    required super.icon,
    required super.onPressed,
    super.key,
    super.iconPlacement,
    super.style,
  }) : super.icon();

  @override
  Widget buildButton(BuildContext context, Widget child) {
    return TextButton(
      style: style?.merge(MButtonStyles.of(context).text),
      onPressed: effectiveOnPressed,
      child: child,
    );
  }
}
