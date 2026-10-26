import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/auth_http_service.dart';
import 'package:mocktail/mocktail.dart';

import '../../_support/stub_api.dart';

const _jim = '018f2b1e-0000-7000-8000-000000000000';
const _pam = '018f2b1e-0000-7000-8000-000000000001';

class _MockPlatform extends Mock implements FlutterSecureStorage;

UserCredential _account({String id = _jim, String accessToken = 'access'}) {
  return AuthResponseDto.fromJson({
    'access_token': accessToken,
    'refresh_token': 'refresh-$accessToken',
    'expires_in': 3600,
    'user': {
      'id': id,
      'full_name': 'Jim Halpert',
      'email': 'jim@example.com',
      'verified': true,
    },
  }).toDomain;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late _MockPlatform platform;
  late Map<String, String> disk;
  late SessionStore store;

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
  });

  Future<AuthManager> createManager({
    List<UserCredential> seeded = const [],
    StubApi? api,
  }) async {
    store = SessionStore(SecureStorage(platform));
    addTearDown(store.onDispose);

    for (final credential in seeded) {
      await store.saveActive(credential);
    }

    final manager = await AuthManager(
      http: AuthHttpService(stubClient(api ?? StubApi())),
      store: store,
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

      expect(await store.readAccounts(), isEmpty);
      expect(await store.readActiveCredential(), isNull);
      expect(manager.activeCredential.value.isEmpty, isTrue);
    },
  );

  test('a late renewal cannot recreate a logged-out account', () async {
    final manager = await createManager(seeded: [_account()]);

    await manager.signOut();
    expect(await store.readAccounts(), isEmpty);

    await store.saveToVault(_account(accessToken: 'renewed'));

    expect(await store.readAccounts(), isEmpty);
    expect(await store.readActiveCredential(), isNull);
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
      expect((await store.readAccounts()).length, 2);
    },
  );

  test('a failed removal is reported and keeps the credentials', () async {
    final manager = await createManager(seeded: [_account()]);
    final before = await store.readAccounts();
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

    expect(await store.readAccounts(), isNotEmpty);
    expect(manager.activeCredential.value.isEmpty, isFalse);
  });
}
