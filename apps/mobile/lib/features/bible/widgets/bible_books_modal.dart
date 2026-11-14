import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/manager/_manager.dart';
import 'package:meno_mobile/features/bible/widgets/book_widget.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:scroll_to_index/scroll_to_index.dart';

const double _rowHeight = 56;
const double _separatorHeight = 10;

class BibleBooksModal extends WatchingWidget {
  const new _() : super(key: null);

  static Future<void> show(BuildContext context) {
    final size = MediaQuery.sizeOf(context);
    return showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useRootNavigator: true,
      constraints: BoxConstraints(maxHeight: size.height * 0.9),
      builder: (_) => const BibleBooksModal._(),
    );
  }

  @override
  Widget build(BuildContext context) {
    final manager = di<BibleManager>();

    final books = manager.bookNames;

    final controller = createOnce<AutoScrollController>(() {
      return AutoScrollController(
        suggestedRowHeight: _rowHeight + _separatorHeight,
        viewportBoundaryGetter: () =>
            Rect.fromLTRB(0, 0, 0, MediaQuery.paddingOf(context).bottom),
      );
    });

    Future<void> scrollToCurrent() async {
      final index = books.indexOf(manager.currentBookName);
      if (index < 0 || !controller.hasClients) return;

      await controller.scrollToIndex(
        index,
        preferPosition: .begin,
        duration: const Duration(milliseconds: 350),
      );
    }

    callOnceAfterThisBuild((context) => scrollToCurrent());

    return MModal(
      title: 'Bible Books',
      builder: (context) => ListView.separated(
        controller: controller,
        itemCount: books.length,
        separatorBuilder: (_, _) => const SizedBox(height: _separatorHeight),
        itemBuilder: (context, index) {
          final bookName = books[index];
          return AutoScrollTag(
            key: ValueKey(bookName),
            controller: controller,
            index: index,
            child: BookWidget(
              key: ValueKey(bookName),
              bookName: bookName,
              onChapterSelected: scrollToCurrent,
            ),
          );
        },
      ),
    );
  }
}
