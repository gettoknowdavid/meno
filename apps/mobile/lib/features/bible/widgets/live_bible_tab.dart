import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/widgets/bible_reader.dart';

/// The room's Bible tab.
///
/// The reader is the same widget the standalone page shows; the room's shell
/// supplies the tab bar and the app bar, so nothing here draws its own chrome.
class LiveBibleTab extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) => const BibleReader();
}
