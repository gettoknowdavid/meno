import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/notes/notes.dart';
import 'package:meno_ui/meno_ui.dart';

/// Which collection a [NewNotesButton] adds to.
enum NotesCollection {
  /// The notes tab.
  notes,

  /// The folders tab.
  folders,
}

/// Adds an item to one of the notes tabs.
///
/// The notes and folders buttons were two files that differed in their label,
/// their count and their destination, and copied the same row, the same button
/// style and the same "nothing yet, so hide the action" rule. One widget with
/// [NotesCollection] carries both.
class NewNotesButton extends WatchingWidget {
  const new _({
    required this.collection,
    required this.isAction,
    this.onTap,
    super.key,
  });

  /// The inline "Add New …" row, drawn only once the collection has an item.
  const new action(
    NotesCollection collection, {
    void Function()? onTap,
    Key? key,
  }) : this._(
         collection: collection,
         isAction: true,
         onTap: onTap,
         key: key,
       );

  /// The bordered button an empty state mounts.
  const new outlined(
    NotesCollection collection, {
    void Function()? onTap,
    Key? key,
  }) : this._(
         collection: collection,
         isAction: false,
         onTap: onTap,
         key: key,
       );

  /// Which tab this button adds to.
  final NotesCollection collection;

  /// Whether to draw the inline row rather than the bordered button.
  final bool isAction;

  /// Overrides what the tap does; the tab's own action is the default.
  final void Function()? onTap;

  @override
  Widget build(BuildContext context) {
    final totalCount = watch(switch (collection) {
      .notes => di<NotesManager>().totalNotesCount,
      .folders => di<FoldersManager>().totalFoldersCount,
    }).value;

    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final (actionLabel, outlinedLabel) = switch (collection) {
      .notes => ('Add New Note', 'Add New Note'),
      .folders => ('Add New Folder', 'Create New Folder'),
    };

    final void Function() add =
        onTap ??
        switch (collection) {
          .notes => () => context.push(MRoutes.noteEditorRoute()),
          .folders => () => FolderEditorModal.show(context),
        };

    if (isAction) {
      if (totalCount < 1) return const SizedBox.shrink();

      return InkWell(
        onTap: add,
        child: Row(
          children: [
            Icon(MIcons.plus, size: 22, color: colors.primary),
            Spaces.horizontalMicro,
            MText(
              actionLabel,
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
        label: outlinedLabel,
        icon: const Icon(MIcons.plus),
        style: OutlinedButton.styleFrom(
          textStyle: textTheme.microMedium,
          foregroundColor: colors.onBackground,
          iconColor: colors.onBackground,
          shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
          side: BorderSide(color: colors.outlineVariant3, width: 1.50),
        ),
        onPressed: add,
      ),
    );
  }
}
