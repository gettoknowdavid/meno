import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/manager/bible_manager.dart';
import 'package:meno_mobile/features/bible/widgets/chapters_grid.dart';
import 'package:meno_ui/meno_ui.dart';

class BookWidget extends WatchingWidget {
  const new({required this.bookName, super.key, this.onChapterSelected});

  final String bookName;

  final VoidCallback? onChapterSelected;

  @override
  Widget build(BuildContext context) {
    final manager = di<BibleManager>();

    final currentBookIdx = watchValue<BibleManager, int>((m) => m.book);
    final isExpanded = manager.bookName(currentBookIdx) == bookName;

    return Column(
      crossAxisAlignment: .stretch,
      mainAxisSize: .min,
      children: [
        _BookRow(
          bookName: bookName,
          isExpanded: isExpanded,
          onTap: () {
            if (isExpanded) return;

            final names = manager.bookNames;
            final idx = names.indexOf(bookName);
            if (idx == -1) return;
            manager.getVerses.run(
              BibleArgs(book: idx, translation: manager.translation.value),
            );
          },
        ),
        AnimatedSize(
          duration: const Duration(milliseconds: 200),
          curve: Curves.easeInOut,
          child: isExpanded
              ? ChaptersGrid(onChapterSelected: onChapterSelected)
              : const SizedBox.shrink(),
        ),
      ],
    );
  }
}

class _BookRow extends StatelessWidget {
  const new({
    required this.bookName,
    required this.isExpanded,
    required this.onTap,
  });

  final String bookName;
  final bool isExpanded;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return InkWell(
      borderRadius: Corners.sm,
      onTap: onTap,
      child: Container(
        height: 56,
        padding: const .symmetric(horizontal: 12, vertical: 16),
        decoration: BoxDecoration(
          color: isExpanded ? colors.primaryContainer : colors.outlineVariant2,
          borderRadius: Corners.sm,
        ),
        child: Row(
          mainAxisAlignment: .spaceBetween,
          children: [
            MText(
              bookName,
              style: isExpanded ? textTheme.bodyMedium : textTheme.bodyRegular,
              color: isExpanded ? colors.primary : null,
            ),
            AnimatedRotation(
              turns: isExpanded ? 0.125 : 0, // 45° when open
              duration: const Duration(milliseconds: 200),
              child: Icon(
                MIcons.plus,
                size: 20,
                color: isExpanded ? colors.primary : null,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
