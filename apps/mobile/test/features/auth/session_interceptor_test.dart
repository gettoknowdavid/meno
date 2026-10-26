import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/session_interceptor.dart';
import 'package:mocktail/mocktail.dart';

import '../../_support/fake_clock.dart';
import '../../_support/stub_api.dart';

const _jim = '018f2b1e-0000-7000-8000-000000000000';
const _pam = '018f2b1e-0000-7000-8000-000000000001';

class _MockPlatform extends Mock implements FlutterSecureStorage;

/// The API base every request in this suite is dispatched against.
const _baseUrl = 'https://api.test';

/// A credential for [id], with its expiry pinned so the fake clock can move it.
UserCredential _account({
  required String id,
  required DateTime expiresAt,
  String accessToken = 'access',
  String? refreshToken = 'refresh',
}) {
  final base = AuthResponseDto.fromJson({
    'access_token': accessToken,
    'refresh_token': refreshToken ?? 'refresh-$accessToken',
    'expires_in': 3600,
    'user': {
      'id': id,
      'full_name': 'Someone',
      'email': 'someone@example.com',
      'verified': true,
    },
  }).toDomain;

  return UserCredential(
    user: base.user,
    session: Session(
      accessToken: accessToken,
      refreshToken: refreshToken == null ? null : 'refresh-$accessToken',
      expiresAt: expiresAt,
    ),
  );
}

/// The bearer header a request carried, read case-insensitively so the map's
/// own casing never decides the answer.
String? _bearerOf(RequestOptions request) {
  for (final entry in request.headers.entries) {
    if (entry.key.toLowerCase() == 'authorization') {
      return entry.value?.toString();
    }
  }
  return null;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late _MockPlatform platform;
  late Map<String, String> disk;
  late SessionStore store;
  late StubApi api;
  late FakeClock clock;
  late Config config;
  late HttpClient client;

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
    when(() => platform.deleteAll()).thenAnswer((_) async {
      disk.clear();
    });
  }

  setUp(() {
    disk = {};
    platform = _MockPlatform();
    stubStorage();

    clock = FakeClock(DateTime.utc(2026, 1, 1));
    store = SessionStore(SecureStorage(platform));
    addTearDown(store.onDispose);

    config = const Config(
      env: Environment.dev,
      menoApiUrl: _baseUrl,
      webSocketUrl: 'wss://api.test/ws',
      menoLiveKitUrl: 'wss://api.test/livekit',
    );

    api = StubApi();
    client = HttpClient(
      baseUrl: _baseUrl,
      logger: MockMenoLogger(),
      dio: Dio()
        ..httpClientAdapter = api
        ..options.followRedirects = false,
      interceptors: [
        SessionInterceptor(
          store: store,
          config: config,
          clock: clock,
          logger: MockMenoLogger(),
          refreshDio: Dio(BaseOptions(baseUrl: _baseUrl))
            ..httpClientAdapter = api,
        ),
      ],
    );
    addTearDown(() => client.onDispose());
  });

  Future<T> expectFailure<T extends MenoException>(
    Future<Object?> request,
  ) async {
    final error = await request.then<Object?>(
      (_) => fail('The request was expected to fail.'),
      onError: (Object e) => e,
    );
    expect(error, isA<T>());
    return error! as T;
  }

  test('a request captured as A is cancelled, not replayed as B', () async {
    await store.saveActive(
      _account(
        id: _jim,
        expiresAt: clock.nowUtc().add(const Duration(hours: 1)),
      ),
    );

    final reachedApi = Completer<void>();
    final held = Completer<ResponseBody>();
    api.answers.add(() {
      reachedApi.complete();
      return held.future;
    });

    final request = client.get<dynamic>('/users/me', fromJson: (data) => data);

    await reachedApi.future;

    // The user switches accounts while the request is in the air.
    await store.saveActive(
      _account(
        id: _pam,
        expiresAt: clock.nowUtc().add(const Duration(hours: 1)),
      ),
    );
    held.complete(
      ResponseBody.fromString(
        '{"code":"UNAUTHORIZED","message":"","status":false}',
        401,
        headers: {
          Headers.contentTypeHeader: [Headers.jsonContentType],
        },
      ),
    );

    await expectFailure<CancelledException>(request);

    // No B credential was ever attached and no renewal or replay happened.
    expect(api.requests, hasLength(1));
    expect(_bearerOf(api.requests.single), 'Bearer access');
  });

  test(
    'an account request to a foreign origin is refused before sending',
    () async {
      await store.saveActive(
        _account(
          id: _jim,
          expiresAt: clock.nowUtc().add(const Duration(hours: 1)),
        ),
      );

      await expectFailure<WrongOriginException>(
        client.get<dynamic>(
          'https://example.com/file',
          fromJson: (data) => data,
        ),
      );
      expect(api.requests, isEmpty);
    },
  );

  test('an anonymous request carries no Authorization header', () async {
    await store.saveActive(
      _account(
        id: _jim,
        expiresAt: clock.nowUtc().add(const Duration(hours: 1)),
      ),
    );

    await client.get<dynamic>(
      '/public',
      authMode: AuthMode.anonymous,
      headers: {SessionInterceptor.authorizationHeader: 'Bearer handed-in'},
      fromJson: (data) => data,
    );

    expect(api.requests, hasLength(1));
    expect(_bearerOf(api.requests.single), isNull);
  });

  test(
    'an expired, unrenewable session is refused, never sent anonymously',
    () async {
      await store.saveActive(
        _account(
          id: _jim,
          refreshToken: null,
          expiresAt: clock.nowUtc().add(const Duration(hours: 1)),
        ),
      );
      clock.advance(const Duration(hours: 3));

      await expectFailure<SessionUnavailableException>(
        client.get<dynamic>('/users/me', fromJson: (data) => data),
      );
      expect(api.requests, isEmpty);
    },
  );

  test('a redirect is not followed and forwards nothing', () async {
    await store.saveActive(
      _account(
        id: _jim,
        expiresAt: clock.nowUtc().add(const Duration(hours: 1)),
      ),
    );

    api.answers.add(
      () async => ResponseBody.fromString(
        '',
        302,
        headers: {
          Headers.contentTypeHeader: [Headers.jsonContentType],
          'location': ['https://evil.test/steal'],
        },
      ),
    );

    await expectFailure<RedirectNotAllowedException>(
      client.get<dynamic>('/users/me', fromJson: (data) => data),
    );
    expect(api.requests, hasLength(1));
  });

  test('a bearer request sends exactly the captured token, once', () async {
    await store.saveActive(
      _account(
        id: _jim,
        expiresAt: clock.nowUtc().add(const Duration(hours: 1)),
      ),
    );

    await client.postUnit(
      '/auth/logout',
      data: const {'refresh_token': 'refresh'},
      authMode: const BearerAuth('captured-token'),
    );

    expect(_bearerOf(api.requests.single), 'Bearer captured-token');
  });
}
