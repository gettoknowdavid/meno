import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/manager/register_form_manager.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// Creates an account.
///
/// The form lives in a get_it scope pushed for this page, so its fields are
/// created when the page is built and destroyed with it: two visits cannot share
/// a half-typed password, and there is nothing to remember to clear.
///
/// The password rules the tracker shows are the API's own
/// (`password_problems` in `apps/api/src/modules/auth/validators.rs`), so the
/// form never asks for something — a digit, a symbol — that the server does not
/// require.
class RegisterPage extends WatchingWidget {
  /// Creates the registration page.
  ///
  /// [implyLeading] shows a back affordance, for when the page was pushed from
  /// the sign-in screen or the intro carousel.
  const new({super.key, this.implyLeading = true});

  /// Whether the app bar shows a back affordance.
  final bool implyLeading;

  @override
  Widget build(BuildContext context) {
    pushScope(
      init: (getIt) =>
          getIt.registerSingleton<RegisterFormManager>(RegisterFormManager()),
    );

    registerHandler(
      select: (AuthManager manager) => manager.register.errors,
      handler: (context, error, _) =>
          context.showErrorToast(errorMessage(error?.error)),
    );
    // Creating the account is what leaves the intro behind for good; the
    // intro's own buttons only open this screen.
    registerHandler(
      select: (AuthManager manager) => manager.register.results,
      handler: (context, result, _) {
        if (result.isSuccess) di<OnboardingManager>().completeOnboarding.run();
      },
    );

    final form = watchValue((RegisterFormManager manager) => manager.form);
    final isRunning = watchValue((AuthManager manager) => manager.register.isRunning);

    final rules = [
      MPasswordRule(
        title: '8+',
        caption: 'Characters',
        isMet: !form.password.breaks(ValueRule.tooShort) &&
            !form.password.breaks(ValueRule.tooLong),
      ),
      MPasswordRule(
        title: 'a-z',
        caption: 'Lowercase',
        isMet: !form.password.breaks(ValueRule.missingLowercase),
      ),
      MPasswordRule(
        title: 'A-Z',
        caption: 'Uppercase',
        isMet: !form.password.breaks(ValueRule.missingUppercase),
      ),
    ];

    /// Sends the registration, then empties the form.
    void submit() {
      if (!form.isValid) return;

      di<AuthManager>().register.run(
        RegisterArgs(
          fullName: form.fullName,
          email: form.email,
          password: form.password,
        ),
      );
      di<RegisterFormManager>().reset();
    }

    return MScaffold(
      appBar: MAppBar.primary(
        title: 'New Account',
        implyLeading: implyLeading,
      ),
      body: Form(
        autovalidateMode: AutovalidateMode.onUserInteraction,
        child: SingleChildScrollView(
          padding: const EdgeInsets.symmetric(vertical: Insets.xl),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              MTextFormField(
                key: const Key('register_full_name'),
                label: 'Full Name',
                hint: 'Ada Lovelace',
                prefixIcon: MIcons.user,
                enabled: !isRunning,
                onChanged: di<RegisterFormManager>().onFullNameChanged,
                validator: (_) => form.fullName.errorMessage,
              ),
              Spaces.verticalXLarge,
              MTextFormField(
                key: const Key('register_email'),
                label: 'Email Address',
                hint: 'example@gmail.com',
                prefixIcon: MIcons.mail,
                keyboardType: TextInputType.emailAddress,
                enabled: !isRunning,
                onChanged: di<RegisterFormManager>().onEmailChanged,
                validator: (_) => form.email.errorMessage,
              ),
              Spaces.verticalXLarge,
              MTextFormField(
                key: const Key('register_password'),
                label: 'Be secure',
                hint: 'At least 8 characters',
                prefixIcon: MIcons.key,
                isPassword: true,
                enabled: !isRunning,
                onChanged: di<RegisterFormManager>().onPasswordChanged,
                validator: (_) => form.password.errorMessage,
              ),
              Spaces.verticalSmall,
              MPasswordRuleTracker(
                strength: form.password.strength,
                touched: form.password.input.isNotEmpty,
                rules: rules,
              ),
              Spaces.verticalXLarge,
              MPrimaryButton(
                key: const Key('register_submit'),
                label: 'Create your account',
                loading: isRunning,
                disabled: isRunning || !form.isValid,
                onPressed: submit,
              ),
              Spaces.verticalXXLarge,
              MInlineActionText(
                label: 'Already have an account?',
                actionLabel: 'Log in',
                onAction: isRunning
                    ? null
                    : () => context.replace(MRoutes.loginRoute()),
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
