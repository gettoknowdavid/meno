import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_ui/meno_ui.dart';

/// Confirms the password was replaced.
///
/// A terminus: the reset flow's previous screens are behind a code that has now
/// been spent, so going back would only produce a failure. The one action is
/// forward, to the sign-in screen, which is where the new password is used.
class ResetPasswordSuccessPage extends StatelessWidget {
  /// Creates the success page.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    return MScaffold(
      padding: EdgeInsets.zero,
      body: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Assets.images.success.image(
            height: 240,
            width: 240,
            package: 'meno_ui',
          ),
          Spaces.verticalXXXLarge,
          MText(
            'Success!',
            style: textTheme.heading2Bold,
            textAlign: TextAlign.center,
          ),
          Spaces.verticalSmall,
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: Insets.xxxl),
            child: MText(
              'Your password has been reset. Sign in with your new password.',
              style: textTheme.bodyRegular,
              textAlign: TextAlign.center,
              maxLines: 3,
            ),
          ),
          Spaces.verticalXXXLarge,
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: Insets.lg),
            child: MPrimaryButton(
              label: 'Back to log in',
              onPressed: () => context.go(MRoutes.login),
            ),
          ),
        ],
      ),
    );
  }
}
