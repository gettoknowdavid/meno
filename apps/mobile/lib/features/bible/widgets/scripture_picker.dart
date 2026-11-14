import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/manager/_manager.dart';
import 'package:meno_mobile/features/bible/widgets/bible_books_modal.dart';
import 'package:meno_mobile/features/bible/widgets/bible_translations_modal.dart';
import 'package:meno_ui/meno_ui.dart';

class ScripturePicker extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    return const SizedBox(
      height: 56,
      child: Row(
        children: [
          Expanded(
            child: Row(
              children: [
                _ScriptureReference(),
                Spaces.horizontalSmall,
                _ScriptureTranslation(),
              ],
            ),
          ),
          Spaces.horizontalSmall,
          _PreviousAndNextButtons(),
        ],
      ),
    );
  }
}

class _ScriptureReference extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final reference = watchValue<BibleManager, String>((m) => m.reference);
    return _Chip(label: reference, onTap: () => BibleBooksModal.show(context));
  }
}

class _ScriptureTranslation extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final translation = watchValue<BibleManager, String>((m) => m.translation);
    return _Chip(
      label: translation.toUpperCase(),
      onTap: () => BibleTranslationsModal.show(context),
    );
  }
}

class _PreviousAndNextButtons extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final style = IconButton.styleFrom(backgroundColor: colors.outlineVariant2);

    watchValue<BibleManager, int>((m) => m.book);
    watchValue<BibleManager, int>((m) => m.chapter);

    final manager = di<BibleManager>();

    return Row(
      children: [
        SizedBox.square(
          dimension: 32,
          child: IconButton.filled(
            icon: const Icon(MIcons.chevron_left),
            padding: .zero,
            iconSize: 20,
            style: style,
            onPressed: manager.isFirstChapter ? null : manager.prevChapter.run,
          ),
        ),
        const SizedBox(width: 13),
        SizedBox.square(
          dimension: 32,
          child: IconButton.filled(
            icon: const Icon(MIcons.chevron_right),
            padding: .zero,
            iconSize: 20,
            style: style,
            onPressed: manager.isLastChapter ? null : manager.nextChapter.run,
          ),
        ),
      ],
    );
  }
}

class _Chip extends StatelessWidget {
  const new({required this.label, required this.onTap});

  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return GestureDetector(
      onTap: onTap,
      child: Container(
        padding: const .symmetric(horizontal: 12, vertical: 6),
        decoration: BoxDecoration(
          color: colors.outlineVariant2,
          borderRadius: Corners.circle,
        ),
        child: MText(label, style: textTheme.captionMedium),
      ),
    );
  }
}
