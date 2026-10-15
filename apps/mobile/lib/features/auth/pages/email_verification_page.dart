import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
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
/// cannot flood the address with codes. The cooldown logic lives in
/// [EmailVerificationManager].
///
/// The page is a dead end: there is no back affordance, and the only way out
/// besides verifying is the Cancel button, which signs the account out so the
/// router redirect can send the user back here if needed.
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
    pushScope(
      init: (getIt) {
        getIt.registerSingleton<EmailVerificationManager>(
          EmailVerificationManager(auth: di<AuthManager>()),
        );
      },
    );

    registerHandler<EmailVerificationManager, void>(
      select: (m) => m.canResend,
      handler: (ctx, _, cancel) {
        // if (cancel) return;

        final address = email != null
            ? Email(email!)
            : di<UserManager>().currentUser.value.email;

        if (!address.isValid) return;
        di<EmailVerificationManager>().startCooldown();
        di<AuthManager>().resendOtp.run(
          OtpRequestArgs(email: address, type: .verifyEmail),
        );
      },
    );

    registerHandler<AuthManager, void>(
      select: (manager) => manager.resendOtp,
      handler: (ctx, _, _) => ctx.showSuccessToast('A new code is on its way.'),
    );

    registerHandler<AuthManager, CommandError<VerifyEmailArgs>?>(
      select: (manager) => manager.verifyEmail.errors,
      handler: (ctx, e, _) => ctx.showErrorToast(errorMessage(e?.error)),
    );

    registerHandler<AuthManager, CommandError<OtpRequestArgs>?>(
      select: (m) => m.resendOtp.errors,
      handler: (context, error, _) {
        context.showErrorToast(errorMessage(error?.error));

        // A failed resend is still a resend attempt, so the cooldown still
        // starts: the timer is about rate-limiting the link, not about whether
        // the network answered.
        di<EmailVerificationManager>().startCooldown();
      },
    );

    registerHandler<
      AuthManager,
      CommandResult<VerifyEmailArgs?, UserCredential>
    >(
      select: (manager) => manager.verifyEmail.results,
      handler: (context, result, _) {
        if (!result.isSuccess) return;
        context.go(MRoutes.home);
      },
    );

    registerHandler<AuthManager, CommandError<VerifyEmailArgs>?>(
      select: (manager) => manager.verifyEmail.errors,
      handler: (ctx, error, _) {
        ctx.showErrorToast(errorMessage(error?.error));
      },
    );

    registerHandler<AuthManager, void>(
      select: (manager) => manager.logout,
      handler: (context, _, _) {
        context.go(MRoutes.loginRoute(implyLeading: false));
      },
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
    final canResend = watchValue<EmailVerificationManager, bool>(
      (m) => m.canResend,
    );

    final codeController = createOnce(TextEditingController.new);
    final address = email != null ? Email(email!) : account.email;
    final isValidAddress = address.isValid;

    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final resendCountdown = watchValue<EmailVerificationManager, int>(
      (m) => m.remaining,
    );

    final canVerify = isValidAddress && !isVerifying;

    return MScaffold(
      appBar: MAppBar.primary(title: 'Verify your email', implyLeading: false),
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
                  TextSpan(
                    text: address.getOrElse((_) => ''),
                    style: textTheme.captionMedium,
                  ),
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
            if (resendCountdown > 0)
              MText(
                'Resend code in ${_formatCountdown(resendCountdown)}',
                textAlign: .center,
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
                onAction: canResend && isValidAddress
                    ? () => di<EmailVerificationManager>().startCooldown()
                    : null,
              ),
            Spaces.verticalXXLarge,
            MPrimaryButton(
              key: const Key('verify_submit'),
              label: 'Verify',
              loading: isVerifying,
              disabled: !canVerify,
              onPressed: () => di<AuthManager>().verifyEmail.run(
                VerifyEmailArgs(email: address, code: codeController.text),
              ),
            ),
            Spaces.verticalLarge,
            MTextButton(
              label: 'Cancel',
              onPressed: isVerifying
                  ? null
                  : () => di<AuthManager>().logout.run(),
            ),
          ],
        ),
      ),
    );
  }
}

/// Formats a countdown in seconds as `MM:SS`, always two digits each.
String _formatCountdown(int seconds) {
  final remainingSeconds = seconds.remainder(60).clamp(0, 59);
  final minutes = (seconds / 60).floor().clamp(0, 59);
  return '''${minutes.toString().padLeft(2, '0')}:${remainingSeconds.toString().padLeft(2, '0')}''';
}
