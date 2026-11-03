import 'dart:convert';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_quill/flutter_quill.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_shared/widgets/m_custom_back_button.dart';
import 'package:meno_mobile/features/notes/manager/note_editor_manager.dart';
import 'package:meno_mobile/features/notes/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';

class NoteEditorWidget extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    final manager = di<NoteEditorManager>();
    final scrollController = createOnce(ScrollController.new);
    final quillScrollController = createOnce(ScrollController.new);
    final quillFocusNode = createOnce(FocusNode.new);
    final quillController = createOnce(QuillController.basic);

    callOnce((_) {
      final content = manager.note.value.content;
      // Attempt to load note content if the note is an existing note
      // On failure or in case of a malformed note, show the error snack bar
      if (content.isValid) {
        try {
          final json = jsonDecode(content.getOrCrash) as List<dynamic>;
          quillController.document = Document.fromJson(json);
        } on Object catch (_) {}
      }

      // Listen for changes on the Quill Editor to update the note content
      quillController.document.changes.listen((change) {
        if (change.change.isEmpty) return;
        final delta = jsonEncode(quillController.document.toDelta().toJson());
        manager.onContentChanged(delta);
      });
    });

    final folder = watch(di<NoteEditorManager>().note).value.folder;
    final status = watch(di<NoteEditorManager>().status).value;

    const padding = EdgeInsets.symmetric(horizontal: Insets.lg);
    const contentPadding = EdgeInsets.symmetric(horizontal: 20);

    return Scaffold(
      appBar: AppBar(
        toolbarHeight: 42,
        leadingWidth: 90,
        leading: const MCustomBackButton(title: 'Notes'),
        actions: [
          NoteEditorAutosaveWidget(status: status),
          _DoneButton(focusNode: quillFocusNode),
          Spaces.horizontalLarge,
        ],
      ),
      body: Column(
        children: [
          Expanded(
            child: CustomScrollView(
              controller: scrollController,
              slivers: [
                SliverToBoxAdapter(
                  child: Padding(
                    padding: padding,
                    child: NoteTitleField(quillFocusNode: quillFocusNode),
                  ),
                ),
                if (folder != null) ...[
                  SliverToBoxAdapter(
                    child: Padding(
                      padding: padding,
                      child: FolderTag(folder: folder),
                    ),
                  ),
                  const SliverToBoxAdapter(child: Spaces.verticalLarge),
                ],
                SliverFillRemaining(
                  hasScrollBody: false,
                  child: Padding(
                    padding: contentPadding,
                    child: QuillEditor.basic(
                      controller: quillController,
                      focusNode: quillFocusNode,
                      scrollController: quillScrollController,
                      config: QuillEditorConfig(
                        padding: MediaQuery.viewInsetsOf(context),
                        placeholder: 'Start writing...',
                        expands: true,
                        customStyles: DefaultStyles(
                          paragraph: DefaultTextBlockStyle(
                            textTheme.bodyRegular,
                            HorizontalSpacing.zero,
                            VerticalSpacing.zero,
                            VerticalSpacing.zero,
                            null,
                          ),
                          placeHolder: DefaultTextBlockStyle(
                            textTheme.bodyRegular,
                            HorizontalSpacing.zero,
                            VerticalSpacing.zero,
                            VerticalSpacing.zero,
                            null,
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
          NoteEditorToolbar(
            controller: quillController,
            quillFocus: quillFocusNode,
          ),
        ],
      ),
    );
  }
}

class _DoneButton extends StatelessWidget {
  const new({required this.focusNode});

  final FocusNode focusNode;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final colors = MColorScheme.of(context);

    return ListenableBuilder(
      listenable: focusNode,
      builder: (context, _) {
        if (!focusNode.hasFocus) return const SizedBox.shrink();
        return TextButton(
          onPressed: focusNode.unfocus,
          child: MText(
            'Done',
            color: colors.primary,
            style: textTheme.captionMedium,
          ),
        );
      },
    );
  }
}
