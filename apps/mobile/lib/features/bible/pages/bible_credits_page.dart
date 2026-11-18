import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/manager/_manager.dart';
import 'package:meno_mobile/features/bible/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';

/// Who to credit for the scripture this device holds.
///
/// Every translation ships with its own copyright line, and that line is part
/// of the licence it is distributed under — this is the one screen where it is
/// shown. The app's own open-source notices live behind the last row rather
/// than in the About page, so the legal surface has a single home.
class BibleCreditsPage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final translations = watchValue<TranslationsManager, List<BibleVersionRow>>(
      (m) => m.downloadedTranslations,
    );

    return MScaffold(
      appBar: MAppBar.secondary(
        title: 'Copyright & Credits',
        centerTitle: true,
      ),
      body: ListView(
        children: [
          Spaces.verticalLarge,
          MText(
            'The translations below are distributed under the terms their '
            'publishers set out.',
            style: textTheme.captionRegular,
            color: colors.inActive,
          ),
          Spaces.verticalXLarge,
          if (translations.isEmpty)
            const MEmptyState(title: 'No translations on this device yet')
          else
            for (final translation in translations) ...[
              _CreditCard(translation: translation),
              Spaces.verticalMedium,
            ],
          Spaces.verticalXLarge,
          MSettingsSection(
            children: [
              MSettingsListTile(
                title: 'Open-source licenses',
                leadingIcon: MIcons.file_02,
                showDivider: false,
                onTap: () =>
                    showLicensePage(context: context, applicationName: 'Menō'),
              ),
            ],
          ),
          Spaces.verticalXXLarge,
        ],
      ),
    );
  }
}

/// One translation's attribution.
class _CreditCard extends StatelessWidget {
  const new({required this.translation});

  final BibleVersionRow translation;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final statement = translation.copyrightStatement.trim();

    return Container(
      padding: const .all(Insets.lg),
      decoration: ShapeDecoration(
        color: colors.surfaceTint,
        shape: const RoundedRectangleBorder(borderRadius: Corners.lg),
      ),
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          MText(
            '${translation.code.toUpperCase()} · ${translation.name}',
            style: textTheme.captionMedium,
          ),
          Spaces.verticalSmall,
          MText(
            statement.isEmpty
                ? 'No attribution was supplied with this translation.'
                : statement,
            style: textTheme.microRegular,
            color: colors.onBackgroundVariant,
          ),
        ],
      ),
    );
  }
}
