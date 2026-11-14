import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/notes/manager/note_editor_manager.dart';
import 'package:meno_ui/meno_ui.dart';

class NoteEditorAutosaveWidget extends StatelessWidget {
  const new({required this.status, super.key});

  final NoteEditorStatus status;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final colors = MColorScheme.of(context);

    return AnimatedSwitcher(
      duration: const Duration(milliseconds: 200),
      child: switch (status) {
        NoteEditorStatus.saving => MText(
          'Saving…',
          key: const ValueKey('saving'),
          color: colors.primary,
          style: textTheme.captionMedium,
        ),
        NoteEditorStatus.saved => MText(
          'Saved',
          key: const ValueKey('saved'),
          color: colors.onBackgroundVariant,
          style: textTheme.captionMedium,
        ),
        NoteEditorStatus.failure => MText(
          'Save failed',
          key: const ValueKey('failure'),
          color: colors.error,
          style: textTheme.captionMedium,
        ),
        _ => const SizedBox.shrink(key: ValueKey('none')),
      },
    );
  }
}
