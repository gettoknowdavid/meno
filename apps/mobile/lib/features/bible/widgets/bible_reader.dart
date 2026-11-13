import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/manager/_manager.dart';
import 'package:meno_mobile/features/bible/services/_services.dart';
import 'package:meno_mobile/features/bible/widgets/bible_verses.dart';
import 'package:meno_mobile/features/bible/widgets/scripture_picker.dart';
import 'package:meno_ui/meno_ui.dart';

/// The reader itself: the picker above, the chapter below.
///
/// Shared by the live room's Bible tab and by the standalone `/bible` page, so
/// both open on the same state and neither can drift from the other.
class BibleReader extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BibleManager>();

    // Opening the reader is what asks for the first chapter. The launch-time
    // seeding may still be running when a route is restored straight into here,
    // and `ensureSeeded` hands back that same run rather than starting another,
    // so the read waits for it instead of querying an empty store. A position
    // the reader already holds is left alone: coming back to the tab must not
    // send the user to Genesis.
    callOnce((_) async {
      await di<BibleSeedService>().ensureSeeded();
      if (manager.verses.value.isEmpty) {
        manager.getVerses.run(const BibleArgs());
      }
    });

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
