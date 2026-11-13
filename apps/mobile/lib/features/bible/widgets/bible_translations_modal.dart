import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/manager/_manager.dart';
import 'package:meno_mobile/features/bible/model/bible_rows.dart';
import 'package:meno_mobile/features/bible/widgets/translation_widget.dart';
import 'package:meno_ui/meno_ui.dart';

class BibleTranslationsModal extends StatelessWidget {
  const new _() : super(key: null);

  static Future<void> show(BuildContext context) {
    return showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useRootNavigator: true,
      builder: (_) => const BibleTranslationsModal._(),
    );
  }

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    return MModal(
      title: 'Bible Translations',
      builder: (context) => SingleChildScrollView(
        child: Column(
          crossAxisAlignment: .stretch,
          children: [
            MText('Downloaded', style: textTheme.microRegular),
            Spaces.verticalSmall,
            const _DownloadedList(),
            Spaces.verticalXXLarge,
            MText('Available Online', style: textTheme.microRegular),
            Spaces.verticalSmall,
            const _OnlineList(),
          ],
        ),
      ),
    );
  }
}

class _DownloadedList extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final translations = watchValue<TranslationsManager, List<BibleVersionRow>>(
      (m) => m.downloadedTranslations,
    );

    return ListView.separated(
      shrinkWrap: true,
      primary: false,
      physics: const NeverScrollableScrollPhysics(),
      itemCount: translations.length,
      separatorBuilder: (_, _) => Spaces.verticalLarge,
      itemBuilder: (context, index) {
        final t = translations[index];
        return TranslationWidget(
          key: ObjectKey(t.code),
          translation: t,
          onSelect: () => _applyTranslation(context, t),
        );
      },
    );
  }

  void _applyTranslation(BuildContext context, BibleVersionRow t) {
    final bible = di<BibleManager>();
    bible.translation.value = t.code;
    bible.refreshChapter.run();
    Navigator.of(context, rootNavigator: true).pop();
  }
}

class _OnlineList extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final translations = watchValue<TranslationsManager, List<BibleVersionRow>>(
      (m) => m.remoteTranslations,
    );

    return ListView.separated(
      shrinkWrap: true,
      primary: false,
      physics: const NeverScrollableScrollPhysics(),
      itemCount: translations.length,
      separatorBuilder: (_, _) => Spaces.verticalLarge,
      itemBuilder: (context, index) {
        final t = translations[index];
        return TranslationWidget(
          key: ObjectKey(t.code),
          translation: t,
          isDownloaded: false,
          onDownload: () => _download(context, t),
        );
      },
    );
  }

  void _download(BuildContext context, BibleVersionRow t) {
    di<TranslationsManager>().downloadTranslation.run(t.code);
  }
}
