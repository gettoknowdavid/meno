import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/manager/bible_manager.dart';
import 'package:meno_ui/meno_ui.dart';

class ChaptersGrid extends WatchingWidget {
  const new({super.key, this.onChapterSelected});

  final VoidCallback? onChapterSelected;

  @override
  Widget build(BuildContext context) {
    final manager = di<BibleManager>();

    final currentChapter = watchValue<BibleManager, int>((m) => m.chapter);
    final chapterCount = manager.chapterCount;

    return GridView.builder(
      padding: const .symmetric(vertical: Insets.lg),
      primary: false,
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      itemCount: chapterCount,
      gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
        crossAxisCount: 6,
        crossAxisSpacing: Insets.md,
        mainAxisSpacing: Insets.md,
      ),
      itemBuilder: (context, index) {
        final chapter = index + 1;
        final isSelected = chapter == currentChapter;
        return _ChapterCell(
          chapter: chapter,
          isSelected: isSelected,
          onTap: () {
            manager.getVerses.run(
              BibleArgs(
                book: manager.book.value,
                chapter: chapter,
                translation: manager.translation.value,
              ),
            );
            onChapterSelected?.call();
            Navigator.of(context, rootNavigator: true).pop();
          },
        );
      },
    );
  }
}

class _ChapterCell extends StatelessWidget {
  const new({
    required this.chapter,
    required this.isSelected,
    required this.onTap,
  });

  final int chapter;
  final bool isSelected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    return InkWell(
      borderRadius: Corners.sm,
      onTap: onTap,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 150),
        alignment: .center,
        decoration: BoxDecoration(
          borderRadius: Corners.sm,
          color: isSelected
              ? colors.primary
              : colors.outlineVariant1.withValues(alpha: 0.5),
        ),
        child: MText('$chapter', color: isSelected ? colors.onPrimary : null),
      ),
    );
  }
}
