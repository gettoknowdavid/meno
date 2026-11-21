import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// What the host chose to do with a removed cohost.
enum RemoveCohostChoice {
  /// Drop the cohost role but leave them listening in the room.
  keepListening,

  /// Drop the cohost role and evict them from the room.
  removeFromRoom,
}

/// Confirms removing [name] as a cohost and asks what happens to their room
/// connection.
///
/// The two destructive outcomes are separate choices rather than a checkbox:
/// keeping a removed cohost listening is safe, while evicting them is not, and
/// an ambiguous default is the wrong way to decide between them.
///
/// A three-way question cannot be `MConfirmationDialog`, which is deliberately
/// two actions, so this keeps that dialog's copy and action styling instead: a
/// dimmed cancel first, then the two outcomes.
class RemoveCohostDialog extends StatelessWidget {
  const new _({required this.name}) : super(key: null);

  /// The cohost's display name, for the confirmation copy.
  final String name;

  /// Opens the dialog, answering null when it is dismissed.
  static Future<RemoveCohostChoice?> show(BuildContext context, String name) {
    return showDialog<RemoveCohostChoice>(
      context: context,
      builder: (_) => RemoveCohostDialog._(name: name),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return AlertDialog(
      title: MText('Remove co-host?', style: textTheme.heading2Regular),
      contentPadding: const .all(Insets.xl),
      content: MText(
        '$name will no longer be able to publish. They can stay in the room '
        'as a listener, or be removed from the room as well.',
        style: textTheme.captionRegular,
      ),
      actions: [
        MTextButton(
          label: 'Cancel',
          onPressed: () => Navigator.pop(context),
          style: TextButton.styleFrom(
            foregroundColor: colors.onDisabled.withValues(alpha: 0.5),
            shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
          ),
        ),
        MTextButton(
          label: 'Keep listening',
          onPressed: () =>
              Navigator.pop(context, RemoveCohostChoice.keepListening),
        ),
        MDangerButton(
          label: 'Remove from room',
          onPressed: () =>
              Navigator.pop(context, RemoveCohostChoice.removeFromRoom),
        ),
      ],
    );
  }
}
