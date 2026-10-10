import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/buttons/m_danger_button.dart';
import 'package:meno_ui/src/buttons/m_primary_button.dart';
import 'package:meno_ui/src/buttons/m_text_button.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The tone of a confirmation dialog's confirm action.
///
/// Use [danger] for destructive or irreversible actions such as deleting or
/// logging out.
enum MConfirmationDialogVariant {
  /// The confirm action uses the brand primary button.
  primary,

  /// The confirm action uses the danger button.
  danger,
}

/// The size of both action buttons, keeping them the same width.
const Size _kActionSize = Size(85, 40);

/// A confirmation dialog with a cancel and a confirm action.
///
/// Consolidates the confirmation, removal and deletion dialogs into one
/// configurable widget. Prefer [show] over constructing it directly: it returns
/// a `bool` (false when dismissed).
///
/// Example usage:
/// ```dart
/// final confirmed = await MConfirmationDialog.show(
///   context,
///   title: 'Delete broadcast?',
///   description: 'This cannot be undone.',
///   confirmLabel: 'Delete',
///   variant: MConfirmationDialogVariant.danger,
/// );
/// ```
class MConfirmationDialog extends StatelessWidget {
  /// Creates a confirmation dialog.
  ///
  /// [title] and [description] are the dialog copy, [confirmLabel] and
  /// [cancelLabel] label the actions, and [variant] styles the confirm action.
  const new({
    required this.title,
    required this.description,
    super.key,
    this.confirmLabel = 'Confirm',
    this.cancelLabel = 'Cancel',
    this.variant = MConfirmationDialogVariant.primary,
  });

  /// The dialog title.
  final String title;

  /// The explanatory body copy.
  final String description;

  /// The label of the confirm action.
  final String confirmLabel;

  /// The label of the cancel action.
  final String cancelLabel;

  /// The tone of the confirm action.
  final MConfirmationDialogVariant variant;

  /// Shows the dialog and resolves to the user's choice.
  ///
  /// Returns true when confirmed and false when cancelled or dismissed.
  static Future<bool> show(
    BuildContext context, {
    required String title,
    required String description,
    String confirmLabel = 'Confirm',
    String cancelLabel = 'Cancel',
    MConfirmationDialogVariant variant = MConfirmationDialogVariant.primary,
  }) async {
    final result = await showDialog<bool>(
      context: context,
      builder: (_) => MConfirmationDialog(
        title: title,
        description: description,
        confirmLabel: confirmLabel,
        cancelLabel: cancelLabel,
        variant: variant,
      ),
    );
    return result ?? false;
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return AlertDialog(
      title: MText(title, style: textTheme.heading2Regular),
      contentPadding: const EdgeInsets.all(Insets.xl),
      content: MText(description, style: textTheme.captionRegular),
      actions: [
        SizedBox.fromSize(
          size: _kActionSize,
          child: MTextButton(
            label: cancelLabel,
            onPressed: () => Navigator.pop(context, false),
            style: TextButton.styleFrom(
              foregroundColor: colors.onDisabled.withValues(alpha: 0.5),
              shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
            ),
          ),
        ),
        SizedBox.fromSize(
          size: _kActionSize,
          child: _ConfirmButton(
            label: confirmLabel,
            variant: variant,
            onPressed: () => Navigator.pop(context, true),
          ),
        ),
      ],
    );
  }
}

/// The confirm action, styled by [variant].
class _ConfirmButton extends StatelessWidget {
  const new({
    required this.label,
    required this.variant,
    required this.onPressed,
  });

  final String label;
  final MConfirmationDialogVariant variant;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final style = FilledButton.styleFrom(
      shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
    );
    return switch (variant) {
      MConfirmationDialogVariant.primary => MPrimaryButton(
        label: label,
        onPressed: onPressed,
        style: style,
      ),
      MConfirmationDialogVariant.danger => MDangerButton(
        label: label,
        onPressed: onPressed,
        style: style,
      ),
    };
  }
}
