import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// Confirms deleting a comment, and answers with the person's choice.
///
/// Returns `true` only on a deliberate delete: a dismissed dialog answers null,
/// which the caller must read as "no" rather than as a refusal it can ignore.
class DeleteCommentAlertDialog extends StatelessWidget {
  const new _({super.key});

  static Future<bool?> show(BuildContext context) {
    return showDialog<bool>(
      context: context,
      builder: (_) => const DeleteCommentAlertDialog._(
        key: Key('delete-comment-alert-dialog'),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return AlertDialog(
      title: MText('Delete Comment?', style: textTheme.heading2Regular),
      contentPadding: const EdgeInsets.all(24),
      content: MText(
        'Delete your comment permanently?',
        style: textTheme.captionRegular,
      ),
      actions: [
        SizedBox.fromSize(
          size: const Size(85, 40),
          child: MTextButton(
            label: 'Cancel',
            onPressed: () => Navigator.of(context).pop(false),
            style: TextButton.styleFrom(
              foregroundColor: colors.onDisabled.withValues(alpha: 0.5),
              shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
            ),
          ),
        ),
        SizedBox(
          height: 40,
          child: MDangerButton(
            label: 'Delete',
            onPressed: () => Navigator.of(context).pop(true),
            style: TextButton.styleFrom(
              backgroundColor: colors.error,
              foregroundColor: colors.onError,
              shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
            ),
          ),
        ),
      ],
    );
  }
}
