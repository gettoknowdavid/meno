import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
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

  /// Shows the sheet and applies what was chosen.
  ///
  /// Reads the store's current contents once, when the sheet opens: the list
  /// is a snapshot of what can be switched to, and it cannot change while the
  /// sheet is up.
  static Future<void> show(BuildContext context) async {
    final auth = di<AuthManager>();
    final accounts = auth.accounts.value.values.toList();
    final activeUserId = auth.activeCredential.value.userId;

    final chosen = await showModalBottomSheet<String>(
      context: context,
      showDragHandle: true,
      useSafeArea: true,
      builder: (sheetContext) {
        final textTheme = MTextTheme.of(sheetContext);
        final colors = MColorScheme.of(sheetContext);

        return Column(
          mainAxisSize: .min,
          children: [
            MText('Accounts', style: textTheme.heading3Medium),
            Spaces.verticalSmall,
            for (final credential in accounts)
              ListTile(
                title: MText(
                  credential.user.fullName,
                  maxLines: 1,
                  overflow: .ellipsis,
                ),
                subtitle: MText(
                  credential.user.email,
                  style: textTheme.captionMedium,
                  maxLines: 1,
                  overflow: .ellipsis,
                ),
                trailing: credential.userId == activeUserId
                    ? Icon(MIcons.check, color: colors.primary)
                    : null,
                onTap: () => Navigator.pop(sheetContext, credential.userId),
              ),
            ListTile(
              title: MText('Add another account', color: colors.primary),
              onTap: () => Navigator.pop(sheetContext, addAccountValue),
            ),
            Spaces.verticalMedium,
          ],
        );
      },
    );

    if (chosen == null) return;
    if (chosen == addAccountValue) {
      auth.addAccount.run();
      return;
    }
    auth.switchAccount.run(chosen);
  }
}
