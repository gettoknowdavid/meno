import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/widgets/dynamic_sliver_app_bar.dart';
import 'package:meno_mobile/features/notes/notes.dart';
import 'package:meno_ui/meno_ui.dart';

class MyNotesPage extends WatchingWidget {
  const new({this.initialIndex = 0, super.key});

  final int initialIndex;

  @override
  Widget build(BuildContext context) {
    final pageCtrl = createOnce(PageController.new);
    final index = createOnce(() => ValueNotifier<int>(initialIndex));

    callAfterEveryBuild((context, cancel) {
      if (pageCtrl.hasClients && pageCtrl.page?.round() != index.value) {
        pageCtrl.animateToPage(
          index.value,
          duration: const Duration(milliseconds: 250),
          curve: Curves.easeInOut,
        );
      }
    });

    watch(index);

    // The two lists are read once per account and then kept up to date by the
    // managers; asking here is what makes them arrive. Folders first, because
    // a note's card shows the folder it is filed in.
    callOnce((_) async {
      await di<FoldersManager>().initialize.runAsync();
      await di<NotesManager>().initialize.runAsync();
    });

    return Scaffold(
      body: NestedScrollView(
        headerSliverBuilder: (context, innerBoxIsScrolled) => [
          _HeaderWidget(innerBoxIsScrolled: innerBoxIsScrolled, index: index),
        ],
        body: PageView(
          controller: pageCtrl,
          onPageChanged: (value) => index.value = value,
          children: const [_NotesListView(), _FoldersListView()],
        ),
      ),
    );
  }
}

class _HeaderWidget extends StatelessWidget {
  const new({required this.innerBoxIsScrolled, required this.index});

  final bool innerBoxIsScrolled;
  final ValueNotifier<int> index;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    return SliverOverlapAbsorber(
      handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
      sliver: DynamicSliverAppBar(
        key: const Key('NotesFolderAppBar'),
        backgroundColor: colors.background,
        surfaceTintColor: Colors.transparent,
        forceElevated: innerBoxIsScrolled,
        elevation: innerBoxIsScrolled ? 1 : 0,
        shadowColor: colors.onBackground.withValues(alpha: 0.08),
        expandedHeight: 158 + kTextTabBarHeight,
        title: const MHeader(
          title: MText('My Notes'),
          addTopMargin: true,
          padding: .zero,
        ),
        actions: [
          switch (index.value) {
            0 => const NewNoteButton.action(),
            1 => const NewFolderButton.action(),
            _ => const SizedBox(),
          },
          Spaces.horizontalLarge,
        ],
        bottom: PreferredSize(
          preferredSize: const .fromHeight(40),
          child: Container(
            color: colors.background,
            height: 40,
            child: switch (index.value) {
              0 => const NotesSearchBar(),
              1 => const FoldersSearchBar(),
              _ => const SizedBox(height: 40),
            },
          ),
        ),
        flexibleSpace: SafeArea(
          bottom: false,
          child: SingleChildScrollView(
            physics: const NeverScrollableScrollPhysics(),
            padding: const .fromLTRB(16, kToolbarHeight, 16, 0),
            child: Column(
              crossAxisAlignment: .start,
              children: [
                Row(
                  mainAxisAlignment: .spaceBetween,
                  crossAxisAlignment: .end,
                  children: [
                    Expanded(
                      child: NotesGroupWidget(
                        onTap: () => index.value = 0,
                        selected: index.value == 0,
                      ),
                    ),
                    Spaces.horizontalSmall,
                    Expanded(
                      child: FoldersGroupWidget(
                        onTap: () => index.value = 1,
                        selected: index.value == 1,
                      ),
                    ),
                  ],
                ),
                Spaces.verticalXXLarge,
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _NotesListView extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final notes = watch(di<NotesManager>().notes).value;
    final isLoading = watch(di<NotesManager>().initialize.isRunning).value;
    return NotesListWidget(
      notes: notes,
      isNested: true,
      isLoading: isLoading,
      onNoteTap: (note) =>
          context.push(MRoutes.noteEditorRoute(note.id.getOrCrash)),
      onNoteLongPress: (note) => NoteOptionsModal.show(context, note),
      onNoteOptionsTap: (note) => NoteOptionsModal.show(context, note),
    );
  }
}

class _FoldersListView extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final folders = watch(di<FoldersManager>().folders).value;
    final isLoading = watch(di<FoldersManager>().initialize.isRunning).value;
    return FoldersListWidget(
      folders: folders,
      isNested: true,
      isLoading: isLoading,
      onFolderTap: (folder) =>
          context.push(MRoutes.folderRoute(folder.id.getOrCrash)),
      onFolderOptionsTap: (folder) => FolderOptionsModal.show(context, folder),
    );
  }
}
