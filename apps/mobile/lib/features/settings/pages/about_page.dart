import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:package_info_plus/package_info_plus.dart';

/// What the build is, and where its legal copy and support address live.
class AboutPage extends StatefulWidget {
  /// Creates the About page.
  const new({super.key});

  @override
  State<AboutPage> createState() => _AboutPageState();
}

class _AboutPageState extends State<AboutPage> {
  /// Read once: the platform channel answers the packaged build's version, and
  /// nothing is gained by asking again whenever this page rebuilds.
  late final Future<String> _version = PackageInfo.fromPlatform().then(
    (info) => '${info.version} (${info.buildNumber})',
  );

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final trailingColor = colors.onBackgroundVariant;
    return MScaffold(
      appBar: MAppBar.secondary(title: 'About Menō', centerTitle: true),
      body: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const SizedBox(height: Insets.lg),
            MSettingsSection(
              children: [
                MSettingsListTile(
                  title: 'Version',
                  trailing: FutureBuilder<String>(
                    future: _version,
                    builder: (_, snapshot) => MText(
                      snapshot.data ?? '',
                      style: textTheme.captionMedium,
                      color: trailingColor,
                    ),
                  ),
                ),
                MSettingsListTile(
                  title: 'Report an app problem',
                  onTap: () => _reportProblem(context),
                ),
                MSettingsListTile(
                  title: 'Privacy Policy',
                  onTap: () => context.push(MRoutes.privacyPolicy),
                ),
                MSettingsListTile(
                  title: 'Terms & Conditions',
                  showDivider: false,
                  onTap: () => context.push(MRoutes.termsOfUse),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  /// The support details, next to what a useful report contains.
  ///
  /// There is no support desk behind the app yet, so the address and the phone
  /// number are the way through — and both are selectable, so they can be
  /// copied out of the sheet.
  Future<void> _reportProblem(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (_) => SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(
            Insets.lg,
            0,
            Insets.lg,
            Insets.lg,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              MText('Report an app problem', style: textTheme.subheadingBold),
              Spaces.verticalSmall,
              MText(
                'Tell us what happened, what you expected, and which device '
                'you are on. A screenshot helps us reproduce it.',
                style: textTheme.captionRegular,
                color: colors.onBackgroundVariant,
              ),
              Spaces.verticalLarge,
              MText('Email', style: textTheme.captionMedium),
              SelectableText(legalSupportEmail, style: textTheme.bodyRegular),
              Spaces.verticalMedium,
              MText('Phone', style: textTheme.captionMedium),
              SelectableText(legalSupportPhone, style: textTheme.bodyRegular),
            ],
          ),
        ),
      ),
    );
  }
}
