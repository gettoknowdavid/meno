import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// Asks the API to mail a password reset code.
///
/// `POST /auth/forgot-password` answers the same way whether or not the address
/// exists, so a successful send here is not proof that an account was found.
class ForgotPasswordPage extends WatchingWidget {
  /// Creates the forgot-password page.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    registerHandler<AuthManager, CommandResult<Email?, void>>(
      select: (manager) => manager.forgotPassword.results,
      handler: (context, result, _) {
        final email = result.paramData;
        if (!result.isSuccess || email == null) return;
        unawaited(
          context.push(MRoutes.resetPasswordOtpRoute(email: email.getOrNull)),
        );
      },
    );
    registerHandler<AuthManager, CommandError<Email>?>(
      select: (manager) => manager.forgotPassword.errors,
      handler: (context, error, _) =>
          context.showErrorToast(errorMessage(error?.error)),
    );

    final isSending = watchValue<AuthManager, bool>(
      (manager) => manager.forgotPassword.isRunning,
    );

    final formKey = createOnce(GlobalKey<FormState>.new);
    final emailController = createOnce(TextEditingController.new);

    return MScaffold(
      appBar: MAppBar.primary(title: 'Reset Password'),
      body: Form(
        key: formKey,
        autovalidateMode: AutovalidateMode.onUserInteraction,
        child: SingleChildScrollView(
          padding: const EdgeInsets.symmetric(vertical: Insets.xl),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              MText(
                'Enter the email address on your account and we will send you '
                'a code to reset your password.',
                style: MTextTheme.of(context).bodyRegular,
                maxLines: 3,
              ),
              Spaces.verticalXXLarge,
              MTextFormField(
                key: const Key('forgot_email'),
                label: 'Email Address',
                hint: 'example@gmail.com',
                prefixIcon: MIcons.mail,
                keyboardType: TextInputType.emailAddress,
                controller: emailController,
                enabled: !isSending,
                validator: (_) => Email(emailController.text).errorMessage,
              ),
              Spaces.verticalXXLarge,
              MPrimaryButton(
                key: const Key('forgot_submit'),
                label: 'Send instructions',
                loading: isSending,
                onPressed: () {
                  if (formKey.currentState?.validate() != true) return;
                  di<AuthManager>().forgotPassword.run(
                    Email(emailController.text),
                  );
                },
              ),
            ],
          ),
        ),
      ),
    );
  }
}
