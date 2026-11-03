import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/_routing/pending_destination.dart';
import 'package:meno_mobile/_routing/route_pages.dart';
import 'package:meno_mobile/_routing/router_guard.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/app/layouts/root_shell.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/broadcast/layouts/live_session_shell.dart';
import 'package:meno_mobile/features/broadcast/pages/_pages.dart';
import 'package:meno_mobile/features/broadcast/widgets/pre_stream_modal.dart';
import 'package:meno_mobile/features/chat/widgets/live_chat_tab.dart';
import 'package:meno_mobile/features/discover/discover.dart';
import 'package:meno_mobile/features/notes/notes.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_mobile/features/onboarding/pages/onboarding_page.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/features/settings/settings.dart';

/// What a path that names no real resource shows.
const String _notFound = 'Not found';

/// The key of the router's root navigator.
///
/// Public because the overlay it builds is the only one that always exists:
/// anything outside the widget tree — the global failure reporter — reaches
/// toasts through it rather than through a page's context.
final rootNavigatorKey = GlobalKey<NavigatorState>(debugLabel: 'root');

/// The navigator the Discover tabs hang under.
///
/// The bottom bar's branch for Discover holds a shell of its own — three tabs,
/// one per list — and a nested shell has to name the navigator it is displayed
/// in rather than being placed on the root (go_router's `parentNavigatorKey`).
final discoverBranchKey = GlobalKey<NavigatorState>(debugLabel: 'discover');
final liveBroadcastKey = GlobalKey<NavigatorState>(debugLabel: 'liveBroadcast');
final liveChatKey = GlobalKey<NavigatorState>(debugLabel: 'liveChat');
final liveBibleKey = GlobalKey<NavigatorState>(debugLabel: 'liveBible');
final liveNotesKey = GlobalKey<NavigatorState>(debugLabel: 'liveNotes');

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
        navigatorKey: discoverBranchKey,
        initialLocation: MRoutes.discoverAllTab,
        routes: [
          GoRoute(
            path: MRoutes.discover,
            // Discover has no screen of its own: it is its first tab. A link to
            // the bare path is a link to "all", not a fourth page.
            redirect: (_, _) => MRoutes.discoverAllTab,
          ),
          StatefulShellRoute(
            parentNavigatorKey: discoverBranchKey,
            builder: (_, _, navigationShell) => navigationShell,
            navigatorContainerBuilder: DiscoverShell.builder,
            branches: [
              StatefulShellBranch(
                routes: [
                  GoRoute(
                    path: MRoutes.discoverAllTab,
                    builder: (_, _) => const AllBroadcastsWidget(),
                  ),
                ],
              ),
              StatefulShellBranch(
                routes: [
                  GoRoute(
                    path: MRoutes.discoverNowLiveTab,
                    builder: (_, _) => const NowLiveBroadcastsWidget(),
                  ),
                ],
              ),
              StatefulShellBranch(
                routes: [
                  GoRoute(
                    path: MRoutes.discoverRecentlyLiveTab,
                    builder: (_, _) => const RecentlyLiveBroadcastsWidget(),
                  ),
                ],
              ),
            ],
          ),
        ],
      ),
      StatefulShellBranch(
        routes: [
          GoRoute(
            path: MRoutes.notes,
            builder: (_, _) => const MyNotesPage(),
            // routes: [
            //   GoRoute(
            //     path: ':noteId',
            //     builder: (_, state) => NoteEditorPage(
            //       noteIdStr: state.pathParameters['noteId'],
            //     ),
            //   ),
            // ],
          ),
          // GoRoute(
          //   path: MRoutes.folders,
          //   // Folders are the second tab of the notes screen, not a destination
          //   // of their own: a link to the list is a link to where it is drawn.
          //   redirect: (_, _) => MRoutes.notes,
          //   routes: [
          //     GoRoute(
          //       path: ':folderId',
          //       builder: (_, state) => FolderPage(
          //         folderIdStr: state.pathParameters['folderId'] ?? '',
          //       ),
          //     ),
          //   ],
          // ),
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

List<RouteBase> get _broadcastRoutes => [
  GoRoute(
    path: MRoutes.broadcasts,
    name: MRoutes.broadcasts,
    builder: (_, state) =>
        BroadcastsPage(queryParameters: state.uri.queryParameters),
    routes: [
      GoRoute(
        path: 'new',
        pageBuilder: (_, state) => CustomTransitionPage<void>(
          key: state.pageKey,
          child: const BroadcastEditorPage(),
          transitionDuration: const Duration(milliseconds: 450),
          reverseTransitionDuration: const Duration(milliseconds: 400),
          transitionsBuilder: (_, animation, _, child) {
            final curved = CurvedAnimation(
              parent: animation,
              curve: Curves.easeOutCirc,
              reverseCurve: Curves.easeInCubic,
            );
            return SlideTransition(
              position: Tween<Offset>(
                begin: const Offset(0, 1),
                end: Offset.zero,
              ).animate(curved),
              child: FadeTransition(
                opacity: Tween<double>(begin: 0, end: 1).animate(curved),
                child: child,
              ),
            );
          },
        ),
      ),
      GoRoute(
        path: ':broadcastId',
        builder: (_, state) {
          final broadcastId = _pathId(state, 'broadcastId');
          if (broadcastId == null) return const StubPage(_notFound);
          return BroadcastDetailsPage(broadcastId: broadcastId);
        },
        routes: [
          GoRoute(
            path: 'edit',
            pageBuilder: (_, state) => CustomTransitionPage<void>(
              key: state.pageKey,
              child: const BroadcastEditorPage(),
              transitionDuration: const Duration(milliseconds: 310),
              reverseTransitionDuration: const Duration(milliseconds: 250),
              transitionsBuilder: (_, animation, _, child) {
                final curved = CurvedAnimation(
                  parent: animation,
                  curve: Curves.easeOutCirc,
                  reverseCurve: Curves.easeInCubic,
                );
                return SlideTransition(
                  position: Tween<Offset>(
                    begin: const Offset(0, 1),
                    end: Offset.zero,
                  ).animate(curved),
                  child: FadeTransition(
                    opacity: Tween<double>(begin: 0, end: 1).animate(curved),
                    child: child,
                  ),
                );
              },
            ),
          ),
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
            path: 'ended',
            builder: (_, state) {
              final broadcastId = _pathId(state, 'broadcastId');
              if (broadcastId == null) return const StubPage(_notFound);
              return EndedBroadcastPage(broadcastId: broadcastId);
            },
          ),
        ],
      ),
    ],
  ),
];

List<RouteBase> get _liveSessionRoutes => [
  StatefulShellRoute(
    parentNavigatorKey: rootNavigatorKey,
    builder: (_, _, navigationShell) => navigationShell,
    navigatorContainerBuilder: LiveSessionShell.builder,
    branches: [
      StatefulShellBranch(
        navigatorKey: liveBroadcastKey,
        routes: [
          GoRoute(
            path: MRoutes.liveBroadcast,
            builder: (_, _) => const LiveBroadcastPage(),
          ),
        ],
      ),
      StatefulShellBranch(
        navigatorKey: liveChatKey,
        routes: [
          GoRoute(
            path: MRoutes.liveChat,
            builder: (_, _) => const LiveChatTab(),
          ),
        ],
      ),
      StatefulShellBranch(
        navigatorKey: liveBibleKey,
        routes: [
          GoRoute(
            path: MRoutes.liveBible,
            builder: (_, _) => const StubPage('Bible'),
          ),
        ],
      ),
      StatefulShellBranch(
        navigatorKey: liveNotesKey,
        routes: [
          GoRoute(
            path: MRoutes.liveNotes,
            builder: (_, _) => const LiveNotesTab(),
            routes: [
              GoRoute(
                path: ':noteId',
                builder: (_, state) =>
                    NoteEditorPage(noteIdStr: state.pathParameters['noteId']),
              ),
            ],
          ),
          GoRoute(
            path: MRoutes.liveFolders,
            builder: (_, _) => const StubPage('Folders'),
            routes: [
              GoRoute(
                path: ':folderId',
                builder: (_, state) => FolderPage(
                  folderIdStr: state.pathParameters['folderId'] ?? '',
                ),
              ),
            ],
          ),
        ],
      ),
    ],
  ),
];

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

List<RouteBase> get _standaloneRoutes => [
  GoRoute(
    path: MRoutes.discoverSearch,
    builder: (_, _) => const DiscoverSearchPage(),
  ),
  // The suggested-accounts tab is not built yet: there is no endpoint that
  // suggests anybody, and profiles are found by searching. The address still
  // lands somewhere real rather than on a "not found" page.
  GoRoute(
    path: MRoutes.discoverSuggestedAccountsTab,
    redirect: (_, _) => MRoutes.discoverAllTab,
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
  GoRoute(
    path: MRoutes.liveSessionInit,
    builder: (_, _) => const LiveSessionInitPage(),
  ),
  GoRoute(
    path: '/notes/:noteId',
    builder: (_, state) {
      final raw = state.pathParameters['noteId'];
      final noteIdStr = (raw == null || raw == 'new') ? null : raw;
      return NoteEditorPage(noteIdStr: noteIdStr);
    },
  ),
  GoRoute(
    path: '/folders/:folderId',
    builder: (_, state) {
      final folderIdStr = state.pathParameters['folderId'] ?? '';
      return FolderPage(folderIdStr: folderIdStr);
    },
  ),
];

/// Builds the application's [GoRouter].
///
/// The router is assembled in one place rather than across many small route
/// files: at this size a single route table reads clearly and keeps the guard
/// and shell logic next to the routes they cover.
GoRouter createRouter({
  required OnboardingManager onboarding,
  required AccountTransitionOwner owner,
  required PendingDestinationSlot pending,
  bool debugLogDiagnostics = false,
}) {
  final guard = RouterGuard(
    onboarding: onboarding,
    owner: owner,
    pending: pending,
  );

  // Built once, as a local: a fresh merge per access would hand the router a
  // new listenable every time and re-register its listener (audit L02).
  // Refreshing on the *snapshot* — not on two booleans — is what makes an A→B
  // switch, a quiesce and a blockage all re-run the guard.
  final refresh = Listenable.merge([owner.snapshot, onboarding.isOnboarded]);

  return GoRouter(
    debugLogDiagnostics: debugLogDiagnostics,
    navigatorKey: rootNavigatorKey,
    initialLocation: MRoutes.home,
    refreshListenable: refresh,
    redirect: guard.redirect,
    routes: [
      ..._publicRoutes,
      ..._standaloneRoutes,
      ..._broadcastRoutes,
      ..._liveSessionRoutes,
      ..._settingsRoutes,
      ..._bottomNavShell,
    ],
  );
}

/// The `name:` of every route the app declares, in declaration order.
///
/// Public so a test can assert that each named navigation has a route behind it
/// without building the router — which needs a ready account, an onboarding
/// service and a router guard to say anything at all. The route table is the
/// authority; this reads it rather than restating it.
Iterable<String> declaredRouteNames() sync* {
  for (final route in [
    ..._publicRoutes,
    ..._standaloneRoutes,
    ..._broadcastRoutes,
    ..._liveSessionRoutes,
    ..._settingsRoutes,
    ..._bottomNavShell,
  ]) {
    yield* _declaredNamesIn(route);
  }
}

Iterable<String> _declaredNamesIn(RouteBase route) sync* {
  if (route is GoRoute && route.name != null) yield route.name!;
  for (final child in route.routes) {
    yield* _declaredNamesIn(child);
  }
}

/// The id [state]'s parameter [name] carries, or null when it is not a UUID.
///
/// A path parameter is untrusted input: a hand-edited link can say anything.
Id? _pathId(GoRouterState state, String name) {
  final raw = state.pathParameters[name];
  if (raw == null) return null;

  final id = Id.fromString(raw);
  return id.isValid ? id : null;
}
