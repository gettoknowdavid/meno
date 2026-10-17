import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

class AboutPage extends StatelessWidget {
  const new({super.key});

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
            const SizedBox(height: 16),
            MSettingsSection(
              children: [
                MSettingsListTile(
                  title: 'Version',
                  trailing: MText(
                    'Beta',
                    style: textTheme.captionMedium,
                    color: trailingColor,
                  ),
                  onTap: () {},
                ),
                MSettingsListTile(title: 'Report an app problem', onTap: () {}),
                MSettingsListTile(
                  title: 'Privacy Policy',
                  onTap: () {},
                  trailing: Icon(
                    MIcons.arrow_narrow_up_right,
                    size: 20,
                    color: trailingColor,
                  ),
                ),
                MSettingsListTile(
                  title: 'Terms & Conditions',
                  showDivider: false,
                  onTap: () {},
                  trailing: Icon(
                    MIcons.arrow_narrow_up_right,
                    size: 20,
                    color: trailingColor,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
