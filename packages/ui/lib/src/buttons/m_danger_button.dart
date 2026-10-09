import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/buttons/m_button.dart';
import 'package:meno_ui/src/theme/extensions/m_button_styles.dart';

/// The danger action button: a filled button in the error colour.
///
/// It marks a destructive or irreversible action, such as deleting.
///
/// Example usage:
/// ```dart
/// MDangerButton(
///   label: 'Delete',
///   onPressed: delete,
/// );
/// ```
///
/// For an icon button version:
/// ```dart
/// MDangerButton.icon(
///   label: 'Delete',
///   icon: const Icon(Icons.delete),
///   onPressed: delete,
/// );
/// ```
class MDangerButton extends MButton {
  /// Creates a danger button with a text label.
  const new({
    required super.label,
    required super.onPressed,
    super.key,
    super.iconPlacement,
    super.loading,
    super.style,
  });

  /// Creates a danger button that renders an icon alongside its label.
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
      style: style?.merge(MButtonStyles.of(context).danger),
      onPressed: effectiveOnPressed,
      child: child,
    );
  }
}
