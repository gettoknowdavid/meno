import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/features/settings/settings.dart';
import 'package:meno_ui/meno_ui.dart';

class SettingsPage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    final manager = di<SettingsManager>();
    final settings = watchValue<SettingsManager, UserSettings>(
      (manager) => manager.settings,
    );
    final profile = watchValue<MyProfileManager, MyProfile>(
      (manager) => manager.profile,
    );

    return MScaffold(
      appBar: MAppBar.primary(title: 'Settings'),
      body: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Spaces.verticalLarge,
            MSettingsSection(
              title: 'General',
              children: [
                MSettingsListTile(
                  title: 'Language',
                  leadingIcon: MIcons.translate_02,
                  onTap: () {},
                  trailing: MTextButton.icon(
                    label: 'English',
                    icon: Icon(
                      MIcons.chevron_right,
                      size: 18,
                      color: colors.onBackgroundVariant,
                    ),
                    iconPlacement: MButtonIconPlacement.right,
                    onPressed: () {},
                    style: TextButton.styleFrom(
                      padding: EdgeInsets.zero,
                      foregroundColor: colors.onBackgroundVariant,
                    ),
                  ),
                ),
                MSettingsListTile(
                  title: 'Notifications',
                  leadingIcon: MIcons.bell,
                  onTap: () => context.push(MRoutes.notificationSettings),
                ),
                MSettingsListTile(
                  title: 'Dark Mode',
                  leadingIcon: MIcons.moon_01,
                  showDivider: false,
                  trailing: Switch(
                    value: settings.display == .dark,
                    onChanged: manager.toggleDarkMode.run,
                  ),
                ),
              ],
            ),
            Spaces.verticalXXLarge,
            MSettingsSection(
              title: 'Account & Security',
              children: [
                MSettingsListTile(
                  title: 'Subscriptions',
                  leadingIcon: MIcons.layers_three_01,
                  onTap: () {},
                  isDisabled: true,
                ),
                MSettingsListTile(
                  title: 'Security',
                  leadingIcon: MIcons.shield,
                  onTap: () => context.push(MRoutes.securitySettings),
                  showDivider: false,
                ),
              ],
            ),
            Spaces.verticalXXLarge,
            MSettingsSection(
              title: 'Other',
              children: [
                MSettingsListTile(
                  title: 'About Menō',
                  leadingIcon: MIcons.users,
                  onTap: () => context.push(MRoutes.about),
                ),
                MSettingsListTile(
                  title: 'FAQs',
                  leadingIcon: MIcons.help_circle,
                  onTap: () {},
                ),
                MSettingsListTile(
                  title: 'Share with friends & family',
                  leadingIcon: MIcons.share,
                  onTap: () {},
                ),
                MSettingsListTile(
                  title: 'Logout',
                  leadingIcon: MIcons.log_out,
                  onTap: () => _onLogout(context, profile),
                ),
                MSettingsListTile(
                  title: 'Delete Account',
                  leadingIcon: MIcons.trash,
                  onTap: () {},
                  titleColor: colors.error,
                  iconColor: colors.error,
                  showDivider: false,
                ),
              ],
            ),
            Spaces.verticalXXLarge,
          ],
        ),
      ),
    );
  }

  Future<void> _onLogout(BuildContext context, MyProfile profile) async {
    final fullName = profile.isEmpty ? 'this account' : profile.fullName;
    final confirmed = await MConfirmationDialog.show(
      context,
      title: 'Log out from account?',
      description: 'You are about to log out from $fullName',
    );
    if (confirmed) di<AuthManager>().logout.run();
  }
}
