import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_ui/meno_ui.dart';

/// Changes the account's password.
///
/// The API checks the current password before writing anything, then revokes
/// every session the account has — this one included. So the screen ends by
/// signing the device out, and the router takes the person to sign-in, where
/// the new password is the one that works.
class SecuritySettingsPage extends WatchingWidget {
  /// Creates the security settings page.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    registerHandler<AuthManager, CommandError<ChangePasswordArgs>?>(
      select: (manager) => manager.changePassword.errors,
      handler: (ctx, error, _) =>
          ctx.showErrorToast(errorMessage(error?.error)),
    );

    final isSaving = watchValue<AuthManager, bool>(
      (manager) => manager.changePassword.isRunning,
    );

    final formKey = createOnce(GlobalKey<FormState>.new);
    final currentCtl = createOnce(TextEditingController.new);
    final passwordCtl = createOnce(TextEditingController.new);
    final confirmationCtrl = createOnce(TextEditingController.new);

    final newPassword = Password.register(passwordCtl.text);

    void submit() {
      if (formKey.currentState?.validate() != true) return;

      di<AuthManager>().changePassword.run(
        ChangePasswordArgs(
          currentPassword: Password.register(currentCtl.text),
          newPassword: newPassword,
        ),
      );
    }

    return MScaffold(
      appBar: MAppBar.secondary(title: 'Security', centerTitle: true),
      body: Form(
        key: formKey,
        autovalidateMode: .onUserInteraction,
        child: SingleChildScrollView(
          padding: const .symmetric(horizontal: Insets.lg),
          child: Column(
            crossAxisAlignment: .stretch,
            children: [
              Spaces.verticalLarge,
              MText(
                'Change/update password',
                style: textTheme.captionMedium,
                color: colors.inActive,
              ),
              Spaces.verticalXLarge,
              MTextFormField(
                key: const Key('security_current_password'),
                label: 'Current password',
                prefixIcon: MIcons.key,
                isPassword: true,
                enabled: !isSaving,
                controller: currentCtl,
                validator: (_) => currentCtl.text.isEmpty
                    ? 'Enter your current password'
                    : null,
              ),
              Spaces.verticalXLarge,
              MTextFormField(
                key: const Key('security_new_password'),
                label: 'New password',
                hint: 'At least 8 characters',
                prefixIcon: MIcons.key,
                isPassword: true,
                enabled: !isSaving,
                controller: passwordCtl,
                validator: (_) => newPassword.errorMsg,
              ),
              Spaces.verticalXLarge,
              MTextFormField(
                key: const Key('security_confirm_password'),
                label: 'Confirm new password',
                hint: 'Type it again',
                isPassword: true,
                enabled: !isSaving,
                controller: confirmationCtrl,
                validator: (_) => confirmationCtrl.text == passwordCtl.text
                    ? null
                    : 'The two passwords do not match',
              ),
              Spaces.verticalSmall,
              MPasswordRuleTracker(
                strength: newPassword.strength,
                touched: passwordCtl.text.isNotEmpty,
                rules: [
                  for (final rule in _passwordRules(newPassword)) rule,
                ],
              ),
              Spaces.verticalXLarge,
              MPrimaryButton(
                key: const Key('security_submit'),
                label: 'Save changes',
                loading: isSaving,
                disabled: isSaving,
                onPressed: submit,
              ),
              Spaces.verticalXXLarge,
            ],
          ),
        ),
      ),
    );
  }

  /// The policy chips for [password].
  ///
  /// The same five the reset screen shows, because it is the same server rule.
  static List<MPasswordRule> _passwordRules(Password password) => [
    MPasswordRule(
      title: '8+',
      caption: 'Characters',
      isMet: !password.isRuleBroken(const PasswordTooShort()),
    ),
    MPasswordRule(
      title: 'A-Z',
      caption: 'Uppercase',
      isMet: !password.isRuleBroken(const PasswordMissingUppercase()),
    ),
    MPasswordRule(
      title: 'a-z',
      caption: 'Lowercase',
      isMet: !password.isRuleBroken(const PasswordMissingLowercase()),
    ),
    MPasswordRule(
      title: '%',
      caption: 'Symbol',
      isMet: !password.isRuleBroken(const PasswordMissingSpecialCharacter()),
    ),
    MPasswordRule(
      title: '123',
      caption: 'Number',
      isMet: !password.isRuleBroken(const PasswordMissingNumber()),
    ),
  ];
}
