import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/widgets/bible_verses.dart';
import 'package:meno_mobile/features/bible/widgets/scripture_picker.dart';
import 'package:meno_ui/meno_ui.dart';

class LiveBibleTab extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    return const Padding(
      padding: .symmetric(horizontal: Insets.lg),
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          ScripturePicker(),
          MDivider(bottomSpace: 16),
          Expanded(child: BibleVerses()),
        ],
      ),
    );
  }
}
