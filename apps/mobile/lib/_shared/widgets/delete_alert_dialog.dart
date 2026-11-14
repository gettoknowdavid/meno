import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// The confirmation a destructive action asks for before it runs.
///
/// A thin name over [MConfirmationDialog]'s danger variant, kept because the
/// callers read better as "ask to delete" than as "show a confirmation with
/// these three options". Resolves to `true` only when the user confirmed.
abstract final class DeleteAlertDialog {
  /// Asks whether [title] should be deleted, explaining with [description].
  ///
  /// [confirmLabel] is the verb on the button — "Delete" by default, and
  /// something more specific ("Remove") where the row survives.
  static Future<bool> show(
    BuildContext context, {
    required String title,
    required String description,
    String confirmLabel = 'Delete',
  }) {
    return MConfirmationDialog.show(
      context,
      title: title,
      description: description,
      confirmLabel: confirmLabel,
      variant: MConfirmationDialogVariant.danger,
    );
  }
}
