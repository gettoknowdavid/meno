import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_ui/meno_ui.dart';

/// Activates an account already held in this device's vault.
///
/// A deep link names the account to switch to, so there is no choice to draw:
/// the page performs the switch and shows a loading state while it happens.
/// The router guard owns what comes next — a successful switch changes the
/// active credential and rebuilds the signed-in tree — so the page only sends
/// the client home once the vault has committed.
class SwitchAccountPage extends WatchingWidget {
  const new({required this.userId, super.key});

  /// The account the link names.
  final Id userId;

  @override
  Widget build(BuildContext context) {
    final auth = di<AuthManager>();

    registerHandler<AuthManager, void>(
      select: (manager) => manager.switchAccount,
      handler: (ctx, _, _) => ctx.go(MRoutes.home),
    );

    registerHandler<AuthManager, CommandError<Id>?>(
      select: (manager) => manager.switchAccount.errors,
      handler: (ctx, error, _) {
        ctx.showErrorToast(errorMessage(error?.error));
        ctx.go(MRoutes.myProfile);
      },
    );

    callOnce((_) => auth.switchAccount.run(userId));

    return const MLoadingPage();
  }
}
