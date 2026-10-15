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
  const new({super.key, this.implyLeading = false});

  /// Whether the app bar shows a back affordance.
  final bool implyLeading;

  @override
  Widget build(BuildContext context) {
    registerHandler<AuthManager, CommandError<LoginArgs>?>(
      select: (manager) => manager.login.errors,
      handler: (ctx, e, _) => ctx.showErrorToast(errorMessage(e?.error)),
    );

    registerHandler<AuthManager, CommandResult<LoginArgs?, UserCredential>>(
      select: (manager) => manager.login.results,
      handler: (ctx, result, _) {
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
    final emailCtrl = createOnce(TextEditingController.new);
    final passwordCtrl = createOnce(TextEditingController.new);

    /// Sends the attempt, with the address the screen is showing.
    void submit() {
      if (formKey.currentState?.validate() != true) return;

      di<AuthManager>().login.run(
        LoginArgs(
          email: account.isEmpty ? Email(emailCtrl.text) : account.email,
          password: Password.login(passwordCtrl.text),
        ),
      );
    }

    return MScaffold(
      appBar: MAppBar.primary(title: 'Log in', implyLeading: implyLeading),
      body: Form(
        key: formKey,
        autovalidateMode: .onUserInteraction,
        child: SingleChildScrollView(
          padding: const .symmetric(vertical: Insets.xl),
          child: Column(
            crossAxisAlignment: .stretch,
            children: [
              if (account.isEmpty)
                MTextFormField(
                  key: const Key('login_email'),
                  label: 'Email Address',
                  hint: 'example@gmail.com',
                  prefixIcon: MIcons.mail,
                  keyboardType: .emailAddress,
                  controller: emailCtrl,
                  enabled: !isRunning,
                  validator: (_) => Email(emailCtrl.text).errorMsg,
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
                controller: passwordCtrl,
                enabled: !isRunning,
                textInputAction: .done,
                onFieldSubmitted: (_) => submit(),
                validator: (_) => Password.login(passwordCtrl.text).errorMsg,
              ),
              Spaces.verticalSmall,
              Align(
                alignment: .centerRight,
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
                enabled: !isRunning,
                onAction: () => context.push(MRoutes.registerRoute()),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
