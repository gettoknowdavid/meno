import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/notes/manager/_manager.dart';
import 'package:meno_ui/meno_ui.dart';

class NewNoteButton extends WatchingWidget {
  const new _({required this.isAction, this.onTap});

  const new outlined({void Function()? onTap})
    : this._(isAction: false, onTap: onTap);

  const new action({void Function()? onTap})
    : this._(isAction: true, onTap: onTap);

  final void Function()? onTap;
  final bool isAction;

  @override
  Widget build(BuildContext context) {
    final totalCount = watch(di<NotesManager>().totalNotesCount).value;

    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    if (isAction) {
      if (totalCount < 1) return const SizedBox.shrink();

      return InkWell(
        onTap: onTap ?? () => context.push(MRoutes.noteEditorRoute()),
        child: Row(
          children: [
            Icon(MIcons.plus, size: 22, color: colors.primary),
            Spaces.horizontalMicro,
            MText(
              'Add New Note',
              style: textTheme.captionMedium,
              color: colors.primary,
            ),
          ],
        ),
      );
    }

    return SizedBox(
      width: 139,
      height: 32,
      child: MSecondaryButton.icon(
        label: 'Add New Note',
        icon: const Icon(MIcons.plus),
        style: OutlinedButton.styleFrom(
          textStyle: textTheme.microMedium,
          foregroundColor: colors.onBackground,
          iconColor: colors.onBackground,
          shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
          side: BorderSide(color: colors.outlineVariant3, width: 1.50),
        ),
        onPressed: onTap ?? () => context.push(MRoutes.noteEditorRoute()),
      ),
    );
  }
}
