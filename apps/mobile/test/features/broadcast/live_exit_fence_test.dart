import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/_shared/services/secure_storage.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/auth_http_service.dart';
import 'package:meno_mobile/features/auth/services/session_store.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

const _jim = '018f2b1e-0000-7000-8000-000000000000';
const _pam = '018f2b1e-0000-7000-8000-000000000001';
const _broadcast = '018f2b1e-0000-7000-8000-000000000002';

class _MockPlatform extends Mock implements FlutterSecureStorage;

/// A service that belongs to one session, and records that it was torn down.
final class _LiveService implements Disposable {
  bool disposed = false;

  @override
  FutureOr<dynamic> onDispose() => disposed = true;
}

UserCredential _account(String id) {
  return AuthResponseDto.fromJson({
    'access_token': 'access',
    'refresh_token': 'refresh',
    'expires_in': 3600,
    'user': {
      'id': id,
      'full_name': 'Account $id',
      'email': '$id@example.com',
      'verified': true,
    },
  }).toDomain;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late LocalStorage storage;
  late LiveSessionStore liveStore;
  late StreamController<EndedBroadcast> ended;
  late StubApi api;

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    await di.reset();
    di.pushNewScope(scopeName: Scope.root);

    storage = LocalStorage(await SharedPreferences.getInstance());
    liveStore = LiveSessionStore(storage, userId: Id.fromString(_jim));
    ended = StreamController<EndedBroadcast>.broadcast();
    api = StubApi();
  });

  tearDown(() async {
    await ended.close();
    await di.reset();
  });

  LiveScopeManager newLiveManager({
    LiveScopeRegistrar? register,
    Duration drain = const Duration(seconds: 10),
  }) {
    return LiveScopeManager(
      Id.fromString(_jim),
      logger: MockMenoLogger(),
      store: liveStore,
      http: BroadcastHttpService(stubClient(api)),
      endedBroadcasts: ended.stream,
      register:
          register ??
          (scope, _) => scope.registerSingleton<_LiveService>(_LiveService()),
      exitDrainTimeout: drain,
    );
  }

  group('the live fence', () {
    test(
      'an exit during an open closes locally and pushes nothing else',
      () async {
        final gate = Completer<void>();
        var built = false;
        final live = newLiveManager(
          register: (scope, _) async {
            built = true;
            scope.registerSingleton<_LiveService>(_LiveService());
            await gate.future;
          },
        );

        final opening = live.start(Id.fromString(_broadcast));
        await pumpUntil(() => built);

        final exiting = live.beginAccountExit();
        gate.complete();
        await exiting;
        await opening;

        expect(di.hasScope(Scope.liveFor(Id.fromString(_broadcast))), isFalse);
        expect(di.isRegistered<LiveSession>(), isFalse);
        expect(di.isRegistered<_LiveService>(), isFalse);
        expect(live.isLive.value, isFalse);
      },
    );

    test('no open is admitted once the exit has begun', () async {
      final live = newLiveManager();
      await live.beginAccountExit();

      await live.start(Id.fromString(_broadcast));

      expect(di.hasScope(Scope.liveFor(Id.fromString(_broadcast))), isFalse);
      expect(di.isRegistered<LiveSession>(), isFalse);
      expect(live.activeBroadcastId.value, isNull);
    });

    test('a drain that cannot finish throws instead of completing', () async {
      final gate = Completer<void>();
      var built = false;
      final live = newLiveManager(
        drain: const Duration(milliseconds: 20),
        register: (scope, _) async {
          built = true;
          scope.registerSingleton<_LiveService>(_LiveService());
          await gate.future;
        },
      );

      unawaited(live.start(Id.fromString(_broadcast)));
      await pumpUntil(() => built);

      // The transition must not complete while an open is still in flight:
      // that is what keeps two accounts from overlapping.
      await expectLater(
        live.beginAccountExit(),
        throwsA(isA<RequestTimeoutException>()),
      );

      gate.complete();
      await pumpEventQueue();
    });

    test('a failed drain leaves the old account scoped', () async {
      final platform = _MockPlatform();
      final disk = <String, String>{};
      when(() => platform.read(key: any(named: 'key'))).thenAnswer(
        (invocation) async => disk[invocation.namedArguments[#key]! as String],
      );
      when(
        () => platform.write(
          key: any(named: 'key'),
          value: any(named: 'value'),
        ),
      ).thenAnswer((invocation) async {
        disk[invocation.namedArguments[#key]! as String] =
            invocation.namedArguments[#value]! as String;
      });
      when(() => platform.delete(key: any(named: 'key')))
          .thenAnswer((invocation) async {
            disk.remove(invocation.namedArguments[#key]);
          });

      final store = SessionStore(SecureStorage(platform));
      addTearDown(store.onDispose);
      final auth = await AuthManager(
        http: AuthHttpService(stubClient(api)),
        store: store,
        logger: MockMenoLogger(),
      ).init();
      addTearDown(auth.onDispose);

      final transition = await AccountTransitionOwner(
        activeCredential: auth.activeCredential,
        logger: MockMenoLogger(),
        beginLiveExit: () async => throw const RequestTimeoutException(),
      ).init();
      addTearDown(transition.onDispose);

      await store.saveActive(_account(_jim));
      await pumpUntil(() => di.isRegistered<UserCredential>());

      await store.saveActive(_account(_pam));
      await pumpUntil(
        () => auth.activeCredential.value.userId == Id.fromString(_pam),
      );
      await transition.ready;

      // The new account is not scoped: the transition is blocked until the
      // live tier can be closed, and the old account keeps what it owns.
      expect(di.currentScopeName, Scope.authenticated);
      expect(di<UserCredential>().userId, Id.fromString(_jim));
      expect(di<UserCredential>().userId, isNot(Id.fromString(_pam)));
    });
  });
}
