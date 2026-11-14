import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_ui/meno_ui.dart';

/// Collects the reset code the API mailed.
///
/// This screen only carries the code forward to the page that sets the
/// password — which is also why a wrong code is reported there, next to the
/// password it refused to replace.
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
    registerHandler<AuthManager, void>(
      select: (manager) => manager.resendOtp,
      handler: (ctx, _, _) => ctx.showSuccessToast('A new code is on its way.'),
    );

    registerHandler<AuthManager, CommandError<OtpRequestArgs>?>(
      select: (manager) => manager.resendOtp.errors,
      handler: (ctx, e, _) => ctx.showErrorToast(errorMessage(e?.error)),
    );

    final account = watchValue<UserManager, User>(
      (manager) => manager.lastKnownUser,
    );

    final isResending = watchValue<AuthManager, bool>(
      (manager) => manager.resendOtp.isRunning,
    );

    final codeController = createOnce(TextEditingController.new);
    final address = email != null ? Email(email!) : account.email;
    final isValidAddress = address.isValid;

    final textTheme = MTextTheme.of(context);

    return MScaffold(
      appBar: MAppBar.primary(title: 'Reset Password'),
      body: SingleChildScrollView(
        padding: const .symmetric(vertical: Insets.xl),
        child: Column(
          crossAxisAlignment: .stretch,
          children: [
            MText('Enter the 6-digit code', style: textTheme.heading2Medium),
            Spaces.verticalSmall,
            MText(
              'We sent it to $address. Enter it to choose a new password.',
              style: textTheme.bodyRegular,
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
                      OtpRequestArgs(email: address, type: .resetPassword),
                    ),
            ),
            Spaces.verticalXXLarge,
            MPrimaryButton(
              key: const Key('reset_code_submit'),
              label: 'Continue',
              disabled: !isValidAddress,
              onPressed: () => context.push(
                MRoutes.createNewPasswordRoute(
                  email: address.orEmpty,
                  code: codeController.text,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
