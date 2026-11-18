import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/bible/manager/_manager.dart';
import 'package:meno_mobile/features/bible/model/_model.dart';
import 'package:meno_mobile/features/bible/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';

/// Everything this device holds, and everything the catalogue offers.
///
/// The reader's own bottom sheet only picks between translations; this page is
/// where one is added or given up. It is reached from Settings so a person can
/// find scripture without starting a broadcast first.
class BibleTranslationsPage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final manager = di<TranslationsManager>();

    final installed = watchValue<TranslationsManager, List<BibleVersionRow>>(
      (m) => m.downloadedTranslations,
    );

    final remote =
        watchValue<TranslationsManager, List<BibleTranslationRelease>>(
          (m) => m.remoteTranslations,
        );

    final downloading = watchValue<TranslationsManager, String?>(
      (m) => m.downloadingAbbreviation,
    );

    final removal =
        watchValue<TranslationsManager, CommandResult<String?, void>>(
          (m) => m.removeTranslation.results,
        );

    final catalogueUnavailable = watchValue<TranslationsManager, bool>(
      (m) => m.catalogueUnavailable,
    );

    final initialising = watchValue<TranslationsManager, bool>(
      (m) => m.initialize.isRunning,
    );

    registerHandler<TranslationsManager, CommandError<String>?>(
      select: (m) => m.downloadTranslation.errors,
      handler: (ctx, e, _) => ctx.showErrorToast(errorMessage(e?.error)),
    );

    registerHandler<TranslationsManager, CommandError<String>?>(
      select: (m) => m.removeTranslation.errors,
      handler: (ctx, e, _) => ctx.showErrorToast(errorMessage(e?.error)),
    );

    // Read again on every visit: a translation downloaded in the live room is
    // already in the store by the time this page opens, and the catalogue may
    // have changed since the manager was created.
    callOnce((_) => manager.initialize.run());

    final removingCode = removal.isRunning ? removal.paramData : null;
    final lastOne = installed.length <= 1;

    // A translation that is installed but out of date is updated from its own
    // row, so the online list offers only what is not on the device at all.
    final installedCodes = {for (final row in installed) row.code};
    final downloadable = [
      for (final release in remote)
        if (!installedCodes.contains(release.code)) release,
    ];

    return MScaffold(
      appBar: MAppBar.secondary(title: 'Bible Translations', centerTitle: true),
      body: RefreshIndicator(
        onRefresh: () => manager.initialize.runAsync(),
        child: ListView(
          children: [
            Spaces.verticalLarge,
            MText(
              'Scripture is kept on this device, so it reads without a '
              'connection. Choose what to keep here.',
              style: textTheme.captionRegular,
              color: colors.inActive,
            ),
            Spaces.verticalXLarge,
            MText('On this device', style: textTheme.microRegular),
            Spaces.verticalSmall,
            if (initialising && installed.isEmpty)
              const Padding(
                padding: .symmetric(vertical: Insets.xxl),
                child: Center(child: CircularProgressIndicator()),
              )
            else ...[
              for (final translation in installed) ...[
                TranslationWidget(
                  key: ObjectKey(translation.code),
                  code: translation.code,
                  name: translation.name,
                  onSelect: () => _select(translation),
                  trailing: _installedActions(
                    context,
                    manager: manager,
                    translation: translation,
                    removingCode: removingCode,
                    downloading: downloading,
                  ),
                ),
                Spaces.verticalMedium,
              ],
              if (lastOne)
                MText(
                  'At least one translation has to stay on this device. '
                  'Download another before removing this one.',
                  style: textTheme.microRegular,
                  color: colors.inActive,
                ),
            ],
            if (downloadable.isNotEmpty) ...[
              Spaces.verticalXLarge,
              MText('Available online', style: textTheme.microRegular),
              Spaces.verticalSmall,
              for (final release in downloadable) ...[
                TranslationWidget(
                  key: ObjectKey(release.code),
                  code: release.code,
                  name: release.name,
                  isDownloaded: false,
                  // One download at a time: the manager publishes progress for
                  // the single translation in flight, so another row offering
                  // its own button would lie about what is happening.
                  onDownload: downloading == null || downloading == release.code
                      ? () => manager.downloadTranslation.run(release.code)
                      : null,
                ),
                Spaces.verticalMedium,
              ],
            ] else if (!initialising) ...[
              Spaces.verticalXLarge,
              if (catalogueUnavailable)
                _CatalogueUnavailable(onRetry: () => manager.initialize.run())
              else
                MText(
                  'Every translation in the catalogue is already on this '
                  'device.',
                  style: textTheme.captionRegular,
                  color: colors.inActive,
                ),
            ],
            Spaces.verticalXXLarge,
          ],
        ),
      ),
    );
  }

  /// The actions beside an installed translation.
  ///
  /// Bundled translations say so instead of offering a removal that would seed
  /// them straight back; the last one left has the same problem for a different
  /// reason, so its removal is drawn but disabled.
  Widget _installedActions(
    BuildContext context, {
    required TranslationsManager manager,
    required BibleVersionRow translation,
    required String? removingCode,
    required String? downloading,
  }) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    if (removingCode == translation.code) {
      return const SizedBox.square(
        dimension: 20,
        child: CircularProgressIndicator(strokeWidth: 2),
      );
    }

    final update = manager.updateFor(translation.code);

    return Row(
      mainAxisSize: .min,
      children: [
        if (update != null)
          MTextButton(
            label: 'Update',
            onPressed: downloading == null
                ? () => manager.downloadTranslation.run(translation.code)
                : null,
          ),
        if (manager.isBundled(translation.code))
          MText(
            'Included',
            style: textTheme.microRegular,
            color: colors.onBackgroundVariant,
          )
        else
          IconButton(
            icon: const Icon(Icons.delete_outline),
            iconSize: 22,
            padding: .zero,
            style: IconButton.styleFrom(foregroundColor: colors.error),
            onPressed:
                manager.canRemove(translation.code) && downloading == null
                ? () => _confirmRemove(context, manager, translation)
                : null,
          ),
      ],
    );
  }

  /// Makes [translation] the one the reader opens on.
  void _select(BibleVersionRow translation) {
    final bible = di<BibleManager>();
    bible.translation.value = translation.code;
    bible.refreshChapter.run();
  }

  Future<void> _confirmRemove(
    BuildContext context,
    TranslationsManager manager,
    BibleVersionRow translation,
  ) async {
    final verses = manager.verseTotal(translation.code);
    final confirmed = await MConfirmationDialog.show(
      context,
      title: 'Remove ${translation.name}?',
      description:
          'Its $verses ${verses == 1 ? 'verse' : 'verses'} will be deleted '
          'from this device. You can download it again from this page.',
      confirmLabel: 'Remove',
      variant: MConfirmationDialogVariant.danger,
    );
    if (confirmed) manager.removeTranslation.run(translation.code);
  }
}

/// The catalogue could not be reached, with a way to try again.
class _CatalogueUnavailable extends StatelessWidget {
  const new({required this.onRetry});

  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Row(
      children: [
        Expanded(
          child: MText(
            'The translations you could download are unavailable right now.',
            style: textTheme.captionRegular,
            color: colors.inActive,
          ),
        ),
        MTextButton(label: 'Retry', onPressed: onRetry),
      ],
    );
  }
}
