import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/_shared.dart';
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
      handler: (context, _, _) => context.go(MRoutes.resetPasswordSuccess),
    );
    registerHandler<AuthManager, CommandError<ResetPasswordArgs>?>(
      select: (manager) => manager.resetPassword.errors,
      handler: (context, error, _) =>
          context.showErrorToast(errorMessage(error?.error)),
    );

    final isSaving = watchValue<AuthManager, bool>(
      (manager) => manager.resetPassword.isRunning,
    );

    final formKey = createOnce(GlobalKey<FormState>.new);
    final passwordController = createOnce(TextEditingController.new);
    final confirmationController = createOnce(TextEditingController.new);

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
          newPassword: Password.register(passwordController.text),
        ),
      );
    }

    return MScaffold(
      appBar: MAppBar.primary(title: 'New Password'),
      body: Form(
        key: formKey,
        autovalidateMode: AutovalidateMode.onUserInteraction,
        child: SingleChildScrollView(
          padding: const EdgeInsets.symmetric(vertical: Insets.xl),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
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
                controller: passwordController,
                validator: (_) =>
                    Password.register(passwordController.text).errorMessage,
              ),
              Spaces.verticalXLarge,
              MTextFormField(
                key: const Key('reset_password_confirm'),
                label: 'Confirm password',
                hint: 'Type it again',
                prefixIcon: MIcons.shield,
                isPassword: true,
                enabled: !isSaving,
                controller: confirmationController,
                validator: (_) => confirmationController.text ==
                        passwordController.text
                    ? null
                    : 'The two passwords do not match',
              ),
              Spaces.verticalSmall,
              MPasswordRuleTracker(
                strength: Password.register(passwordController.text).strength,
                touched: passwordController.text.isNotEmpty,
                rules: [
                  for (final rule in _passwordRules(
                    Password.register(passwordController.text),
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
  }    /// The policy chips for [password].
  static List<MPasswordRule> _passwordRules(Password password) => [
    MPasswordRule(
      title: '8+',
      caption: 'Characters',
      isMet:
          !password.breaks(ValueRule.tooShort) &&
          !password.breaks(ValueRule.tooLong),
    ),
    MPasswordRule(
      title: 'a-z',
      caption: 'Lowercase',
      isMet: !password.breaks(ValueRule.missingLowercase),
    ),
    MPasswordRule(
      title: 'A-Z',
      caption: 'Uppercase',
      isMet: !password.breaks(ValueRule.missingUppercase),
    ),
  ];

  @override
  void debugFillProperties(DiagnosticPropertiesBuilder properties) {
    super.debugFillProperties(properties);
    properties
      ..add(StringProperty('email', email))
      ..add(StringProperty('code', code));
  }
}
