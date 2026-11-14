import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:intl/intl.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/notes/manager/_manager.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/services/_services.dart';
import 'package:meno_mobile/features/notes/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';

class FolderPage extends WatchingWidget {
  const new({required this.folderIdStr, super.key});

  final String folderIdStr;

  @override
  Widget build(BuildContext context) {
    pushScope(
      init: (getIt) {
        getIt.registerLazySingleton<FolderManager>(() {
          return FolderManager(
            http: di<NotesHttpService>(),
            local: di<NotesLocalService>(),
            folders: di<FoldersManager>(),
            logger: di<MenoLogger>(),
            folderId: .fromString(folderIdStr),
          );
        }, onCreated: (instance) => instance.initialize.run());
      },
    );

    return const _FolderPageContent();
  }
}

class _FolderPageContent extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final scrollController = createOnce(ScrollController.new);

    final isLoading = watch(di<FolderManager>().initialize.isRunning).value;
    final folder = watch(di<FolderManager>().folder).value;

    return RefreshIndicator(
      onRefresh: () async {},
      child: Scaffold(
        appBar: AppBar(
          toolbarHeight: 42,
          leadingWidth: 90,
          leading: const MCustomBackButton(title: 'Folders'),
          actions: isLoading ? null : [_PageOptionsButton(folder: folder)],
        ),

        body: CustomScrollView(
          controller: scrollController,
          slivers: [
            if (isLoading)
              const SliverToBoxAdapter(
                child: Center(child: MLoadingIndicator.box()),
              )
            else ...[
              const SliverToBoxAdapter(
                child: Padding(
                  padding: .symmetric(horizontal: 16),
                  child: _FolderWidget(),
                ),
              ),
              const SliverToBoxAdapter(child: Spaces.verticalLarge),
              const SliverToBoxAdapter(
                child: Padding(
                  padding: .symmetric(horizontal: 16),
                  child: _SearchBox(),
                ),
              ),
              const SliverFillRemaining(child: _FolderNotesList()),
            ],
          ],
        ),
      ),
    );
  }
}

class _FolderWidget extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final folder = watch(di<FolderManager>().folder).value;
    final notesCount = watch(di<FolderManager>().totalNotesCount).value;

    return RawMaterialButton(
      onPressed: null,
      shape: const FolderWidgetBorder(),
      fillColor: colors.primary,
      elevation: 0,
      hoverElevation: 0,
      focusElevation: 0,
      highlightElevation: 0,
      child: Container(
        height: 88,
        width: double.infinity,
        padding: const EdgeInsets.fromLTRB(16, 22, 16, 16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            MText(
              folder.title.getOrElse((_) => ''),
              style: textTheme.subheadingMedium,
              color: colors.onPrimary,
            ),
            MText(
              _formatNotesNumber(notesCount),
              style: textTheme.captionMedium,
              color: colors.onPrimary,
            ),
          ],
        ),
      ),
    );
  }

  String _formatNotesNumber(int numberOfNotes) {
    return Intl.plural(
      numberOfNotes,
      zero: 'No notes',
      one: '1 note',
      other: '$numberOfNotes notes',
    );
  }
}

class _PageOptionsButton extends StatelessWidget {
  const new({required this.folder});

  final NoteFolder folder;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final folderId = folder.id.getOrCrash;

    return IconButton(
      icon: const Icon(MIcons.dots_horizontal),
      onPressed: () => showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        useRootNavigator: true,
        builder: (context) => MModal(
          builder: (ctx) => Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              MModalListTile(
                leading: const Icon(MIcons.file_plus_02),
                title: 'Add Notes',
                onTap: () async {
                  final result = await SelectNotesModal.show(ctx, folder.id);
                  if (result == null) return;
                  di<NoteActionsManager>().assignNotesToFolder.run((
                    noteIds: result,
                    targetFolderId: folder.id,
                  ));
                },
              ),
              Spaces.verticalSmall,
              MModalListTile(
                leading: const Icon(MIcons.edit_05),
                title: 'Rename Folder',
                onTap: () => FolderEditorModal.show(ctx, folderId),
              ),
              Spaces.verticalSmall,
              MModalListTile(
                leading: Icon(MIcons.trash, color: colors.error),
                title: 'Delete',
                titleColor: colors.error,
                onTap: () => _deleteFolder(context),
              ),
              Spaces.verticalLarge,
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _deleteFolder(BuildContext context) async {
    final result = await DeleteAlertDialog.show(
      context,
      title: 'Delete Folder?',
      description: 'Do want to delete this folder?',
    );

    if (result) {
      di<FoldersManager>().deleteFolder.run(folder.id);
      // The folder is gone, so there is nothing left for this page to show:
      // it returns to the list the folder was opened from.
      if (context.mounted) context.pop();
    }
  }
}

class _SearchBox extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final manager = di<FolderManager>();
    final query = watch(di<FolderManager>().searchQuery).value;
    final controller = createOnce(() => TextEditingController(text: query));

    Timer? debounce;

    void onChanged(String value) {
      debounce?.cancel();
      debounce = Timer(const Duration(milliseconds: 400), () {
        manager.performSearch.run(value);
      });
    }

    onDispose(() => debounce?.cancel());

    final textTheme = MTextTheme.of(context);

    return SizedBox(
      height: 40,
      child: SearchBar(
        elevation: const WidgetStatePropertyAll(0),
        controller: controller,
        onChanged: onChanged,
        hintText: 'Search for note',
        hintStyle: WidgetStatePropertyAll(textTheme.captionRegular),
        padding: const WidgetStatePropertyAll(
          EdgeInsets.symmetric(horizontal: Insets.md),
        ),
        leading: const Icon(MIcons.search, size: Insets.lg),
        trailing: [
          if (controller.text.isNotEmpty)
            IconButton(
              icon: const Icon(Icons.clear, size: Insets.lg),
              onPressed: () {
                controller.clear();
                manager.searchQuery.value = '';
                manager.performSearch.run('');
              },
            ),
        ],
        shape: const WidgetStatePropertyAll(
          RoundedRectangleBorder(
            side: BorderSide(color: Color(0xFFC2C7D0)),
            borderRadius: Corners.sm,
          ),
        ),
      ),
    );
  }
}

class _FolderNotesList extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext ctx) {
    final textTheme = MTextTheme.of(ctx);
    final folder = watch(di<FolderManager>().folder).value;
    final notes = watch(di<FolderManager>().notes).value;

    if (notes.isEmpty) {
      return Padding(
        padding: const .all(Insets.lg),
        child: MText(
          'No notes found in this folder',
          style: textTheme.bodyRegular,
          textAlign: TextAlign.center,
        ),
      );
    }

    return NotesListWidget(
      notes: notes,
      onNoteTap: (note) =>
          ctx.push(MRoutes.noteEditorRoute(note.id.getOrCrash)),
      onNoteOptionsTap: (note) => _NoteOptionsModal.show(ctx, note, folder.id),
    );
  }
}

class _NoteOptionsModal extends WatchingWidget {
  const new _({required this.note, this.folderId})
    : super(key: null);

  final Note note;
  final Id? folderId;

  static Future<dynamic> show(BuildContext context, Note note, [Id? folderId]) {
    return showModalBottomSheet<dynamic>(
      context: context,
      isScrollControlled: true,
      useRootNavigator: true,
      builder: (_) => _NoteOptionsModal._(note: note, folderId: folderId),
    );
  }

  @override
  Widget build(BuildContext context) {
    return MModal(
      builder: (context) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          MModalListTile(
            leading: const Icon(MIcons.file_02),
            title: 'Move Note',
            onTap: () => _moveNoteToFolder(context),
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.folder_x),
            title: 'Remove from Folder',
            onTap: () => _removeNoteFromFolder(context),
          ),
          Spaces.verticalSmall,
        ],
      ),
    );
  }

  Future<void> _moveNoteToFolder(BuildContext ctx) async {
    final folder = await SelectFolderModal.show(ctx, 'Move Note', isMove: true);
    if (folder == null) return;
    di<NoteActionsManager>().assignNotesToFolder.run((
      noteIds: [note.id],
      targetFolderId: folder.id,
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
}
