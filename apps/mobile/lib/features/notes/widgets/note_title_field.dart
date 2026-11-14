import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/notes/manager/note_editor_manager.dart';
import 'package:meno_ui/meno_ui.dart';

class NoteTitleField extends WatchingWidget {
  const new({required this.quillFocusNode, super.key});

  final FocusNode quillFocusNode;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    final manager = di<NoteEditorManager>();
    final isEdit = watch(di<NoteEditorManager>().isEdit).value;

    final controller = createOnce(() {
      final currentTitle = manager.note.value.title.getOrElse((_) => '');
      return TextEditingController(text: currentTitle);
    });

    final status = watch(di<NoteEditorManager>().status).value;

    return TextFormField(
      controller: controller,
      autofocus: !isEdit,
      style: textTheme.heading3Bold,
      textInputAction: .next,
      maxLines: null,
      keyboardType: .multiline,
      enabled: !status.isSaving,
      onChanged: manager.onTitleChanged,
      onFieldSubmitted: (_) => quillFocusNode.requestFocus(),
      decoration: InputDecoration(
        border: .none,
        errorBorder: .none,
        enabledBorder: .none,
        focusedBorder: .none,
        disabledBorder: .none,
        focusedErrorBorder: .none,
        hintText: 'Enter Title',
        contentPadding: .zero,
        hintStyle: textTheme.heading3Bold.copyWith(color: Colors.grey.shade400),
      ),
    );
  }
}
