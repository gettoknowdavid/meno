import 'dart:async';
import 'dart:convert';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

class MockMenoLogger extends Mock implements MenoLogger;

/// A service that is supposed to live exactly as long as one broadcast session.
final class _LiveService {
  const new(this.broadcastId);

  final String broadcastId;
}

/// A broadcast that is on air, as the API would describe one.
Broadcast _broadcast({bool live = true}) => Broadcast.empty.copyWith(
  id: 'b1',
  state: live ? BroadcastState.live : BroadcastState.ended,
);

/// The session joining or going live would have produced.
BroadcastSession _session({bool live = true, DateTime? at}) => BroadcastSession(
  userId: 'user-1',
  broadcast: _broadcast(live: live),
  token: 'room-token',
  timestamp: at,
);

/// The stored record for [_session], as the app would have left it [ago] ago.
String _record({required Duration ago}) => jsonEncode({
  'saved_at': DateTime.now().toUtc().subtract(ago).toIso8601String(),
  'session': BroadcastSessionDto(
    broadcast: _broadcast().toDto,
    token: 'room-token',
  ).toJson(),
});

String get _liveSessionKey =>
    StorageKeys.forUser(StorageKeys.liveSession, 'user-1');

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late LocalStorage storage;
  late LiveSessionStore store;
  late StreamController<EndedBroadcast> ended;
  late StubApi api;

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    await di.reset();
    di.pushNewScope(scopeName: Scope.root);

    storage = LocalStorage(await SharedPreferences.getInstance());
    store = LiveSessionStore(storage, userId: 'user-1');
    ended = StreamController<EndedBroadcast>.broadcast();
    api = StubApi();
  });

  tearDown(() async {
    await ended.close();
    await di.reset();
  });

  /// A manager with nothing stored and an API that answers nothing useful.
  LiveScopeManager newManager({
    LiveScopeRegistrar? register,
    LiveSessionStore? liveStore,
  }) {
    return LiveScopeManager(
      'user-1',
      logger: MockMenoLogger(),
      store: liveStore ?? store,
      http: BroadcastHttpService(stubClient(api)),
      endedBroadcasts: ended.stream,
      register: register,
    );
  }

  test('starting a session opens a scope named for the broadcast', () async {
    final live = newManager();

    await live.start('b1');

    expect(di.currentScopeName, Scope.liveFor('b1'));
    expect(di<LiveSession>().broadcastId, 'b1');
    expect(di<LiveSession>().userId, 'user-1');
    expect(live.activeBroadcastId.value, 'b1');
    expect(live.isLive.value, isTrue);
  });

  test('the registered services go when the session ends', () async {
    var registered = 0;
    final live = newManager(
      register: (scope, session) {
        registered++;
        scope.registerSingleton<_LiveService>(
          _LiveService(session.broadcastId),
        );
      },
    );

    await live.start('b1');
    expect(di<_LiveService>().broadcastId, 'b1');

    await live.end();

    expect(registered, 1);
    expect(di.currentScopeName, Scope.root);
    expect(di.isRegistered<_LiveService>(), isFalse);
    expect(di.isRegistered<LiveSession>(), isFalse);
    expect(live.isLive.value, isFalse);
  });

  test('starting again for the same broadcast keeps its session', () async {
    var registered = 0;
    final live = newManager(
      register: (scope, session) {
        registered++;
        scope.registerSingleton<_LiveService>(
          _LiveService(session.broadcastId),
        );
      },
    );

    await live.start('b1');
    final service = di<_LiveService>();
    await live.start('b1');

    expect(registered, 1);
    expect(di<_LiveService>(), same(service));
    expect(di.currentScopeName, Scope.liveFor('b1'));
  });

  test('a second broadcast replaces the first', () async {
    final live = newManager();

    await live.start('b1');
    await live.start('b2');

    expect(di.currentScopeName, Scope.liveFor('b2'));
    expect(di.hasScope(Scope.liveFor('b1')), isFalse);
    expect(di<LiveSession>().broadcastId, 'b2');
  });

  test('an end for another broadcast leaves the session standing', () async {
    final live = newManager();

    await live.start('b1');
    // A late `endedBroadcast` for the session that has already been replaced.
    await live.end('b0');

    expect(di.currentScopeName, Scope.liveFor('b1'));
    expect(live.isLive.value, isTrue);

    await live.end('b1');
    expect(live.isLive.value, isFalse);
  });

  test('two starts in flight settle on the last broadcast', () async {
    final live = newManager();

    unawaited(live.start('b1'));
    unawaited(live.start('b2'));

    await pumpUntil(() => live.activeBroadcastId.value == 'b2');

    expect(di.currentScopeName, Scope.liveFor('b2'));
    expect(di.hasScope(Scope.liveFor('b1')), isFalse);
  });

  test('a registrar that fails leaves no session, and starting again works',
      () async {
    var attempts = 0;
    final live = newManager(
      register: (scope, session) {
        attempts++;
        if (attempts == 1) throw const NetworkException();
        scope.registerSingleton<_LiveService>(
          _LiveService(session.broadcastId),
        );
      },
    );

    await live.start('b1');
    expect(di.currentScopeName, Scope.root);
    expect(live.isLive.value, isFalse);

    await live.start('b1');
    expect(di.currentScopeName, Scope.liveFor('b1'));
  });

  test('a sign-out ends the session with the account', () async {
    await di.pushNewScopeAsync(
      scopeName: Scope.authenticated,
      init: (scope) async {
        scope.registerSingleton<LiveScopeManager>(newManager());
      },
    );

    final live = di<LiveScopeManager>();
    await live.start('b1');
    expect(di.currentScopeName, Scope.liveFor('b1'));

    await di.popScopesTill(Scope.root, inclusive: false);

    expect(di.currentScopeName, Scope.root);
    expect(di.hasScope(Scope.liveFor('b1')), isFalse);
    expect(di.isRegistered<LiveScopeManager>(), isFalse);
  });

  test('a session is remembered while it is open', () async {
    final live = newManager();

    await live.start('b1', session: _session());
    expect(store.read()?.broadcast.id, 'b1');

    await live.end();
    expect(store.read(), isNull);
  });

  test('a room opened without a session is not remembered', () async {
    final live = newManager();

    // A deep link: a room that can be listed but not entered.
    await live.start('b1');

    expect(di.currentScopeName, Scope.liveFor('b1'));
    expect(store.read(), isNull);
  });

  test('the session this device was in is recovered on the next start',
      () async {
    await store.save(_session());
    api.answers.add(
      () async => envelope(data: _broadcast().toDto.toJson()),
    );

    final live = newManager();
    await live.init();
    await pumpUntil(() => live.activeBroadcastId.value == 'b1');

    expect(di.currentScopeName, Scope.liveFor('b1'));
    expect(di<LiveSession>().broadcastId, 'b1');
    expect(di<BroadcastSession>().token, 'room-token');
  });

  test('a session whose broadcast has ended is dropped, not recovered',
      () async {
    await store.save(_session());
    api.answers.add(
      () async => envelope(data: _broadcast(live: false).toDto.toJson()),
    );

    final live = newManager();
    await live.init();
    await pumpUntil(() => store.read() == null);

    expect(di.currentScopeName, Scope.root);
    expect(live.isLive.value, isFalse);
  });

  test('an aged-out session is dropped without asking the API', () async {
    // Written thirteen hours ago: a session that was still open when the app
    // stopped last night is not worth going back to today.
    await storage.setString(
      _liveSessionKey,
      value: _record(ago: const Duration(hours: 13)),
    );

    final live = newManager();
    await live.init();
    await pumpUntil(() => store.read() == null);

    expect(api.requests, isEmpty);
    expect(di.currentScopeName, Scope.root);
  });

  test('a broadcast ending on the socket closes its session', () async {
    final live = newManager();
    await live.init();
    await live.start('b1', session: _session());

    ended.add(
      const EndedBroadcast(broadcastId: 'b1', title: 'A broadcast'),
    );
    await pumpUntil(() => !live.isLive.value);

    expect(di.currentScopeName, Scope.root);
    expect(di.isRegistered<LiveSession>(), isFalse);
  });
}
