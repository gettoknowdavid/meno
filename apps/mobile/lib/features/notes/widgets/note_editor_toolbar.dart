import 'package:flutter_quill/flutter_quill.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

class NoteEditorToolbar extends StatelessWidget {
  const new({
    required this.controller,
    required this.quillFocus,
    super.key,
  });

  final QuillController controller;
  final FocusNode quillFocus;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: quillFocus,
      builder: (context, child) => AnimatedSize(
        duration: const Duration(milliseconds: 180),
        curve: Curves.easeOut,
        child: quillFocus.hasFocus ? child : const SizedBox.shrink(),
      ),
      child: _ToolbarContent(controller: controller),
    );
  }
}

class _ToolbarContent extends StatelessWidget {
  const new({required this.controller});

  final QuillController controller;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    return SafeArea(
      top: false,
      child: SizedBox(
        height: 56,
        child: Card(
          color: colors.surfaceTint,
          shape: const RoundedRectangleBorder(borderRadius: Corners.circle),
          child: QuillSimpleToolbar(
            controller: controller,
            config: const QuillSimpleToolbarConfig(
              showRedo: false,
              showUndo: false,
              showFontFamily: false,
              showFontSize: false,
              showStrikeThrough: false,
              showInlineCode: false,
              showSubscript: false,
              showSuperscript: false,
              showColorButton: false,
              showBackgroundColorButton: false,
              showClearFormat: false,
              showDividers: false,
              showHeaderStyle: false,
              showListCheck: false,
              showCodeBlock: false,
              showLink: false,
              showSearchButton: false,
              showQuote: false,
              showLeftAlignment: false,
              showRightAlignment: false,
              showCenterAlignment: false,
              showIndent: false,
            ),
          ),
        ),
      ),
    );
  }
}
