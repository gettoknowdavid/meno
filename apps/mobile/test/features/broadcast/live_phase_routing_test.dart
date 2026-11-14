import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/features/auth/model/auth_context.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/broadcast/pages/live_session_init_page.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/stub_api.dart';

const _account = '018f2b1e-0000-7000-8000-000000000020';
const _current = '018f2b1e-0000-7000-8000-000000000021';
const _other = '018f2b1e-0000-7000-8000-000000000022';

class _StubGateway implements LiveGateway {
  int enterCalls = 0;
  int leaveCalls = 0;
  int endCalls = 0;
  bool failEnter = false;

  @override
  Future<BroadcastSession> enter(
    LiveEnterRequest request,
    AccountContext context,
  ) async {
    enterCalls++;
    if (failEnter) throw const NotFoundException(message: 'gone', code: 'X');
    throw StateError('this test does not enter a real room');
  }

  @override
  Future<EndedBroadcast> end(Id broadcastId) async {
    endCalls++;
    return EndedBroadcast(
      broadcastId: broadcastId,
      title: SingleLineString('Morning show'),
    );
  }

  @override
  Future<void> leave(Id broadcastId) async {
    leaveCalls++;
  }
}

Broadcast _broadcast(String id) => Broadcast(
  id: Id.fromString(id),
  title: SingleLineString('Morning show'),
  state: .live,
  status: .active,
  participantRole: .host,
  creator: Participant(
    id: Id.fromString(_account),
    fullName: SingleLineString('Ada'),
  ),
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late _StubGateway gateway;
  late ActiveBroadcastManager manager;
  late GoRouter router;

  setUp(() async {
    Command.globalExceptionHandler = (_, _) {};
    SharedPreferences.setMockInitialValues({});
    await di.reset();
    di.pushNewScope(scopeName: Scope.root);
    di.registerSingleton<MenoLogger>(MockMenoLogger());

    gateway = _StubGateway();
    manager = ActiveBroadcastManager(
      context: AccountContext(
        accountId: Id.fromString(_account),
        generation: 1,
      ),
      gateway: gateway,
      store: LiveSessionStore(
        LocalStorage(await SharedPreferences.getInstance()),
        userId: Id.fromString(_account),
      ),
      logger: di<MenoLogger>(),
      createSession: ({required session, required exitOnServer}) =>
          throw StateError('no session is built in this test'),
    );
    di.registerSingleton<ActiveBroadcastManager>(manager);
  });

  tearDown(() async {
    Command.globalExceptionHandler = null;
    await di.reset();
  });

  /// A router with one text screen per destination the page can hand off to.
  void buildRouter() {
    router = GoRouter(
      initialLocation: MRoutes.liveSessionInit,
      routes: [
        GoRoute(
          path: MRoutes.liveSessionInit,
          builder: (_, _) => const LiveSessionInitPage(),
        ),
        GoRoute(
          path: MRoutes.liveBroadcast,
          builder: (_, _) => const Scaffold(body: Text('room')),
        ),
        GoRoute(
          path: MRoutes.preStreamPattern,
          builder: (_, _) => const Scaffold(body: Text('lobby')),
        ),
        GoRoute(
          path: '/broadcasts/:broadcastId/ended',
          builder: (_, _) => const Scaffold(body: Text('ended')),
        ),
        GoRoute(
          path: MRoutes.home,
          builder: (_, _) => const Scaffold(body: Text('home')),
        ),
      ],
    );
  }

  Future<void> pumpInit(WidgetTester tester) async {
    buildRouter();
    await tester.pumpWidget(
      MaterialApp.router(theme: MTheme.dark, routerConfig: router),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
  }

  group('the transition page follows the phase', () {
    testWidgets('connecting shows progress and stays put', (tester) async {
      manager.snapshot.value = LiveSnapshot(
        phase: LivePhase.connecting,
        broadcast: _broadcast(_current),
      );

      await pumpInit(tester);

      expect(find.text('Connecting to the broadcast…'), findsOne);
      expect(router.state.uri.toString(), MRoutes.liveSessionInit);
    });

    testWidgets('reconnecting says so', (tester) async {
      manager.snapshot.value = LiveSnapshot(
        phase: LivePhase.reconnecting,
        broadcast: _broadcast(_current),
      );

      await pumpInit(tester);

      expect(find.text('Reconnecting to the broadcast…'), findsOne);
    });

    testWidgets('live hands off to the room exactly once', (tester) async {
      manager.snapshot.value = LiveSnapshot.room(
        phase: LivePhase.live,
        broadcast: _broadcast(_current),
        accountId: Id.fromString(_account),
        canPublish: true,
      );

      await pumpInit(tester);
      await tester.pump(const Duration(milliseconds: 50));

      expect(find.text('room'), findsOne);
      expect(router.state.uri.toString(), MRoutes.liveBroadcast);
    });

    testWidgets('ending shows progress with the controls gone', (tester) async {
      manager.snapshot.value = LiveSnapshot(
        phase: LivePhase.ending,
        broadcast: _broadcast(_current),
      );

      await pumpInit(tester);

      expect(find.text('Ending the broadcast…'), findsOne);
      expect(find.byType(MLoadingIndicator), findsOne);
    });

    testWidgets('failed shows the typed message and a retry', (tester) async {
      manager.snapshot.value = const LiveSnapshot(
        phase: LivePhase.failed,
        failure: LiveFailure(message: 'gone', retryable: true),
      );

      await pumpInit(tester);

      expect(find.text('gone'), findsOne);
      expect(find.text('Retry'), findsOne);
    });

    test('retry re-runs the last entry', () async {
      gateway.failEnter = true;
      await manager.enter(
        LiveEnterRequest(
          broadcastId: Id.fromString(_current),
          mode: LiveEnterMode.listen,
        ),
      );
      expect(gateway.enterCalls, 1);

      await manager.retry();

      expect(gateway.enterCalls, 2, reason: 'the last request is reused');
      expect(manager.snapshot.value.phase, LivePhase.failed);
    });

    testWidgets('ended goes to the ended screen with the retained id', (
      tester,
    ) async {
      final ended = _broadcast(_current);
      manager.snapshot.value = LiveSnapshot(
        phase: LivePhase.ended,
        endedSummary: ended,
      );

      await pumpInit(tester);
      await tester.pump(const Duration(milliseconds: 50));

      expect(find.text('ended'), findsOne);
      expect(router.state.uri.path, contains(_current));
    });

    testWidgets('idle leaves the flow', (tester) async {
      manager.snapshot.value = LiveSnapshot.idle();

      await pumpInit(tester);
      await tester.pump(const Duration(milliseconds: 50));

      expect(find.text('home'), findsOne);
    });
  });

  group('the pre-stream lobby decides from the phase', () {
    testWidgets('the same live broadcast returns to the room', (tester) async {
      manager.snapshot.value = LiveSnapshot.room(
        phase: .live,
        broadcast: _broadcast(_current),
        accountId: Id.fromString(_account),
        canPublish: true,
      );

      buildRouter();
      await tester.pumpWidget(
        MaterialApp.router(
          theme: MTheme.dark,
          routerConfig: router = GoRouter(
            initialLocation: MRoutes.home,
            routes: [
              GoRoute(
                path: MRoutes.home,
                builder: (context, _) => Scaffold(
                  body: TextButton(
                    onPressed: () =>
                        PreStreamModal.show(context, Id.fromString(_current)),
                    child: const Text('open'),
                  ),
                ),
              ),
              GoRoute(
                path: MRoutes.preStreamPattern,
                builder: (_, _) => const Scaffold(body: Text('lobby')),
              ),
              GoRoute(
                path: MRoutes.liveBroadcast,
                builder: (_, _) => const Scaffold(body: Text('room')),
              ),
            ],
          ),
        ),
      );

      await tester.tap(find.text('open'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));

      expect(find.text('room'), findsOne);
      expect(gateway.enterCalls, 0);
    });

    testWidgets('another live broadcast asks before replacing it', (
      tester,
    ) async {
      manager.snapshot.value = LiveSnapshot.room(
        phase: .live,
        broadcast: _broadcast(_current),
        accountId: Id.fromString(_account),
        canPublish: true,
      );

      await tester.pumpWidget(
        MaterialApp.router(
          theme: MTheme.dark,
          routerConfig: router = GoRouter(
            initialLocation: MRoutes.home,
            routes: [
              GoRoute(
                path: MRoutes.home,
                builder: (context, _) => Scaffold(
                  body: TextButton(
                    onPressed: () =>
                        PreStreamModal.show(context, Id.fromString(_other)),
                    child: const Text('open'),
                  ),
                ),
              ),
              GoRoute(
                path: MRoutes.preStreamPattern,
                builder: (_, _) => const Scaffold(body: Text('lobby')),
              ),
            ],
          ),
        ),
      );

      await tester.tap(find.text('open'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));

      expect(find.text('You are already in a broadcast'), findsOne);
      expect(gateway.enterCalls, 0, reason: 'never a second entry');
      expect(
        router.state.uri.path,
        MRoutes.home,
        reason: 'nothing opens until the person confirms',
      );
    });
  });
}
