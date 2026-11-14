import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/buttons/m_button.dart';
import 'package:meno_ui/src/theme/extensions/m_button_styles.dart';

/// The success action button: a filled button in the success colour.
///
/// It marks a positive outcome, such as confirming a change.
///
/// Example usage:
/// ```dart
/// MSuccessButton(
///   label: 'Confirm',
///   onPressed: confirm,
/// );
/// ```
///
/// For an icon button version:
/// ```dart
/// MSuccessButton.icon(
///   label: 'Confirm',
///   icon: const Icon(Icons.check),
///   onPressed: confirm,
/// );
/// ```
class MSuccessButton extends MButton {
  /// Creates a success button with a text label.
  const new({
    required super.label,
    required super.onPressed,
    super.key,
    super.iconPlacement,
    super.style,
  });

  /// Creates a success button that renders an icon alongside its label.
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
    return FilledButton(
      style: style?.merge(MButtonStyles.of(context).success),
      onPressed: effectiveOnPressed,
      child: child,
    );
  }
}
