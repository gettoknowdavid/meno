import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/notes/widgets/new_note_button.dart';
import 'package:meno_ui/meno_ui.dart';

class EmptyNoteListWidget extends StatelessWidget {
  const new({this.showAddButton = true, super.key});

  final bool showAddButton;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    return Center(
      child: SizedBox(
        width: 266,
        child: Column(
          children: [
            if (showAddButton) Spaces.verticalXLarge,
            Assets.images.newFile.image(
              height: 120,
              width: 160,
              package: 'meno_ui',
            ),
            MText(
              'Welcome! Start writing down everything you take in.',
              style: textTheme.bodyRegular,
              textAlign: .center,
            ),
            Spaces.verticalXLarge,
            if (showAddButton) const NewNoteButton.outlined(),
          ],
        ),
      ),
    );
  }
}
