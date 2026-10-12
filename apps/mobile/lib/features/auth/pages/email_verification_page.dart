import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_ui/meno_ui.dart';

/// Confirms the signed-in account's address with a one-time code.
///
/// Verifying replaces the session: the API returns a fresh token pair for the
/// now-verified account, so [AuthManager.verifyEmail] adopts it and the router
/// guard sees [AuthManager.emailVerified] flip and moves the user on.
///
/// After a resend the "Send again" link is disabled for one minute so the user
/// cannot flood the address with codes. The countdown is a screen-local concern
/// and lives only as long as the page.
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
    registerHandler<AuthManager, CommandError<VerifyEmailArgs>?>(
      select: (manager) => manager.verifyEmail.errors,
      handler: (context, error, _) =>
          context.showErrorToast(errorMessage(error?.error)),
    );
    registerHandler<AuthManager, CommandError<OtpRequestArgs>?>(
      select: (manager) => manager.resendOtp.errors,
      handler: (context, error, _) =>
          context.showErrorToast(errorMessage(error?.error)),
    );

    final account = watchValue<UserManager, User>(
      (manager) => manager.currentUser,
    );
    final isVerifying = watchValue<AuthManager, bool>(
      (manager) => manager.verifyEmail.isRunning,
    );
    final isResending = watchValue<AuthManager, bool>(
      (manager) => manager.resendOtp.isRunning,
    );

    final codeController = createOnce(TextEditingController.new);
    final address = email ?? account.email;
    final isValidAddress = Email(address).isValid;

    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    // One minute after the last resend the link and button are usable again.
    final resendCooldown = createOnce(
      () => ValueNotifier<Duration>(Duration.zero),
    );

    registerHandler<AuthManager, CommandError<OtpRequestArgs>?>(
      select: (manager) => manager.resendOtp.errors,
      handler: (ctx, error, _) {
        ctx.showErrorToast(errorMessage(error?.error));

        // A failed resend still means the user asked for a new code, so the
        // cooldown still starts: the timer is about rate-limiting the link,
        // not about whether the network answered.
        const remaining = Duration(minutes: 1);
        resendCooldown.value = remaining;
        Timer.periodic(
          const Duration(seconds: 1),
          (timer) {
            final next =
                resendCooldown.value - const Duration(seconds: 1);
            if (next.isNegative) {
              resendCooldown.value = Duration.zero;
              timer.cancel();
            } else {
              resendCooldown.value = next;
            }
          },
        );
      },
    );

    final onResend =
        isResending || !isValidAddress || resendCooldown.value > Duration.zero
        ? null
        : () => di<AuthManager>().resendOtp.run(
            OtpRequestArgs(email: Email(address), type: .verifyEmail),
          );

    final canVerify = isValidAddress && !isVerifying;

    return MScaffold(
      appBar: MAppBar.primary(title: 'Verify your email'),
      body: SingleChildScrollView(
        padding: const EdgeInsets.symmetric(vertical: Insets.xl),
        child: Column(
          crossAxisAlignment: .stretch,
          children: [
            MText.rich(
              TextSpan(
                children: [
                  const TextSpan(
                    text: 'Enter the 6-digit code we just sent to ',
                  ),
                  TextSpan(text: address, style: textTheme.captionMedium),
                  const TextSpan(text: ' to continue.'),
                ],
                style: textTheme.captionRegular.copyWith(
                  color: colors.inActive,
                ),
              ),
            ),
            Spaces.verticalXXLarge,
            MOtpField(
              key: const Key('verify_code'),
              length: 6,
              enabled: canVerify,
              controller: codeController,
              padding: const .all(12),
              constraints: const BoxConstraints(maxHeight: 66, maxWidth: 88),
            ),
            Spaces.verticalXLarge,
            if (resendCooldown.value > Duration.zero)
              MText(
                'Resend code in ${_formatCountdown(resendCooldown.value)}',
                style: textTheme.captionRegular.copyWith(
                  color: colors.inActive,
                ),
              )
            else
              MInlineActionText(
                label: "Didn't receive a code?",
                actionLabel: isResending ? 'Sending…' : 'Send again',
                labelTextStyle: textTheme.captionRegular.copyWith(
                  color: colors.inActive,
                ),
                actionLabelTextStyle: textTheme.captionMedium,
                onAction: onResend,
              ),
            Spaces.verticalXXLarge,
            MPrimaryButton(
              key: const Key('verify_submit'),
              label: 'Verify',
              loading: isVerifying,
              disabled: !canVerify,
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
}

/// Formats a countdown duration as `MM:SS`, always two digits each.
String _formatCountdown(Duration duration) {
  final totalSeconds =
      duration.inSeconds.remainder(60).abs().clamp(0, 59);
  final minutes =
      duration.inMinutes.remainder(60).clamp(0, 59);
  return '${minutes.toString().padLeft(2, '0')}:${totalSeconds.toString().padLeft(2, '0')}';
}
