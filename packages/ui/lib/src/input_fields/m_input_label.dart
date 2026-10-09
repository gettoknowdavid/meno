import 'package:flutter/material.dart';
import 'package:meno_ui/meno_ui.dart';

/// A widget that displays a label for an input field, optionally with an icon.
///
/// This widget is used to create labels for input fields, such as text fields
/// or form fields.
/// It can optionally display an icon and a required indicator.
///
/// To use this widget, pass the label text and optionally provide an icon and
/// specify if the field is required.
///
/// Example usage:
/// ```dart
/// MInputLabel(
///   'Username',
///   icon: Icons.person,
///   required: true,
/// );
/// ```
class MInputLabel extends StatelessWidget {
  /// Creates an instance of [MInputLabel].
  ///
  /// Parameters:
  /// - [label]: The text to display for the label.
  /// - [key]: An optional key to identify the widget.
  /// - [icon]: An optional icon to display alongside the label.
  /// - [required]: A boolean indicating whether the field is required.
  /// Defaults to false.
  const new(this.label, {super.key, this.icon, this.required = false});

  /// The text to display for the label.
  final String label;

  /// An optional icon to display alongside the label.
  final IconData? icon;

  /// A boolean indicating whether the field is required. Defaults to false.
  final bool required;

  @override
  Widget build(BuildContext context) {
    final styles = MTextFieldStyle.of(context);
    return SizedBox(
      height: 18,
      child: Row(
        mainAxisSize: .min,
        children: [
          if (icon != null) ...[
            Icon(icon, size: Insets.lg, color: styles.iconColor),
            const SizedBox(width: 6),
          ],
          MText(label, color: styles.textColor, style: styles.labelTextStyle),
          if (required) ...[
            const SizedBox(width: 6),
            MText('*', color: styles.errorColor, style: styles.labelTextStyle),
          ],
        ],
      ),
    );
  }
}
