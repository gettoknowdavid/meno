import 'dart:convert';

import 'package:dio/dio.dart';
import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/pages/login_page.dart';
import 'package:meno_mobile/features/auth/services/auth_http_service.dart';
import 'package:meno_mobile/features/auth/services/session_service.dart';
import 'package:meno_mobile/features/broadcast/pages/home_page.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/locator.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/fake_clock.dart';
import '../../_support/fake_path_provider.dart';
import '../../_support/pump_until_found.dart';
import '../../_support/session_vault.dart';
import '../../_support/stub_api.dart';

const _jim = '018f2b1e-0000-7000-8000-000000000000';
const _pam = '018f2b1e-0000-7000-8000-000000000001';

class _MockPlatform extends Mock implements FlutterSecureStorage;

/// A dio adapter that answers by path, so the app's own requests do not depend
/// on the order a screen happens to make them in.
final class _RoutedApi implements HttpClientAdapter {
  new(this.answers);

  final Map<String, Future<ResponseBody> Function()> answers;
  final List<RequestOptions> requests = [];

  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) {
    requests.add(options);
    final answer = answers[options.path];
    return answer == null ? Future<ResponseBody>.value(envelope()) : answer();
  }

  @override
  void close({bool force = false}) {}
}

const _appConfig = Config(
  env: Environment.dev,
  menoApiUrl: 'https://dev.api',
  webSocketUrl: 'wss://dev.socket',
  menoLiveKitUrl: 'https://dev.livekit',
);

/// The session a sign-in answers with: signed in, address confirmed.
Map<String, dynamic> _authJson({String id = _jim, bool verified = true}) => {
  'access_token': 'access-token',
  'refresh_token': 'refresh-token',
  'expires_in': 3600,
  'user': {
    'id': id,
    'full_name': 'Jim Halpert',
    'email': 'jim@example.com',
    'verified': verified,
  },
};

/// The record `GET /users/me` answers with.
Map<String, dynamic> _profileJson() => {
  'id': _jim,
  'full_name': 'Jim Halpert',
  'email': 'jim@example.com',
  'email_verified': true,
};

/// Where this account's own cache entry lives.
const String _profileKey = 'profile.v2/$_jim/$_jim';

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

      // Seeding a session is not a reason to hide the address field: only an
      // explicit choice of a remembered identity asks for the password-only
      // form (Step 7/23).
      expect(manager.loginFormMode.value, LoginFormMode.fullEmail);

      await manager.addAccount.runAsync();

      expect(manager.loginFormMode.value, LoginFormMode.fullEmail);
      expect(manager.activeCredential.value.isEmpty, isTrue);
      expect(store.readSnapshot().credentials, hasLength(2));
    },
  );

  test('a sign-in leaves the form mode on the full form', () async {
    final manager = await createManager(
      api: StubApi([() async => envelope(data: _authJson())]),
    );

    await manager.login.runAsync(
      LoginArgs(
        email: Email('jim@example.com'),
        password: Password.login('password123'),
      ),
    );

    expect(manager.loginFormMode.value, LoginFormMode.fullEmail);
  });

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
    when(() => platform.delete(key: any(named: 'key')))
        .thenThrow(PlatformException(code: 'unavailable'));

    await expectLater(manager.signOut(), throwsA(isA<StorageException>()));

    expect(store.readSnapshot().credentials, isNotEmpty);
    expect(manager.activeCredential.value.isEmpty, isFalse);
  });

  // The tests below drive the real app: whether an account becomes usable, and
  // whether the screen it was signed in on leaves, is decided by the owner's
  // snapshot and the router's reaction to it — not by the sign-in screen.

  /// The API the app's own client talks to while a test drives the app.
  _RoutedApi routedApi() => _RoutedApi({
    '/auth/login': () async => envelope(data: _authJson()),
    '/auth/verify-email': () async => envelope(data: _authJson()),
    '/users/me': () async => envelope(data: _profileJson()),
  });

  /// The app's client over [api], with no interceptors.
  HttpClient routedClient(_RoutedApi api) => HttpClient(
    baseUrl: 'https://api.test',
    logger: MockMenoLogger(),
    dio: Dio()..httpClientAdapter = api,
  );

  /// Points the app's lazy transport at [api] before anything resolves it.
  Future<void> useApi(_RoutedApi api) async {
    await di.unregister<HttpClient>();
    di.registerSingleton<HttpClient>(routedClient(api));
  }

  /// Registers the app's dependencies and mounts it, the way `main()` does.
  Future<_RoutedApi> bootApp(
    WidgetTester tester, {
    Map<String, Object> prefs = const {},
  }) async {
    installFakePathProvider();
    FlutterSecureStorage.setMockInitialValues({});
    SharedPreferences.setMockInitialValues({
      StorageKeys.onboardingCompleted: true,
      ...prefs,
    });

    await di.reset();
    addTearDown(di.reset);

    final api = routedApi();
    configureDependencies(_appConfig);
    await useApi(api);
    configureGlobalExceptionHandler();
    await di.allReady();

    await tester.pumpWidget(const MenoApp());
    await tester.pumpAndSettle();
    return api;
  }

  testWidgets(
    'a sign-in publishes ready and the router leaves the login route',
    (tester) async {
      await bootApp(tester);
      expect(find.byType(LoginPage), findsOneWidget);

      await di<AuthManager>().login.runAsync(
        LoginArgs(
          email: Email('jim@example.com'),
          password: Password.login('password123'),
        ),
      );
      await tester.pumpAndSettle();

      expect(
        di<AccountTransitionOwner>().snapshot.value.phase,
        AccountPhase.ready,
      );
      expect(find.byType(LoginPage), findsNothing);
      expect(find.byType(HomePage), findsOneWidget);

      // The screen the sign-in happened on never decides where the app goes,
      // and it never hides the address field because a session now exists.
      expect(di<AuthManager>().loginFormMode.value, LoginFormMode.fullEmail);

      // `ready` means the account's own manager exists and has adopted its
      // account — the chrome has something to read before any request answers.
      expect(di<MyProfileManager>().profile.value.id.getOrCrash, _jim);

      await unmountTree(tester);
    },
  );

  testWidgets('the account restored at startup is initialized before ready', (
    tester,
  ) async {
    const cachedProfile = {
      'id': _jim,
      'full_name': 'Pam Beesly',
      'email': 'jim@example.com',
      'email_verified': true,
    };

    installFakePathProvider();
    FlutterSecureStorage.setMockInitialValues({});
    SharedPreferences.setMockInitialValues({
      StorageKeys.onboardingCompleted: true,
      // A cached record whose name the API would never answer with, so the
      // assertion below can only pass from the cache read.
      _profileKey: jsonEncode(cachedProfile),
    });

    await di.reset();
    addTearDown(di.reset);

    configureDependencies(_appConfig);
    await useApi(routedApi());
    configureGlobalExceptionHandler();
    await di.allReady();
    await seedVault(
      testRecord(accountId: _jim),
      remembered: testGreeting(accountId: _jim, displayName: 'Jim Halpert'),
    );

    // Read at the moment `ready` is published: nothing has had a chance to
    // happen between the registrar's initialize() and the snapshot.
    MyProfile? atReady;
    final owner = di<AccountTransitionOwner>();
    owner.snapshot.addListener(() {
      if (atReady == null && owner.snapshot.value.phase == AccountPhase.ready) {
        atReady = di<MyProfileManager>().profile.value;
      }
    });

    await tester.pumpWidget(const MenoApp());
    await tester.pumpAndSettle();

    expect(atReady, isNotNull);
    expect(atReady!.id.getOrCrash, _jim);
    expect(atReady!.fullName.orEmpty, 'Pam Beesly');
    expect(find.byType(HomePage), findsOneWidget);

    await unmountTree(tester);
  });

  testWidgets('a verified address keeps the account on its own manager', (
    tester,
  ) async {
    installFakePathProvider();
    FlutterSecureStorage.setMockInitialValues({});
    SharedPreferences.setMockInitialValues({
      StorageKeys.onboardingCompleted: true,
    });

    await di.reset();
    addTearDown(di.reset);

    configureDependencies(_appConfig);
    await useApi(routedApi());
    configureGlobalExceptionHandler();
    await di.allReady();
    await seedVault(testRecord(accountId: _jim, verified: false));

    await tester.pumpWidget(const MenoApp());
    await tester.pumpAndSettle();

    final profile = di<MyProfileManager>();
    expect(profile.profile.value.id.getOrCrash, _jim);

    await di<AuthManager>().verifyEmail.runAsync(
      VerifyEmailArgs(email: Email('jim@example.com'), code: '123456'),
    );
    await tester.pumpAndSettle();

    expect(
      di<AccountTransitionOwner>().snapshot.value.phase,
      AccountPhase.ready,
    );
    expect(find.text('Verify your email'), findsNothing);
    expect(di<MyProfileManager>(), same(profile));

    await unmountTree(tester);
  });
}
