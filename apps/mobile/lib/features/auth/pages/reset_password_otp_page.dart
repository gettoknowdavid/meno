import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/manager/user_manager.dart';
import 'package:meno_mobile/features/auth/model/entities/otp_type.dart';
import 'package:meno_ui/meno_ui.dart';

/// Collects the reset code the API mailed.
///
/// The code is *not* checked here, because the API has no endpoint that checks
/// one on its own: `POST /auth/reset-password` takes the address, the code and
/// the new password together, and the code is what authorises the change. So this
/// screen only carries the code forward to the page that sets the password —
/// which is also why a wrong code is reported there, next to the password it
/// refused to replace.
class ResetPasswordOtpPage extends WatchingWidget {
  /// Creates the reset-code page.
  ///
  /// [email] is the address the code was sent to, from the route's query. It
  /// falls back to the account that last signed in on this device.
  const new({super.key, this.email});

  /// The address the code was sent to, when the route carried one.
  final String? email;

  @override
  Widget build(BuildContext context) {
    registerHandler(
      select: (AuthManager manager) => manager.resendOtp,
      handler: (context, _, _) =>
          context.showSuccessToast('A new code is on its way.'),
    );
    registerHandler(
      select: (AuthManager manager) => manager.resendOtp.errors,
      handler: (context, error, _) =>
          context.showErrorToast(errorMessage(error?.error)),
    );

    final account = watchValue((UserManager manager) => manager.lastKnownUser);
    final isResending = watchValue(
      (AuthManager manager) => manager.resendOtp.isRunning,
    );

    final codeController = createOnce(TextEditingController.new);
    final address = email ?? account.email;
    final isValidAddress = Email(address).isValid;

    return MScaffold(
      appBar: MAppBar.primary(title: 'Reset Password'),
      body: SingleChildScrollView(
        padding: const EdgeInsets.symmetric(vertical: Insets.xl),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            MText(
              'Enter the 6-digit code',
              style: MTextTheme.of(context).heading2Medium,
            ),
            Spaces.verticalSmall,
            MText(
              'We sent it to $address. Enter it to choose a new password.',
              style: MTextTheme.of(context).bodyRegular,
              maxLines: 3,
            ),
            Spaces.verticalXXLarge,
            MOtpField(
              key: const Key('reset_code'),
              length: 6,
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
                        type: OtpType.resetPassword,
                      ),
                    ),
            ),
            Spaces.verticalXXLarge,
            MPrimaryButton(
              key: const Key('reset_code_submit'),
              label: 'Continue',
              disabled: !isValidAddress,
              onPressed: () => context.push(
                MRoutes.createNewPasswordRoute(
                  email: address,
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
