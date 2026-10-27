import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/pending_destination.dart';
import 'package:meno_mobile/_routing/router.dart' show rootNavigatorKey;
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// Reactive navigation guard for the app router.
///
/// Reads the account lifecycle directly from [AccountTransitionOwner]'s
/// snapshot, so every change of phase, account, generation or verification
/// re-runs [redirect] — including an A→B switch, which leaves the old
/// boolean-based guard blind.
///
/// A destination the guard redirects away from is stashed in [pending]
/// instead of being thrown away, so a deep link survives a sign-in and lands
/// where it was aimed once the account is ready.
final class RouterGuard {
  const new({
    required this.onboarding,
    required this.owner,
    required this.pending,
  });

  final OnboardingManager onboarding;
  final AccountTransitionOwner owner;

  /// The one-slot holder for a destination that could not be shown yet.
  final PendingDestinationSlot pending;

  String? redirect(BuildContext context, GoRouterState state) {
    final location = state.matchedLocation;
    final isPublic = MRoutes.publicRoutes.contains(location);

    if (!onboarding.isOnboarded.value) {
      return isPublic ? null : MRoutes.onboarding;
    }

    final snapshot = owner.snapshot.value;

    if (snapshot.phase == .signedOut) {
      if (location.startsWith(MRoutes.switchAccountPrefix)) return null;
      if (isPublic) return null;
      _preserve(location);
      return MRoutes.login;
    }

    // Preparing, quiescing or blocked: no private route may render. The gate
    // shows the neutral page; the router keeps anything private off the stack.
    if (snapshot.phase != .ready) {
      if (isPublic || location == MRoutes.loading) return null;
      return MRoutes.loading;
    }

    if (!snapshot.isEmailVerified) {
      if (location == MRoutes.emailVerification) return null;
      _preserve(location);
      return MRoutes.emailVerification;
    }

    // A ready account consumes whatever the slot holds — a preserved deep
    // link, or a tap that arrived before the navigator existed.
    final destination = pending.peek();
    if (destination != null) {
      pending.take();
      final recipient = destination.recipientAccountId;
      if (recipient == null || recipient == snapshot.context?.accountId) {
        return destination.path;
      }
      // A tap addressed to another account is dropped, never executed as this
      // one: attribution is not proof of access (M19/M16).
      _snackbar('That notification is for a different account.');
      return null;
    }

    if (isPublic) return MRoutes.home;

    return null;
  }

  /// Keeps a private destination so it can be revisited once access exists.
  void _preserve(String location) {
    pending.set(PendingDestination(path: location));
  }

  /// Tells the user why a tap did nothing, when there is an overlay to say it
  /// on. Before the navigator exists the message is on its way to the log.
  void _snackbar(String message) {
    final overlay = rootNavigatorKey.currentState?.overlay;
    if (overlay == null) return;

    MToastController.of(overlay).show(
      MToastRequest(
        builder: (_, dismiss) =>
            MToast(message, variant: MToastVariant.error, onClose: dismiss),
      ),
    );
  }
}
