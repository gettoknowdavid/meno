import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/widgets/delete_alert_dialog.dart';
import 'package:meno_mobile/features/notes/manager/_manager.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';

class FolderOptionsModal extends WatchingWidget {
  const new _({required this.folder}) : super(key: null);

  final NoteFolder folder;

  static Future<dynamic> show(BuildContext context, NoteFolder folder) {
    return showModalBottomSheet<dynamic>(
      context: context,
      isScrollControlled: true,
      useRootNavigator: true,
      builder: (context) => FolderOptionsModal._(folder: folder),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    return MModal(
      builder: (ctx) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          MModalListTile(
            leading: const Icon(MIcons.file_plus_02),
            title: 'Add Notes',
            onTap: () => _addNoteToFolder(context),
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.edit_05),
            title: 'Rename Folder',
            onTap: () => FolderEditorModal.show(ctx, folder.id.getOrCrash),
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: Icon(MIcons.trash, color: colors.error),
            title: 'Delete',
            titleColor: colors.error,
            onTap: () => _deleteFolder(context),
          ),
          Spaces.verticalSmall,
        ],
      ),
    );
  }

  Future<void> _addNoteToFolder(BuildContext context) async {
    final folderId = folder.id;
    final results = await SelectNotesModal.show(context, folderId);
    if (results == null || results.isEmpty) return;
    di<NoteActionsManager>().assignNotesToFolder.run((
      noteIds: results,
      targetFolderId: folderId,
    ));
    if (context.mounted) context.pop();
  }

  Future<void> _deleteFolder(BuildContext context) async {
    final result = await DeleteAlertDialog.show(
      context,
      title: 'Delete Folder?',
      description: 'Do want to delete this folder?',
    );

    if (!result) return;
    di<FoldersManager>().deleteFolder.run(folder.id);
    if (context.mounted) context.pop();
  }
}
