import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// The accounts signed in on this device, as a list to choose from.
///
/// Switching is a copy from the vault into the active keys
/// ([AuthManager.switchAccount]) rather than a sign-in: every account in the
/// list still holds live tokens, so choosing one is instant and needs no
/// password. That is the whole point of keeping the vault, and it is what
/// makes the several-accounts behaviour of messengers work here too.
final class SwitchAccountSheet {
  /// Prevents instantiation: this type is a namespace for [show].
  const new _();

  /// What the "add another account" row returns.
  ///
  /// A sentinel rather than a user id, because adding an account is not a
  /// switch — it signs the active account out locally and leaves the rest in
  /// place.
  static const String addAccountValue = '__add_account__';

  /// What the "logout" row returns.
  ///
  /// A sentinel for the same reason as [addAccountValue]: logging out is not a
  /// switch, and it needs a confirmation the sheet cannot show while it is up.
  static const String logoutValue = '__logout__';

  /// What the "log in to an existing account" button returns.
  ///
  /// Signed in with one account, there is nothing to switch *to*: this button
  /// means "be someone else", which is a deactivate followed by the sign-in
  /// screen, not a switch.
  static const String loginExistingAccountValue = '__login_existing__';

  /// What the "create new account" button returns.
  static const String createAccountValue = '__create_account__';

  /// Shows the sheet and applies what was chosen.
  ///
  /// Reads the store's current contents once, when the sheet opens: the list
  /// is a snapshot of what can be switched to, and it cannot change while the
  /// sheet is up.
  static Future<void> show(BuildContext context) async {
    final auth = di<AuthManager>();
    final accounts = auth.accounts.value.values.toList();
    final activeUserId = auth.activeCredential.value.userId;
    final activeName = auth.activeCredential.value.user.fullName.orEmpty;
    // One account has nothing to choose between: the sheet then offers the
    // ways off this account instead of a list of one.
    final hasSeveralAccounts = accounts.length > 1;

    final chosen = await showModalBottomSheet<Object>(
      context: context,
      showDragHandle: true,
      useSafeArea: true,
      useRootNavigator: true,
      builder: (sheetContext) {
        final textTheme = MTextTheme.of(sheetContext);
        final colors = MColorScheme.of(sheetContext);

        return Column(
          mainAxisSize: .min,
          children: [
            Padding(
              padding: const .symmetric(horizontal: 16),
              child: Row(
                mainAxisAlignment: .spaceBetween,
                children: [
                  MText('Switch Account', style: textTheme.subheadingMedium),
                  MIconButton(
                    icon: const Icon(MIcons.x_close),
                    onPressed: () => context.pop(),
                    size: Insets.lg,
                    color: colors.outlineVariant1,
                  ),
                ],
              ),
            ),
            const Padding(
              padding: .symmetric(horizontal: 16),
              child: MDivider(topSpace: Insets.sm, bottomSpace: Insets.lg),
            ),
            if (hasSeveralAccounts) ...[
              for (final credential in accounts)
                RadioGroup(
                  key: ValueKey(credential.user.id),
                  groupValue: activeUserId,
                  onChanged: (v) => sheetContext.pop(v),
                  child: RadioListTile(
                    value: credential.user.id,
                    controlAffinity: .trailing,
                    contentPadding: const .fromLTRB(16, 0, 14, 0),
                    title: Row(
                      children: [
                        MAvatar(radius: 20, url: credential.user.image?.getUrl),
                        Spaces.horizontalLarge,
                        MText(
                          credential.user.fullName.getOrCrash,
                          style: textTheme.bodyRegular,
                        ),
                      ],
                    ),
                  ),
                ),
              // ListTile(
              //   title: MText(
              //     credential.user.fullName.orEmpty,
              //     maxLines: 1,
              //     overflow: .ellipsis,
              //   ),
              //   subtitle: MText(
              //     credential.user.email.orEmpty,
              //     style: textTheme.captionMedium,
              //     maxLines: 1,
              //     overflow: .ellipsis,
              //   ),
              //   trailing: credential.userId == activeUserId
              //       ? Icon(MIcons.check, color: colors.primary)
              //       : null,
              //   onTap: () => Navigator.pop(sheetContext, credential.userId),
              // ),
              // ListTile(
              //   title: MText('Use another account', color: colors.primary),
              //   onTap: () => Navigator.pop(sheetContext, addAccountValue),
              // ),
              Spaces.verticalSmall,
              MModalListTile(
                leading: const Icon(MIcons.plus_circle),
                title: 'Add account',
                titleColor: colors.primary,
                contentPadding: const .fromLTRB(16, 0, 16, 0),
                onTap: () => sheetContext.pop(addAccountValue),
              ),
              Spaces.verticalSmall,
              MModalListTile(
                leading: const Icon(MIcons.log_out),
                title: 'Logout',
                titleColor: colors.error,
                onTap: () => sheetContext.pop(logoutValue),
                contentPadding: const .fromLTRB(16, 0, 16, 0),
              ),
              Spaces.verticalMedium,
            ] else ...[
              Padding(
                padding: const .symmetric(horizontal: 16),
                child: SizedBox(
                  width: double.infinity,
                  child: MPrimaryButton(
                    label: 'Log in to existing account',
                    onPressed: () =>
                        sheetContext.pop(loginExistingAccountValue),
                  ),
                ),
              ),
              Spaces.verticalSmall,
              MTextButton(
                label: 'Create new account',
                onPressed: () => sheetContext.pop(createAccountValue),
              ),
              Spaces.verticalMedium,
            ],
          ],
        );
      },
    );

    if (chosen == null) return;
    if (chosen == logoutValue) {
      if (!context.mounted) return;
      final confirmed = await MConfirmationDialog.show(
        context,
        title: 'Log out from account?',
        description: 'You are about to log out from '
            '${activeName.isEmpty ? 'this account' : activeName}',
      );
      if (confirmed) auth.logout.run();
      return;
    }
    if (chosen == loginExistingAccountValue || chosen == createAccountValue) {
      final destination = chosen == createAccountValue
          ? MRoutes.registerRoute()
          : MRoutes.login;
      // Deactivate before navigating: the guard sends every public route home
      // while an account is still active, so the sign-in screens could not be
      // shown. The vault keeps the account, so it greets the user and can be
      // switched back to afterwards.
      await auth.addAccount.runAsync();
      if (context.mounted) context.go(destination);
      return;
    }
    if (chosen == addAccountValue) {
      auth.addAccount.run();
      return;
    }
    if (chosen is Id) auth.switchAccount.run(chosen);
  }
}
