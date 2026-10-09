import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/route_pages.dart';
import 'package:meno_mobile/_routing/router_guard.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_ui/meno_ui.dart';

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
  GoRoute(
    path: MRoutes.onboarding,
    builder: (_, _) => const StubPage('Onboarding'),
  ),
  GoRoute(path: MRoutes.login, builder: (_, _) => const StubPage('Login')),
  GoRoute(
    path: MRoutes.register,
    builder: (_, _) => const StubPage('Register'),
  ),
  GoRoute(
    path: MRoutes.emailVerification,
    builder: (_, _) => const StubPage('Verify Email'),
  ),
  GoRoute(
    path: MRoutes.forgotPassword,
    builder: (_, _) => const StubPage('Forgot Password'),
  ),
  GoRoute(
    path: MRoutes.resetPassword,
    builder: (_, _) => const StubPage('Reset Password'),
    routes: [
      GoRoute(
        path: 'otp',
        builder: (_, _) => const StubPage('Reset Password OTP'),
      ),
      GoRoute(
        path: 'new',
        builder: (_, _) => const StubPage('Create New Password'),
      ),
      GoRoute(
        path: 'success',
        builder: (_, _) => const StubPage('Reset Password Success'),
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
      _RootShell(navigationShell: navigationShell),
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

/// Root bottom-navigation shell — Home, Discover, Notes, Profile.
class _RootShell extends StatelessWidget {
  const new({required this.navigationShell});

  final StatefulNavigationShell navigationShell;

  @override
  Widget build(BuildContext context) {
    return MScaffold(
      padding: EdgeInsets.zero,
      body: navigationShell,
      bottomNavigationBar: MBottomNavigationBar(
        currentIndex: navigationShell.currentIndex,
        onTap: (index) => navigationShell.goBranch(
          index,
          initialLocation: index == navigationShell.currentIndex,
        ),
        items: const [
          BottomNavigationBarItem(
            icon: Icon(Icons.home_outlined),
            label: 'Home',
          ),
          BottomNavigationBarItem(icon: Icon(Icons.search), label: 'Discover'),
          BottomNavigationBarItem(
            icon: Icon(Icons.article_outlined),
            label: 'Notes',
          ),
          BottomNavigationBarItem(
            icon: Icon(Icons.person_outline),
            label: 'Profile',
          ),
        ],
      ),
    );
  }
}
