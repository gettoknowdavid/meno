import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';

class FolderTag extends StatelessWidget {
  const new({required this.folder, super.key});
  final NoteFolder folder;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        MTag(
          title: folder.title.getOrCrash,
          style: MTextTheme.of(context).microMedium,
        ),
      ],
    );
  }
}
