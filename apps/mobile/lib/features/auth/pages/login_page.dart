import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// Signs an account in with an email address and a password.
///
/// When the app knows who last signed in — after a sign-out, or a session that
/// expired — it greets them and shows only the password field; a returning
/// user is one tap and a password away, and the accounts already on the
/// device are one tap away through the avatar. With nobody to greet, the full
/// form is shown.
///
/// Nothing here navigates on success. A signed-in account flips
/// [AuthManager.isAuthenticated], the router listens, and the guard moves the
/// user to wherever they belong — the same path every other way of becoming
/// signed in takes, including a session restored at startup.
class LoginPage extends WatchingWidget {
  /// Creates the sign-in page.
  ///
  /// [implyLeading] shows a back affordance. It is false when the page was
  /// opened from a tab, where there is nothing to go back to, and true when it
  /// was pushed from the intro carousel.
  const new({super.key, this.implyLeading = false});

  /// Whether the app bar shows a back affordance.
  final bool implyLeading;

  @override
  Widget build(BuildContext context) {
    registerHandler<AuthManager, CommandError<LoginArgs>?>(
      select: (manager) => manager.login.errors,
      handler: (context, error, _) =>
          context.showErrorToast(errorMessage(error?.error)),
    );
    registerHandler<AuthManager, CommandResult<LoginArgs?, UserCredential>>(
      select: (manager) => manager.login.results,
      handler: (context, result, _) {
        if (result.isSuccess) di<OnboardingManager>().completeOnboarding.run();
      },
    );

    final account = watchValue<UserManager, User>(
      (manager) => manager.lastKnownUser,
    );
    final isRunning = watchValue<AuthManager, bool>(
      (manager) => manager.login.isRunning,
    );

    final formKey = createOnce(GlobalKey<FormState>.new);
    final passwordController = createOnce(TextEditingController.new);
    final emailController = createOnce(TextEditingController.new);

    /// Sends the attempt, with the address the screen is showing.
    void submit() {
      if (formKey.currentState?.validate() != true) return;

      di<AuthManager>().login.run(
        LoginArgs(
          email: Email(account.isEmpty ? emailController.text : account.email),
          password: Password.login(passwordController.text),
        ),
      );
    }

    return MScaffold(
      appBar: MAppBar.primary(title: 'Log in', implyLeading: implyLeading),
      body: Form(
        key: formKey,
        autovalidateMode: AutovalidateMode.onUserInteraction,
        child: SingleChildScrollView(
          padding: const EdgeInsets.symmetric(vertical: Insets.xl),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (account.isEmpty)
                MTextFormField(
                  key: const Key('login_email'),
                  label: 'Email Address',
                  hint: 'example@gmail.com',
                  prefixIcon: MIcons.mail,
                  keyboardType: TextInputType.emailAddress,
                  controller: emailController,
                  enabled: !isRunning,
                  validator: (_) => Email(emailController.text).errorMessage,
                )
              else
                UserAccountDetails(
                  user: account,
                  onSwitchAccount: () => SwitchAccountSheet.show(context),
                ),
              Spaces.verticalXLarge,
              MTextFormField(
                key: const Key('login_password'),
                label: 'Be secure',
                hint: 'Enter your password',
                prefixIcon: MIcons.key,
                isPassword: true,
                controller: passwordController,
                enabled: !isRunning,
                textInputAction: TextInputAction.done,
                onFieldSubmitted: (_) => submit(),
                validator: (_) =>
                    Password.login(passwordController.text).errorMessage,
              ),
              Spaces.verticalSmall,
              Align(
                alignment: Alignment.centerRight,
                child: MTextButton(
                  label: 'Forgot password?',
                  onPressed: () => context.push(MRoutes.forgotPassword),
                ),
              ),
              Spaces.verticalXLarge,
              MPrimaryButton(
                key: const Key('login_submit'),
                label: 'Log in',
                loading: isRunning,
                onPressed: submit,
              ),
              Spaces.verticalXXLarge,
              MInlineActionText(
                label: "Don't have an account?",
                actionLabel: 'Create an account',
                onAction: isRunning
                    ? null
                    : () => context.push(MRoutes.registerRoute()),
              ),
            ],
          ),
        ),
      ),
    );
  }

  @override
  void debugFillProperties(DiagnosticPropertiesBuilder properties) {
    super.debugFillProperties(properties);
    properties.add(DiagnosticsProperty<bool>('implyLeading', implyLeading));
  }
}
