import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/manager/_manager.dart';
import 'package:meno_mobile/features/bible/model/bible_rows.dart';
import 'package:meno_ui/meno_ui.dart';

class TranslationWidget extends WatchingWidget {
  const new({
    required this.translation,
    super.key,
    this.isDownloaded = true,
    this.onSelect,
    this.onDownload,
  });

  final BibleVersionRow translation;
  final bool isDownloaded;
  final VoidCallback? onSelect;
  final VoidCallback? onDownload;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final translationsManager = di<TranslationsManager>();

    final currentTranslation = watchValue<BibleManager, String>(
      (m) => m.translation,
    );

    final downloadProgress = watchValue<TranslationsManager, double>(
      (m) => m.downloadTranslation.progress,
    );

    final downloadingAbbr = watchValue<TranslationsManager, String?>(
      (m) => m.downloadingAbbreviation,
    );

    final abb = translation.code;
    final isSelected = currentTranslation == abb;
    final isThisDownloading = downloadingAbbr == abb;

    final borderRadius = BorderRadius.circular(24);

    return InkWell(
      onTap: isDownloaded ? onSelect : null,
      borderRadius: borderRadius,
      child: Container(
        height: 66,
        padding: const .symmetric(horizontal: 24, vertical: 12),
        decoration: BoxDecoration(
          borderRadius: borderRadius,
          border: isSelected ? Border.all(color: colors.primary) : null,
          color: isSelected ? colors.primaryContainer : colors.outlineVariant2,
        ),
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: .start,
                mainAxisAlignment: .center,
                children: [
                  MText(abb.toUpperCase(), style: textTheme.bodyMedium),
                  MText(translation.name, style: textTheme.microRegular),
                ],
              ),
            ),
            if (!isDownloaded)
              _DownloadButton(
                isDownloading: isThisDownloading,
                progress: isThisDownloading ? downloadProgress.toInt() : null,
                onDownload: onDownload,
                onCancel: translationsManager.downloadTranslation.cancel,
              ),
          ],
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------

class _DownloadButton extends StatelessWidget {
  const new({
    required this.isDownloading,
    this.progress,
    this.onDownload,
    this.onCancel,
  });

  final bool isDownloading;
  final int? progress;
  final VoidCallback? onDownload;
  final VoidCallback? onCancel;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    if (!isDownloading) {
      return IconButton(
        icon: const Icon(Icons.download_outlined),
        iconSize: 24,
        padding: .zero,
        onPressed: onDownload,
      );
    }

    return SizedBox.square(
      dimension: 40,
      child: Stack(
        alignment: .center,
        children: [
          CircularProgressIndicator(
            value: (progress ?? 0) / 100.0,
            strokeCap: StrokeCap.round,
          ),
          IconButton(
            icon: const Icon(Icons.stop),
            iconSize: 20,
            padding: .zero,
            style: IconButton.styleFrom(foregroundColor: colors.error),
            onPressed: onCancel,
          ),
        ],
      ),
    );
  }
}
