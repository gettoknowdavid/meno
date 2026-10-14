import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

/// A service that is supposed to live exactly as long as one account.
final class _AccountService {
  const new(this.userId);

  final String userId;
}

UserCredential _account(String id, {String token = 'access-token'}) {
  return AuthResponseDto.fromJson({
    'access_token': token,
    'refresh_token': 'refresh-token',
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

  late AuthManager auth;
  late SessionStore store;

  setUp(() async {
    FlutterSecureStorage.setMockInitialValues({});
    await di.reset();
    di.pushNewScope(scopeName: Scope.root);

    store = SessionStore(SecureStorage(const FlutterSecureStorage()));
    auth = AuthManager(
      http: AuthHttpService(stubClient(StubApi())),
      store: store,
      logger: MockMenoLogger(),
    );
    await auth.init();
  });

  tearDown(di.reset);

  /// Starts a manager following [auth], with [register] filling the scope.
  Future<UserScopeManager> scopeAccounts({
    AccountScopeRegistrar? register,
  }) async {
    final manager = UserScopeManager(
      auth,
      logger: MockMenoLogger(),
      register: register,
    );
    await manager.init();
    return manager;
  }

  test('an account that becomes active is scoped', () async {
    await scopeAccounts();
    expect(di.currentScopeName, Scope.root);

    await store.saveActive(_account('1'));
    await pumpUntil(() => di.currentScopeName == Scope.authenticated);

    expect(di<UserCredential>().userId, '1');
    expect(di<UserCredential>().user.fullName, 'Account 1');
  });

  test('the registered services belong to the account and go with it',
      () async {
    await scopeAccounts(
      register: (scope, credential) =>
          scope.registerSingleton<_AccountService>(
            _AccountService(credential.userId),
          ),
    );

    await store.saveActive(_account('1'));
    await pumpUntil(() => di.isRegistered<_AccountService>());
    expect(di<_AccountService>().userId, '1');

    await store.clearActive();
    await pumpUntil(() => di.currentScopeName == Scope.root);

    expect(di.isRegistered<_AccountService>(), isFalse);
    expect(di.isRegistered<UserCredential>(), isFalse);
  });

  test('switching accounts replaces the credential in the scope', () async {
    await scopeAccounts();
    await store.saveActive(_account('1'));
    await pumpUntil(() => di.currentScopeName == Scope.authenticated);

    await store.saveActive(_account('2'));
    await pumpUntil(() => di<UserCredential>().userId == '2');

    expect(di.currentScopeName, Scope.authenticated);
    expect(di.hasScope(Scope.authenticated), isTrue);
  });

  test('a renewed token keeps the scope and its services', () async {
    var registered = 0;
    await scopeAccounts(
      register: (scope, credential) {
        registered++;
        scope.registerSingleton<_AccountService>(
          _AccountService(credential.userId),
        );
      },
    );

    await store.saveActive(_account('1', token: 'first'));
    await pumpUntil(() => di.currentScopeName == Scope.authenticated);
    final service = di<_AccountService>();

    await store.saveActive(_account('1', token: 'second'));
    await pumpUntil(
      () =>
          di.isRegistered<UserCredential>() &&
          di<UserCredential>().session.accessToken == 'second',
    );

    // The account did not change, so nothing was rebuilt — only the credential
    // the scope hands out was swapped.
    expect(registered, 1);
    expect(di<_AccountService>(), same(service));
  });

  test('two transitions in flight settle on the last account', () async {
    await scopeAccounts(
      register: (scope, credential) async {
        await Future<void>.delayed(const Duration(milliseconds: 50));
        scope.registerSingleton<_AccountService>(
          _AccountService(credential.userId),
        );
      },
    );

    unawaited(store.saveActive(_account('1')));
    await Future<void>.delayed(const Duration(milliseconds: 10));
    await store.saveActive(_account('2'));

    await pumpUntil(
      () =>
          di.currentScopeName == Scope.authenticated &&
          di.isRegistered<_AccountService>() &&
          di<_AccountService>().userId == '2',
      step: const Duration(milliseconds: 5),
    );

    expect(di<UserCredential>().userId, '2');
  });

  test('a registrar that fails scopes nothing, and the next account works',
      () async {
    var attempts = 0;
    await scopeAccounts(
      register: (scope, credential) {
        attempts++;
        if (attempts == 1) throw const NetworkException();
        scope.registerSingleton<_AccountService>(
          _AccountService(credential.userId),
        );
      },
    );

    await store.saveActive(_account('1'));
    await pumpUntil(() => attempts == 1);
    expect(di.currentScopeName, Scope.root);
    expect(di.hasScope(Scope.authenticated), isFalse);

    await store.saveActive(_account('2'));
    await pumpUntil(() => di.isRegistered<_AccountService>());
    expect(di<_AccountService>().userId, '2');
  });
}
