import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// Sets a new password with a reset code.
///
/// The last step of the flow and the only one that calls `/auth/reset-password`.
class ResetPasswordPage extends WatchingWidget {
  /// Creates the new-password page.
  ///
  /// [email] and [code] come from the route's query. Both are required by the
  /// API call, so a page opened without them cannot submit.
  const new({super.key, this.email, this.code});

  /// The address the reset code was sent to.
  final String? email;

  /// The reset code the user entered.
  final String? code;

  @override
  Widget build(BuildContext context) {
    registerHandler<AuthManager, void>(
      select: (manager) => manager.resetPassword,
      handler: (ctx, _, _) => ctx.go(MRoutes.resetPasswordSuccess),
    );

    registerHandler<AuthManager, CommandError<ResetPasswordArgs>?>(
      select: (manager) => manager.resetPassword.errors,
      handler: (ctx, e, _) => ctx.showErrorToast(errorMessage(e?.error)),
    );

    final isSaving = watchValue<AuthManager, bool>(
      (manager) => manager.resetPassword.isRunning,
    );

    final formKey = createOnce(GlobalKey<FormState>.new);
    final passwordCtl = createOnce(TextEditingController.new);
    final confirmationCtrl = createOnce(TextEditingController.new);

    final address = email ?? '';
    final resetCode = code ?? '';

    /// Validates both fields and sends the change.
    void submit() {
      if (formKey.currentState?.validate() != true) return;
      if (address.isEmpty || resetCode.isEmpty) return;

      di<AuthManager>().resetPassword.run(
        ResetPasswordArgs(
          email: Email(address),
          code: resetCode,
          newPassword: .register(passwordCtl.text),
        ),
      );
    }

    return MScaffold(
      appBar: MAppBar.primary(title: 'New Password'),
      body: Form(
        key: formKey,
        autovalidateMode: .onUserInteraction,
        child: SingleChildScrollView(
          padding: const .symmetric(vertical: Insets.xl),
          child: Column(
            crossAxisAlignment: .stretch,
            children: [
              if (address.isEmpty || resetCode.isEmpty) ...[
                MText(
                  'This link has expired. Ask for a new reset code to '
                  'continue.',
                  style: MTextTheme.of(context).bodyRegular,
                  maxLines: 3,
                ),
                Spaces.verticalXLarge,
              ],
              MTextFormField(
                key: const Key('reset_password'),
                label: 'New password',
                hint: 'At least 8 characters',
                prefixIcon: MIcons.key,
                isPassword: true,
                enabled: !isSaving,
                controller: passwordCtl,
                validator: (_) => Password.register(passwordCtl.text).errorMsg,
              ),
              Spaces.verticalXLarge,
              MTextFormField(
                key: const Key('reset_password_confirm'),
                label: 'Confirm password',
                hint: 'Type it again',
                prefixIcon: MIcons.shield,
                isPassword: true,
                enabled: !isSaving,
                controller: confirmationCtrl,
                validator: (_) => confirmationCtrl.text == passwordCtl.text
                    ? null
                    : 'The two passwords do not match',
              ),
              Spaces.verticalSmall,
              MPasswordRuleTracker(
                strength: Password.register(passwordCtl.text).strength,
                touched: passwordCtl.text.isNotEmpty,
                rules: [
                  for (final rule in _passwordRules(
                    Password.register(passwordCtl.text),
                  ))
                    rule,
                ],
              ),
              Spaces.verticalXLarge,
              MPrimaryButton(
                key: const Key('reset_password_submit'),
                label: 'Reset password',
                loading: isSaving,
                disabled: isSaving || address.isEmpty || resetCode.isEmpty,
                onPressed: submit,
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// The policy chips for [password].
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
