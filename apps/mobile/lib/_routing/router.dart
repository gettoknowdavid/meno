import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/_routing/route_pages.dart';
import 'package:meno_mobile/_routing/router_guard.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/broadcast/pages/_pages.dart';
import 'package:meno_mobile/features/broadcast/widgets/broadcast_tab_widget.dart';
import 'package:meno_mobile/features/broadcast/widgets/pre_stream_modal.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_mobile/features/onboarding/pages/onboarding_page.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/features/settings/settings.dart';

/// The key of the router's root navigator.
///
/// Public because the overlay it builds is the only one that always exists:
/// anything outside the widget tree — the global failure reporter — reaches
/// toasts through it rather than through a page's context.
final rootNavigatorKey = GlobalKey<NavigatorState>(debugLabel: 'root');

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
    routes: [
      ..._publicRoutes,
      ..._standaloneRoutes,
      ..._broadcastFlow,
      ..._settingsRoutes,
      ..._bottomNavShell,
    ],
  );
}

List<RouteBase> get _publicRoutes => [
  GoRoute(path: MRoutes.loading, builder: (_, _) => const StubPage('Loading')),
  GoRoute(path: MRoutes.onboarding, builder: (_, _) => const OnboardingPage()),
  GoRoute(
    path: MRoutes.login,
    builder: (_, state) {
      final implyLeading = state.uri.queryParameters['implyLeading'] == 'true';
      return LoginPage(implyLeading: implyLeading);
    },
  ),
  GoRoute(
    path: MRoutes.register,
    builder: (_, state) {
      final implyLeading = state.uri.queryParameters['implyLeading'] != 'false';
      return RegisterPage(implyLeading: implyLeading);
    },
  ),
  GoRoute(
    path: MRoutes.emailVerification,
    builder: (_, state) {
      final email = state.uri.queryParameters['email'];
      return EmailVerificationPage(email: email);
    },
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
        builder: (_, state) {
          final email = state.uri.queryParameters['email'];
          return ResetPasswordOtpPage(email: email);
        },
      ),
      GoRoute(
        path: 'new',
        builder: (_, state) {
          final email = state.uri.queryParameters['email'];
          final code = state.uri.queryParameters['code'];
          return ResetPasswordPage(email: email, code: code);
        },
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
    path: MRoutes.liveSessionInit,
    builder: (_, _) => const LiveSessionInitPage(),
  ),
];

/// What a path that names no real resource shows.
const String _notFound = 'Not found';

/// The id [state]'s parameter [name] carries, or null when it is not a UUID.
///
/// A path parameter is untrusted input: a hand-edited link can say anything.
Id? _pathId(GoRouterState state, String name) {
  final raw = state.pathParameters[name];
  if (raw == null) return null;

  final id = Id.fromString(raw);
  return id.isValid ? id : null;
}

List<RouteBase> get _standaloneRoutes => [
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
    builder: (_, state) {
      final userId = _pathId(state, 'userId');
      if (userId == null) return const StubPage(_notFound);
      return StubPage('User Profile ${userId.getOrCrash}');
    },
  ),
  GoRoute(
    path: MRoutes.switchAccountPattern,
    builder: (_, state) {
      final userId = _pathId(state, 'userId');
      if (userId == null) return const StubPage(_notFound);
      return StubPage('Switch Account ${userId.getOrCrash}');
    },
  ),
  GoRoute(
    path: MRoutes.webCreateBroadcast,
    builder: (_, _) => const StubPage('Create Broadcast (Web)'),
  ),
];

List<RouteBase> get _broadcastFlow => [
  GoRoute(
    path: MRoutes.broadcasts,
    builder: (_, _) => const BroadcastsPage(queryParameters: {}),
    routes: [
      GoRoute(path: 'new', builder: (_, _) => const BroadcastEditorPage()),
      GoRoute(
        path: ':broadcastId',
        builder: (_, state) {
          final broadcastId = _pathId(state, 'broadcastId');
          if (broadcastId == null) return const StubPage(_notFound);
          return BroadcastDetailsPage(broadcastId: broadcastId);
        },
        routes: [
          GoRoute(path: 'edit', builder: (_, _) => const BroadcastEditorPage()),
          GoRoute(
            path: 'pre-stream',
            pageBuilder: (_, state) {
              final broadcastId = _pathId(state, 'broadcastId');
              if (broadcastId == null) {
                return const MaterialPage<void>(child: StubPage(_notFound));
              }
              return ModalPage<dynamic>(child: PreStreamModal(id: broadcastId));
            },
          ),
          GoRoute(
            path: 'live',
            pageBuilder: (context, state) {
              final navigationShell = state as StatefulNavigationShell;
              return CustomTransitionPage<void>(
                key: const ValueKey('liveSessionShell'),
                transitionsBuilder: (_, _, _, _) => const SizedBox.expand(),
                child: LiveSessionShell(
                  key: const ValueKey('liveSessionShell'),
                  navigationShell: navigationShell,
                  children: const [
                    BroadcastTabWidget(),
                    StubPage('Live Chat'),
                    StubPage('Live Bible'),
                    StubPage('Live Notes'),
                  ],
                ),
              );
            },
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
          GoRoute(path: 'ended', builder: (_, _) => const EndedBroadcastPage()),
        ],
      ),
    ],
  ),
];

List<RouteBase> get _settingsRoutes => [
  GoRoute(
    path: MRoutes.settings,
    builder: (_, _) => const SettingsPage(),
    routes: [
      GoRoute(
        path: 'notifications',
        builder: (_, _) => const NotificationsSettingsPage(),
      ),
      GoRoute(
        path: 'security',
        builder: (_, _) => const SecuritySettingsPage(),
      ),
      GoRoute(path: 'about', builder: (_, _) => const AboutPage()),
    ],
  ),
];

List<RouteBase> get _bottomNavShell => [
  StatefulShellRoute.indexedStack(
    builder: (_, _, navigationShell) {
      return RootShell(navigationShell: navigationShell);
    },
    branches: [
      StatefulShellBranch(
        routes: [
          GoRoute(path: MRoutes.home, builder: (_, _) => const HomePage()),
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
            builder: (_, _) => const MyProfilePage(),
          ),
        ],
      ),
    ],
  ),
];
