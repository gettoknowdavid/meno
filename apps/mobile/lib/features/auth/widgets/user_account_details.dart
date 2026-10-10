import 'package:flutter/foundation.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_ui/meno_ui.dart';

/// Greets the account that last signed in on this device.
///
/// The sign-in screen shows this instead of an email field when the app
/// already knows who is expected, which is the case every time somebody signs
/// out or has a session expire. Offering the account back — rather than an
/// empty form — is what makes returning to the app one tap instead of typing.
class UserAccountDetails extends StatelessWidget {
  /// Creates the account card.
  ///
  /// [user] is the account to greet and [onSwitchAccount] the action behind
  /// the avatar, which opens the accounts signed in on this device.
  const new({required this.user, super.key, this.onSwitchAccount});

  /// The account being greeted.
  final User user;

  /// Opens the account switcher, when one is available.
  final VoidCallback? onSwitchAccount;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Row(
      children: [
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              MText('Welcome back,', style: textTheme.subheadingMedium),
              MText(user.fullName, style: textTheme.heading2Medium),
            ],
          ),
        ),
        Spaces.horizontalLarge,
        Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            MAvatar(
              radius: Insets.xl,
              url: user.avatarUrl,
              onTap: onSwitchAccount,
              hasBorder: false,
            ),
            if (onSwitchAccount != null) ...[
              Spaces.verticalMicro,
              MText(
                'Switch account',
                style: textTheme.captionMedium,
                color: colors.primary,
              ),
            ],
          ],
        ),
      ],
    );
  }

  @override
  void debugFillProperties(DiagnosticPropertiesBuilder properties) {
    super.debugFillProperties(properties);
    properties
      ..add(DiagnosticsProperty<User>('user', user))
      ..add(
        DiagnosticsProperty<VoidCallback?>(
          'onSwitchAccount',
          onSwitchAccount,
          ifNull: 'no switcher',
        ),
      );
  }
}
