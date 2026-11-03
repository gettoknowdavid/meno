import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/widgets/_widgets.dart';
import 'package:meno_mobile/features/notes/manager/_manager.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';

class NoteOptionsModal extends WatchingWidget {
  const new _({required this.note, this.folderId})
    : super(key: null);

  final Note note;
  final Id? folderId;

  static Future<dynamic> show(BuildContext context, Note note, [Id? folderId]) {
    return showModalBottomSheet<dynamic>(
      context: context,
      isScrollControlled: true,
      useRootNavigator: true,
      builder: (_) => NoteOptionsModal._(note: note, folderId: folderId),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    return MModal(
      builder: (context) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (note.folder == null)
            MModalListTile(
              leading: const Icon(MIcons.plus),
              title: 'Add to Folder',
              onTap: () => _addNoteToFolder(context),
            )
          else
            MModalListTile(
              leading: const Icon(MIcons.x_close),
              title: 'Remove from Folder',
              onTap: () => _removeNoteFromFolder(context),
            ),

          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.share),
            title: 'Share',
            onTap: () {},
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.link_02),
            title: 'Copy Link',
            onTap: () {},
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: Icon(MIcons.trash, color: colors.error),
            title: 'Delete',
            titleColor: colors.error,
            onTap: () => _deleteNote(context),
          ),
          Spaces.verticalSmall,
        ],
      ),
    );
  }

  Future<void> _addNoteToFolder(BuildContext ctx) async {
    final targetFolder = await SelectFolderModal.show(
      ctx,
      'Add to Folder',
      isMove: true,
    );
    if (targetFolder == null) return;
    di<NoteActionsManager>().assignNotesToFolder.run((
      noteIds: [note.id],
      targetFolderId: targetFolder.id,
    ));
    if (ctx.mounted) ctx.pop();
  }

  Future<void> _removeNoteFromFolder(BuildContext context) async {
    final folderId = note.folder?.id;
    if (folderId == null) return;

    final result = await DeleteAlertDialog.show(
      context,
      title: 'Remove Note',
      description: 'Do you want to remove this note from this folder?',
    );

    if (!result) return;

    di<NoteActionsManager>().removeNotesFromFolder.run((
      noteIds: [note.id],
      folderId: folderId,
    ));
    if (context.mounted) context.pop();
  }

  Future<void> _deleteNote(BuildContext context) async {
    final result = await DeleteAlertDialog.show(
      context,
      title: 'Delete Note?',
      description: 'Do want to delete this note?',
    );

    if (!result) return;
    di<NoteActionsManager>().deleteNote.run(note);
    if (context.mounted) context.pop();
  }
}
