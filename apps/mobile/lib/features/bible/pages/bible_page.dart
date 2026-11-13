import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/widgets/bible_reader.dart';
import 'package:meno_ui/meno_ui.dart';

/// The reader on its own, behind `/bible`.
///
/// The live room reaches the same [BibleReader] through its Bible tab, where
/// the room's shell supplies the app bar; here the page supplies it, so a deep
/// link and a tab both land on a reader rather than a placeholder.
class BiblePage extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: MAppBar.secondary(title: 'Bible'),
      body: const SafeArea(bottom: false, child: BibleReader()),
    );
  }
}
