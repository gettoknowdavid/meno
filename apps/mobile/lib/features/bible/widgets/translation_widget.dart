import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/manager/_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// One translation, held or on offer.
///
/// Takes the code and name rather than a row because the two lists it appears
/// in hold different types: the device's own reads come from the store
/// (`BibleVersionRow`), the catalogue's from the API
/// (`BibleTranslationRelease`), and what this widget draws is the same either
/// way.
class TranslationWidget extends WatchingWidget {
  const new({
    required this.code,
    required this.name,
    super.key,
    this.isDownloaded = true,
    this.onSelect,
    this.onDownload,
    this.trailing,
  });

  /// The translation code, for example `kjv`.
  final String code;

  /// The name shown under the code.
  final String name;

  final bool isDownloaded;
  final VoidCallback? onSelect;
  final VoidCallback? onDownload;

  /// Actions drawn after the name, replacing the built-in download button.
  ///
  /// The settings page passes its update and remove actions here; the reader's
  /// bottom sheet leaves it null and keeps the plain card.
  final Widget? trailing;

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

    final abb = code;
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
                  MText(name, style: textTheme.microRegular),
                ],
              ),
            ),
            if (trailing != null)
              trailing!
            else if (!isDownloaded)
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
