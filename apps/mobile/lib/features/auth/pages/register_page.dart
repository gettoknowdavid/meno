import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// Creates an account.
///
/// The form lives in a get_it scope pushed for this page, so its fields are
/// created when the page is built and destroyed with it: two visits cannot
/// share a half-typed password, and there is nothing to remember to clear.
///
/// The password rules the tracker shows are the API's own
/// (`password_problems` in `apps/api/src/modules/auth/validators.rs`).
class RegisterPage extends WatchingWidget {
  const new({super.key, this.implyLeading = true});

  /// Whether the app bar shows a back affordance.
  final bool implyLeading;

  @override
  Widget build(BuildContext context) {
    pushScope(
      init: (getIt) {
        getIt.registerSingleton<RegisterFormManager>(RegisterFormManager());
      },
    );

    registerHandler<AuthManager, CommandError<RegisterArgs>?>(
      select: (manager) => manager.register.errors,
      handler: (ctx, e, _) => ctx.showErrorToast(errorMessage(e?.error)),
    );

    registerHandler<AuthManager, CommandResult<RegisterArgs?, UserCredential>>(
      select: (manager) => manager.register.results,
      handler: (ctx, result, _) {
        if (!result.isSuccess) return;

        // Only a registration that went through empties the form. Clearing it
        // as the request is sent would throw away a valid form on a refused
        // one — an already-registered address, say — leaving every field
        // showing text the manager no longer holds, and a submit button that
        // stays disabled until each field is edited again.
        di<RegisterFormManager>().reset();
        di<OnboardingManager>().completeOnboarding.run();
      },
    );

    final isOnboarded = watchValue<OnboardingManager, bool>(
      (manager) => manager.isOnboarded,
    );

    final isRunning = watchValue<AuthManager, bool>(
      (manager) => manager.register.isRunning,
    );

    return MScaffold(
      appBar: MAppBar.primary(
        title: 'New Account',
        backText: !isOnboarded ? 'Onboarding' : 'Log in',
        implyLeading: implyLeading,
      ),
      body: SingleChildScrollView(
        padding: const .symmetric(vertical: Insets.xl),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const _RegisterForm(),
            Spaces.verticalXXLarge,
            const GoogleDivider(title: 'Or'),
            Spaces.verticalXLarge,
            const MGoogleButton(title: 'Create with Google'),
            const SizedBox(height: 44),
            MInlineActionText(
              label: 'Already have an account?',
              actionLabel: 'Log in',
              enabled: !isRunning,
              onAction: () => onRedirectPressed(context, isOnboarded),
            ),
          ],
        ),
      ),
    );
  }

  void onRedirectPressed(BuildContext context, bool isOnboarded) {
    if (isOnboarded) {
      context.replace(MRoutes.login);
    } else {
      context.replace(MRoutes.loginRoute(implyLeading: true));
    }
  }
}

class _RegisterForm extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final formKey = createOnce(GlobalKey<FormState>.new);

    return Form(
      key: formKey,
      child: const Column(
        crossAxisAlignment: .stretch,
        children: [
          _FullNameField(),
          Spaces.verticalXLarge,
          _EmailField(),
          Spaces.verticalXLarge,
          _PasswordField(),
          Spaces.verticalXLarge,
          _TermsAndConditionsCheckBox(),
          Spaces.verticalXXLarge,
          _RegisterButton(),
        ],
      ),
    );
  }
}

class _FullNameField extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final focusNode = createOnce(FocusNode.new);

    final fullName = watchValue<RegisterFormManager, FullName>(
      (manager) => manager.fullName,
    );

    final isLoading = watchValue<AuthManager, bool>(
      (manager) => manager.register.isRunning,
    );

    return MTextFormField(
      label: 'Full Name',
      hint: 'Jim Halpert',
      prefixIcon: MIcons.user,
      focusNode: focusNode,
      enabled: !isLoading,
      onChanged: di<RegisterFormManager>().onFullNameChanged,
      validator: (_) => fullName.errorMsg,
    );
  }
}

class _EmailField extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final focusNode = createOnce(FocusNode.new);

    final email = watchValue<RegisterFormManager, Email>(
      (manager) => manager.email,
    );

    final isLoading = watchValue<AuthManager, bool>(
      (manager) => manager.register.isRunning,
    );

    return MTextFormField(
      label: 'Email Address',
      hint: 'example@gmail.com',
      prefixIcon: MIcons.mail,
      keyboardType: .emailAddress,
      focusNode: focusNode,
      enabled: !isLoading,
      onChanged: di<RegisterFormManager>().onEmailChanged,
      validator: (_) => email.errorMsg,
    );
  }
}

class _PasswordField extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final focusNode = createOnce(FocusNode.new);

    final password = watchValue<RegisterFormManager, Password>(
      (manager) => manager.password,
    );

    final strength = watchValue<RegisterFormManager, double>(
      (manager) => manager.strength,
    );

    final isLoading = watchValue<AuthManager, bool>(
      (manager) => manager.register.isRunning,
    );

    final rules = [
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

    return Column(
      crossAxisAlignment: .stretch,
      mainAxisSize: .min,
      children: [
        MTextFormField(
          key: const Key('register_form_password_input'),
          hint: 'Must be at least 8 characters',
          label: 'Be secure',
          isPassword: true,
          prefixIcon: MIcons.key,
          onChanged: di<RegisterFormManager>().onPasswordChanged,
          enabled: !isLoading,
          focusNode: focusNode,
        ),
        const SizedBox(height: Insets.sm),
        MPasswordRuleTracker(
          strength: strength,
          touched: password.input.isNotEmpty,
          rules: rules,
        ),
      ],
    );
  }
}

class _TermsAndConditionsCheckBox extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final terms = watchValue<RegisterFormManager, TermsAcceptance>(
      (manager) => manager.terms,
    );

    final isRunning = watchValue<AuthManager, bool>(
      (manager) => manager.register.isRunning,
    );

    return MCheckboxFormField(
      key: const Key('register_form_terms'),
      label: 'By continuing you agree to our ',
      actionLabel: 'Terms of Service',
      onAction: () {},
      enabled: !isRunning,
      value: terms.input,
      onChanged: di<RegisterFormManager>().onTermsChanged,
    );
  }
}

class _RegisterButton extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final manager = di<RegisterFormManager>();

    final isLoading = watchValue<AuthManager, bool>(
      (manager) => manager.register.isRunning,
    );

    final isFormValid = watchValue<RegisterFormManager, bool>(
      (manager) => manager.isValid,
    );

    /// Sends the registration. The form is emptied once the API accepts it,
    /// not here: a refused registration must leave what was typed in place.
    void submit() {
      FocusScope.of(context).unfocus();
      final args = RegisterArgs(
        fullName: manager.fullName.value,
        email: manager.email.value,
        password: manager.password.value,
      );
      di<AuthManager>().register.run(args);
    }

    return MPrimaryButton(
      key: const Key('register_submit'),
      label: 'Create Your Account',
      loading: isLoading,
      disabled: isLoading || !isFormValid,
      onPressed: submit,
    );
  }
}
