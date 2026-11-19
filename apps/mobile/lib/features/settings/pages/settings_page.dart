import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/bible/bible.dart';
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

    final isDeleting = watchValue<AuthManager, bool>(
      (manager) => manager.deleteAccount.isRunning,
    );

    // Device data, not the account's: the count is how many translations the
    // reader has to choose from without starting a broadcast.
    final installedTranslations =
        watchValue<TranslationsManager, List<BibleVersionRow>>(
          (manager) => manager.downloadedTranslations,
        );

    // The account subtree is torn down once the vault forgets the account, so
    // the router — which outlives it — is what takes the user to the intro
    // rather than a page that is already disappearing.
    registerHandler<AuthManager, void>(
      select: (manager) => manager.deleteAccount,
      handler: (context, _, _) => _showOnboardingAfterDelete(),
    );

    registerHandler<AuthManager, CommandError<void>?>(
      select: (manager) => manager.deleteAccount.errors,
      handler: (context, error, _) =>
          context.showErrorToast(errorMessage(error?.error)),
    );

    return MScaffold(
      appBar: MAppBar.primary(title: 'Settings'),
      body: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: .stretch,
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
                    onPressed: null,
                    style: TextButton.styleFrom(
                      padding: .zero,
                      disabledBackgroundColor: Colors.transparent,
                      foregroundColor: colors.onBackgroundVariant,
                      disabledForegroundColor: colors.onBackgroundVariant,
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
              title: 'Bible',
              children: [
                MSettingsListTile(
                  title: 'Translations',
                  leadingIcon: MIcons.translate_02,
                  onTap: () => context.push(MRoutes.bibleTranslations),
                  trailing: MTextButton.icon(
                    label: '${installedTranslations.length} installed',
                    icon: Icon(
                      MIcons.chevron_right,
                      size: 18,
                      color: colors.onBackgroundVariant,
                    ),
                    iconPlacement: MButtonIconPlacement.right,
                    onPressed: null,
                    style: TextButton.styleFrom(
                      padding: .zero,
                      disabledBackgroundColor: Colors.transparent,
                      foregroundColor: colors.onBackgroundVariant,
                      disabledForegroundColor: colors.onBackgroundVariant,
                    ),
                  ),
                ),
                MSettingsListTile(
                  title: 'Copyright & Credits',
                  leadingIcon: MIcons.file_02,
                  showDivider: false,
                  onTap: () => context.push(MRoutes.bibleCredits),
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
                  onTap: () => context.push(MRoutes.faqs),
                ),
                // Sharing is commented out until a share link resolves
                // (docs/launch-readiness.md §2.2).
                // MSettingsListTile(
                //   title: 'Share with friends & family',
                //   leadingIcon: MIcons.share,
                //   onTap: () {},
                // ),
                MSettingsListTile(
                  title: 'Logout',
                  leadingIcon: MIcons.log_out,
                  onTap: () => _onLogout(context, profile),
                ),
                MSettingsListTile(
                  title: 'Delete Account',
                  leadingIcon: MIcons.trash,
                  isDisabled: isDeleting,
                  onTap: isDeleting
                      ? null
                      : () => _onDeleteAccount(context, profile),
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
    final fullName = profile.isEmpty
        ? 'this account'
        : profile.fullName.orEmpty;
    final confirmed = await MConfirmationDialog.show(
      context,
      title: 'Log out from account?',
      description: 'You are about to log out from $fullName',
    );
    if (confirmed) di<AuthManager>().logout.run();
  }

  /// Waits for the deleted account's scope to come down, then shows the intro.
  ///
  /// Going to onboarding while the account is still `ready` would be undone by
  /// the router guard, which sends every public route home in that phase. Once
  /// the transition publishes `signedOut`, onboarding is a valid destination
  /// and stays there.
  Future<void> _showOnboardingAfterDelete() async {
    await di<AccountTransitionOwner>().ready;
    di<GoRouter>().go(MRoutes.onboarding);
  }

  Future<void> _onDeleteAccount(BuildContext context, MyProfile profile) async {
    final fullName = profile.isEmpty
        ? 'this account'
        : profile.fullName.orEmpty;
    final confirmed = await MConfirmationDialog.show(
      context,
      title: 'Delete account?',
      description:
          'This permanently deletes $fullName, revokes every session, and '
          'removes this account from the device. This cannot be undone.',
      confirmLabel: 'Delete',
      variant: MConfirmationDialogVariant.danger,
    );
    if (confirmed) di<AuthManager>().deleteAccount.run();
  }
}
