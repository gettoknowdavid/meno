import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/route_pages.dart';
import 'package:meno_mobile/_routing/router_guard.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_mobile/features/onboarding/pages/onboarding_page.dart';

/// The key of the router's root navigator.
///
/// Public because the overlay it builds is the only one that always exists:
/// anything outside the widget tree — the global failure reporter — reaches
/// toasts through it rather than through a page's context.
final GlobalKey<NavigatorState> rootNavigatorKey = GlobalKey<NavigatorState>(
  debugLabel: 'root',
);

/// Builds the application's [GoRouter].
///
/// The router is assembled in one place rather than across many small route
/// files: at this size a single route table reads clearly and keeps the guard
/// and shell logic next to the routes they cover.
GoRouter createRouter({
  required AuthManager auth,
  required OnboardingManager onboarding,
  bool debugLogDiagnostics = false,
}) {
  final guard = RouterGuard(auth: auth, onboarding: onboarding);

  return GoRouter(
    debugLogDiagnostics: debugLogDiagnostics,
    navigatorKey: rootNavigatorKey,
    initialLocation: MRoutes.home,
    refreshListenable: Listenable.merge([
      onboarding.isOnboarded,
      auth.isAuthenticated,
      auth.emailVerified,
    ]),
    redirect: guard.redirect,
    routes: _routes,
  );
}

List<RouteBase> get _routes => [
  GoRoute(path: MRoutes.loading, builder: (_, _) => const StubPage('Loading')),
  GoRoute(path: MRoutes.onboarding, builder: (_, _) => const OnboardingPage()),
  GoRoute(
    path: MRoutes.login,
    builder: (_, state) => LoginPage(
      implyLeading: state.uri.queryParameters['implyLeading'] == 'true',
    ),
  ),
  GoRoute(
    path: MRoutes.register,
    builder: (_, state) => RegisterPage(
      implyLeading: state.uri.queryParameters['implyLeading'] != 'false',
    ),
  ),
  GoRoute(
    path: MRoutes.emailVerification,
    builder: (_, state) =>
        EmailVerificationPage(email: state.uri.queryParameters['email']),
  ),
  GoRoute(
    path: MRoutes.forgotPassword,
    builder: (_, _) => const ForgotPasswordPage(),
  ),
  // `/reset-password` is the flow's root, not a screen: the address is asked for
  // at `/forgot-password`, and the API's own name for asking is
  // `forgot-password`. A deep link to the bare path therefore starts the flow
  // rather than showing a duplicate form.
  GoRoute(
    path: MRoutes.resetPassword,
    redirect: (_, _) => MRoutes.forgotPassword,
    routes: [
      GoRoute(
        path: 'otp',
        builder: (_, state) =>
            ResetPasswordOtpPage(email: state.uri.queryParameters['email']),
      ),
      GoRoute(
        path: 'new',
        builder: (_, state) => ResetPasswordPage(
          email: state.uri.queryParameters['email'],
          code: state.uri.queryParameters['code'],
        ),
      ),
      GoRoute(
        path: 'success',
        builder: (_, _) => const ResetPasswordSuccessPage(),
      ),
    ],
  ),
  GoRoute(
    path: MRoutes.authGoogle,
    builder: (_, _) => const StubPage('Google Sign-in'),
  ),
  GoRoute(
    path: MRoutes.notifications,
    builder: (_, _) => const StubPage('Notifications'),
  ),
  GoRoute(path: MRoutes.nowLive, builder: (_, _) => const StubPage('Now Live')),
  GoRoute(
    path: MRoutes.recentlyLive,
    builder: (_, _) => const StubPage('Recently Live'),
  ),
  GoRoute(path: MRoutes.bible, builder: (_, _) => const StubPage('Bible')),
  GoRoute(
    path: MRoutes.profiles,
    builder: (_, _) => const StubPage('Profiles'),
  ),
  GoRoute(
    path: MRoutes.userProfilePattern,
    builder: (_, state) =>
        StubPage('Profile ${state.pathParameters['userId'] ?? ''}'),
  ),
  GoRoute(
    path: MRoutes.switchAccountPattern,
    builder: (_, state) =>
        StubPage('Switch Account ${state.pathParameters['userId'] ?? ''}'),
  ),
  GoRoute(
    path: MRoutes.settings,
    builder: (_, _) => const StubPage('Settings'),
    routes: [
      GoRoute(
        path: 'notifications',
        builder: (_, _) => const StubPage('Notification Settings'),
      ),
      GoRoute(
        path: 'security',
        builder: (_, _) => const StubPage('Security Settings'),
      ),
      GoRoute(path: 'about', builder: (_, _) => const StubPage('About')),
    ],
  ),
  GoRoute(
    path: MRoutes.folders,
    builder: (_, _) => const StubPage('Folders'),
    routes: [
      GoRoute(
        path: ':folderId',
        builder: (_, state) =>
            StubPage('Folder ${state.pathParameters['folderId'] ?? ''}'),
      ),
    ],
  ),
  GoRoute(
    path: MRoutes.broadcasts,
    builder: (_, _) => const StubPage('Broadcasts'),
    routes: [
      GoRoute(
        path: 'new',
        builder: (_, _) => const StubPage('Create Broadcast'),
      ),
      GoRoute(
        path: ':broadcastId',
        builder: (_, state) =>
            StubPage('Broadcast ${state.pathParameters['broadcastId'] ?? ''}'),
        routes: [
          GoRoute(
            path: 'edit',
            builder: (_, _) => const StubPage('Edit Broadcast'),
          ),
          GoRoute(
            path: 'pre-stream',
            pageBuilder: (_, state) => ModalPage<dynamic>(
              child: StubPage(
                'Pre-stream ${state.pathParameters['broadcastId'] ?? ''}',
              ),
            ),
          ),
          GoRoute(
            path: 'live',
            builder: (_, _) => const StubPage('Live Broadcast'),
            routes: [
              GoRoute(
                path: 'chat',
                builder: (_, _) => const StubPage('Live Chat'),
              ),
              GoRoute(
                path: 'bible',
                builder: (_, _) => const StubPage('Live Bible'),
              ),
              GoRoute(
                path: 'notes',
                builder: (_, _) => const StubPage('Live Notes'),
                routes: [
                  GoRoute(
                    path: ':noteId',
                    builder: (_, state) => StubPage(
                      'Live Note ${state.pathParameters['noteId'] ?? ''}',
                    ),
                  ),
                ],
              ),
              GoRoute(
                path: 'folders',
                builder: (_, _) => const StubPage('Live Folders'),
                routes: [
                  GoRoute(
                    path: ':folderId',
                    builder: (_, state) => StubPage(
                      'Live Folder ${state.pathParameters['folderId'] ?? ''}',
                    ),
                  ),
                ],
              ),
            ],
          ),
          GoRoute(
            path: 'ended',
            builder: (_, _) => const StubPage('Broadcast Ended'),
          ),
        ],
      ),
    ],
  ),
  GoRoute(
    path: MRoutes.webCreateBroadcast,
    builder: (_, _) => const StubPage('Create Broadcast (Web)'),
  ),
  _rootShell,
];

StatefulShellRoute get _rootShell => StatefulShellRoute.indexedStack(
  builder: (_, _, navigationShell) =>
      RootShell(navigationShell: navigationShell),
  branches: [
    StatefulShellBranch(
      routes: [
        GoRoute(path: MRoutes.home, builder: (_, _) => const StubPage('Home')),
      ],
    ),
    StatefulShellBranch(
      routes: [
        GoRoute(
          path: MRoutes.discover,
          builder: (_, _) => const StubPage('Discover'),
          routes: [
            GoRoute(
              path: 'all',
              builder: (_, _) => const StubPage('All Broadcasts'),
            ),
            GoRoute(
              path: 'now-live',
              builder: (_, _) => const StubPage('Discover Now Live'),
            ),
            GoRoute(
              path: 'recently-live',
              builder: (_, _) => const StubPage('Discover Recently Live'),
            ),
            GoRoute(
              path: 'accounts',
              builder: (_, _) => const StubPage('Suggested Accounts'),
            ),
            GoRoute(
              path: 'search',
              builder: (_, _) => const StubPage('Discover Search'),
            ),
          ],
        ),
      ],
    ),
    StatefulShellBranch(
      routes: [
        GoRoute(
          path: MRoutes.notes,
          builder: (_, _) => const StubPage('Notes'),
        ),
      ],
    ),
    StatefulShellBranch(
      routes: [
        GoRoute(
          path: MRoutes.myProfile,
          builder: (_, _) => const StubPage('My Profile'),
        ),
      ],
    ),
  ],
);
