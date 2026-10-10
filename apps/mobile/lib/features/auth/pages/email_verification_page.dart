import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/manager/user_manager.dart';
import 'package:meno_mobile/features/auth/model/entities/otp_type.dart';
import 'package:meno_ui/meno_ui.dart';

/// Confirms the signed-in account's address with a one-time code.
///
/// The code is what the API mails to the address, six digits, because that is
/// what `apps/api/src/modules/auth/mailer.rs` sends and what
/// `code_problem` accepts. A four-digit field — as this screen used to show —
/// could never have been verified.
///
/// Verifying replaces the session: the API returns a fresh token pair for the
/// now-verified account, so [AuthManager.verifyEmail] adopts it and the router
/// guard sees [AuthManager.emailVerified] flip and moves the user on.
class EmailVerificationPage extends WatchingWidget {
  /// Creates the verification page.
  ///
  /// [email] is the address the code was sent to, from the route's query. It
  /// falls back to the account that is signed in.
  const new({super.key, this.email});

  /// The address being verified, when the route carried one.
  final String? email;

  @override
  Widget build(BuildContext context) {
    registerHandler<AuthManager, void>(
      select: (manager) => manager.resendOtp,
      handler: (context, _, _) =>
          context.showSuccessToast('A new code is on its way.'),
    );
    registerHandler(
      select: (AuthManager manager) => manager.verifyEmail.errors,
      handler: (context, error, _) =>
          context.showErrorToast(errorMessage(error?.error)),
    );
    registerHandler(
      select: (AuthManager manager) => manager.resendOtp.errors,
      handler: (context, error, _) =>
          context.showErrorToast(errorMessage(error?.error)),
    );

    final account = watchValue((UserManager manager) => manager.currentUser);
    final isVerifying = watchValue(
      (AuthManager manager) => manager.verifyEmail.isRunning,
    );
    final isResending = watchValue(
      (AuthManager manager) => manager.resendOtp.isRunning,
    );

    final codeController = createOnce(TextEditingController.new);
    final address = email ?? account.email;
    final isValidAddress = Email(address).isValid;

    return MScaffold(
      appBar: MAppBar.primary(title: 'Verify Your Email'),
      body: SingleChildScrollView(
        padding: const EdgeInsets.symmetric(vertical: Insets.xl),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            MText(
              'Check your inbox',
              style: MTextTheme.of(context).heading2Medium,
            ),
            Spaces.verticalSmall,
            MText(
              'Enter the 6-digit code we sent to $address to continue.',
              style: MTextTheme.of(context).bodyRegular,
              maxLines: 3,
            ),
            Spaces.verticalXXLarge,
            MOtpField(
              key: const Key('verify_code'),
              length: 6,
              enabled: !isVerifying,
              controller: codeController,
            ),
            Spaces.verticalXLarge,
            MInlineActionText(
              label: "Didn't receive a code?",
              actionLabel: isResending ? 'Sending…' : 'Send again',
              onAction: isResending || !isValidAddress
                  ? null
                  : () => di<AuthManager>().resendOtp.run(
                      OtpRequestArgs(
                        email: Email(address),
                        type: OtpType.verifyEmail,
                      ),
                    ),
            ),
            Spaces.verticalXXLarge,
            MPrimaryButton(
              key: const Key('verify_submit'),
              label: 'Verify',
              loading: isVerifying,
              disabled: isVerifying || !isValidAddress,
              onPressed: () => di<AuthManager>().verifyEmail.run(
                VerifyEmailArgs(
                  email: Email(address),
                  code: codeController.text,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  @override
  void debugFillProperties(DiagnosticPropertiesBuilder properties) {
    super.debugFillProperties(properties);
    properties.add(StringProperty('email', email));
  }
}
