import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';

/// Reactive navigation guard for the app router.
///
/// Reads from the auth and onboarding managers, so redirects follow state
/// changes: [GoRouter] re-runs [redirect] whenever the managers notify through
/// the router's `refreshListenable`.
final class RouterGuard {
  const new({required this.auth, required this.onboarding});

  final AuthManager auth;
  final OnboardingManager onboarding;

  String? redirect(BuildContext context, GoRouterState state) {
    final location = state.matchedLocation;
    final isPublic = MRoutes.publicRoutes.contains(location);

    if (!onboarding.isOnboarded.value) {
      return isPublic ? null : MRoutes.onboarding;
    }

    if (!auth.isAuthenticated.value) {
      if (location.startsWith(MRoutes.switchAccountPrefix)) return null;
      return isPublic ? null : MRoutes.login;
    }

    if (!auth.emailVerified.value) {
      return location == MRoutes.emailVerification
          ? null
          : MRoutes.emailVerification;
    }

    if (isPublic) return MRoutes.home;

    return null;
  }
}
