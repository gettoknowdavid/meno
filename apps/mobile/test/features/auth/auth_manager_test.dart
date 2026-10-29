import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/auth_http_service.dart';
import 'package:meno_mobile/features/auth/services/session_service.dart';
import 'package:mocktail/mocktail.dart';

import '../../_support/fake_clock.dart';
import '../../_support/session_vault.dart';
import '../../_support/stub_api.dart';

const _jim = '018f2b1e-0000-7000-8000-000000000000';
const _pam = '018f2b1e-0000-7000-8000-000000000001';

class _MockPlatform extends Mock implements FlutterSecureStorage;

CredentialRecord _account({String id = _jim, String accessToken = 'access'}) =>
    testRecord(
      accountId: id,
      accessToken: accessToken,
      refreshToken: 'refresh-$accessToken',
    );

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late _MockPlatform platform;
  late Map<String, String> disk;
  late FakeClock clock;
  late SessionService store;

  void stubStorage() {
    when(() => platform.read(key: any(named: 'key')))
        .thenAnswer((invocation) async {
          return disk[invocation.namedArguments[#key]! as String];
        });
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
  }

  setUp(() {
    disk = {};
    platform = _MockPlatform();
    stubStorage();
    clock = FakeClock(DateTime.utc(2026));
  });

  Future<AuthManager> createManager({
    List<CredentialRecord> seeded = const [],
    StubApi? api,
  }) async {
    store = SessionService(SecureStorage(platform));
    addTearDown(store.onDispose);
    await store.initialize();

    for (final record in seeded) {
      await store.adoptCandidate(record);
      await store.activate(record.accountId);
    }

    final manager = await AuthManager(
      http: AuthHttpService(stubClient(api ?? StubApi())),
      store: store,
      clock: clock,
      logger: MockMenoLogger(),
    ).init();
    addTearDown(manager.onDispose);
    return manager;
  }

  test(
    'logout removes the credentials even when the server call fails',
    () async {
      final manager = await createManager(
        seeded: [_account()],
        api: StubApi([() async => envelope(status: 500)]),
      );

      await manager.logout.runAsync();

      expect(store.readSnapshot().credentials, isEmpty);
      expect(store.readSnapshot().activeAccountId, isNull);
      expect(manager.activeCredential.value.isEmpty, isTrue);
    },
  );

  test('a late renewal cannot recreate a logged-out account', () async {
    final manager = await createManager(seeded: [_account()]);

    await manager.signOut();
    expect(store.readSnapshot().credentials, isEmpty);

    // The renewal that was in flight settles for the epoch that just went away.
    expect(
      await store.conditionalReplace(
        Id.fromWire(_jim),
        epoch: 'epoch-test',
        fromRevision: 1,
        accessToken: 'renewed',
        accessExpiresAt: clock.nowUtc().add(const Duration(hours: 1)),
      ),
      isA<MutationStale>(),
    );

    expect(store.readSnapshot().credentials, isEmpty);
    expect(store.readSnapshot().activeAccountId, isNull);
  });

  test(
    'add-account switches to the full form and keeps the other accounts',
    () async {
      final manager = await createManager(
        seeded: [
          _account(),
          _account(id: _pam),
        ],
      );

      expect(
        manager.loginFormMode.value,
        LoginFormMode.prefillSelectedIdentity,
      );

      await manager.addAccount.runAsync();

      expect(manager.loginFormMode.value, LoginFormMode.fullEmail);
      expect(manager.activeCredential.value.isEmpty, isTrue);
      expect(store.readSnapshot().credentials, hasLength(2));
    },
  );

  test('a failed removal is reported and keeps the credentials', () async {
    final manager = await createManager(seeded: [_account()]);
    final before = store.readSnapshot().credentials;
    expect(before, isNotEmpty);

    when(
      () => platform.write(
        key: any(named: 'key'),
        value: any(named: 'value'),
      ),
    ).thenThrow(PlatformException(code: 'unavailable'));
    when(
      () => platform.delete(key: any(named: 'key')),
    ).thenThrow(PlatformException(code: 'unavailable'));

    await expectLater(manager.signOut(), throwsA(isA<StorageException>()));

    expect(store.readSnapshot().credentials, isNotEmpty);
    expect(manager.activeCredential.value.isEmpty, isFalse);
  });
}
