import 'dart:convert';
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/http_client.dart';

/// A success envelope.
Map<String, dynamic> _success(Object? data) => {
  'code': 'OK',
  'message': 'ok',
  'status': true,
  'data': ?data,
};

/// A failure envelope.
Map<String, dynamic> _failure(
  String code, {
  String message = 'something went wrong',
  Map<String, dynamic>? data,
  String? requestId,
}) => {
  'code': code,
  'message': message,
  'status': false,
  'data': ?data,
  if (requestId != null) 'meta': {'requestId': requestId},
};

/// The failure a request threw.
Future<MenoException> _captured(Future<Object?> request) async {
  try {
    await request;
  } on MenoException catch (error) {
    return error;
  }
  fail('Expected the request to fail with a MenoException.');
}

/// One scripted answer from the stub transport.
final class _Reply {
  const new(this.status, this.body, {this.retryAfter}) : offline = false;

  /// The device never reached the API.
  const new offline()
    : status = 0,
      body = null,
      retryAfter = null,
      offline = true;

  final int status;
  final Object? body;
  final int? retryAfter;
  final bool offline;
}

/// A dio transport that answers from a script and records what it was asked.
///
/// The last reply repeats, so a test can script one failure and let any further
/// attempt succeed.
final class _StubAdapter implements HttpClientAdapter {
  new(this._replies);

  final List<_Reply> _replies;

  /// Every request that reached the transport, in order.
  final List<RequestOptions> sent = [];

  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) async {
    sent.add(options);
    final reply = _replies.length > 1 ? _replies.removeAt(0) : _replies.first;
    if (reply.offline) {
      throw DioException(
        requestOptions: options,
        type: DioExceptionType.connectionError,
      );
    }
    return ResponseBody.fromString(
      jsonEncode(reply.body),
      reply.status,
      headers: {
        Headers.contentTypeHeader: [Headers.jsonContentType],
        if (reply.retryAfter != null) 'retry-after': ['${reply.retryAfter}'],
      },
    );
  }

  @override
  void close({bool force = false}) {}
}

/// A logger that keeps what it was told.
final class _RecordingLogger implements MenoLogger {
  final List<String> messages = [];

  @override
  void v(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    messages.add(message);
  }

  @override
  void d(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    messages.add(message);
  }

  @override
  void i(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    messages.add(message);
  }

  @override
  void w(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    messages.add(message);
  }

  @override
  void e(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    messages.add(message);
  }

  @override
  void fatal(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {
    messages.add(message);
  }

  @override
  void event(String name, Map<String, Object> context) {
    messages.add(name);
  }
}

void main() {
  late _StubAdapter adapter;

  HttpClient clientWith(List<_Reply> replies) {
    adapter = _StubAdapter(replies);
    return HttpClient(
      logger: _RecordingLogger(),
      baseUrl: 'https://api.test',
      retryBackoff: const Duration(milliseconds: 1),
      dio: Dio()..httpClientAdapter = adapter,
    );
  }

  group('the envelope', () {
    test('a read unwraps the payload', () async {
      final client = clientWith([
        _Reply(200, _success({'id': 7})),
      ]);

      final id = await client.get<int>(
        '/things/7',
        fromJson: (data) => (data as Map<String, dynamic>)['id'] as int,
      );

      expect(id, 7);
    });

    test('a list read decodes each item once', () async {
      final client = clientWith([
        _Reply(
          200,
          _success([
            {'n': 1},
            {'n': 2},
          ]),
        ),
      ]);

      final numbers = await client.getList<int>(
        '/things',
        fromJson: (data) => (data as Map<String, dynamic>)['n'] as int,
      );

      expect(numbers, [1, 2]);
    });

    test('a write that returns no payload completes', () async {
      final client = clientWith([_Reply(200, _success(null))]);

      await expectLater(
        client.postUnit('/things/7/read', data: {'read': true}),
        completes,
      );
    });

    test('a body that is not the envelope is reported as malformed', () async {
      final client = clientWith([const _Reply(200, 'not-an-envelope')]);

      final error = await _captured(
        client.get<int>('/things', fromJson: (_) => 0),
      );

      expect(error, isA<MalformedResponseException>());
      expect(error.code, MenoErrorCode.malformedResponse);
    });
  });

  group('failures', () {
    test(
      '401 becomes an unauthenticated failure, request id included',
      () async {
        final client = clientWith([
          _Reply(401, _failure('UNAUTHORIZED', requestId: 'req-1')),
        ]);

        final error = await _captured(
          client.get<int>('/me', fromJson: (_) => 0),
        );

        expect(error, isA<UnauthenticatedException>());
        expect(error.code, MenoErrorCode.unauthorized);
        expect(error.statusCode, 401);
        expect(error.requestId, 'req-1');
      },
    );

    test('422 becomes a validation failure carrying its fields', () async {
      final client = clientWith([
        _Reply(
          422,
          _failure(
            'VALIDATION_FAILED',
            data: {
              'email': ['Email is required'],
            },
          ),
        ),
      ]);

      final error = await _captured(
        client.postUnit('/users', data: {'email': ''}),
      );

      expect(error, isA<ValidationException>());
      final validation = error as ValidationException;
      expect(validation.messagesFor('email'), ['Email is required']);
      expect(validation.messagesFor('name'), isEmpty);
    });

    test('the code survives the status family it shares', () async {
      final client = clientWith([_Reply(409, _failure('EMAIL_TAKEN'))]);

      final error = await _captured(client.postUnit('/users', data: const {}));

      expect(error, isA<ConflictException>());
      expect(error.code, MenoErrorCode.emailTaken);
    });

    test('400, 403, 404 and 503 map to their own failures', () async {
      final client = clientWith([
        _Reply(400, _failure('INVALID_CURSOR')),
        _Reply(403, _failure('NOT_OWNER')),
        _Reply(404, _failure('NOT_FOUND')),
        _Reply(503, _failure('UPSTREAM_UNAVAILABLE')),
      ]);

      expect(
        await _captured(client.get<int>('/a', fromJson: (_) => 0)),
        isA<BadRequestException>(),
      );
      expect(
        await _captured(client.get<int>('/b', fromJson: (_) => 0)),
        isA<ForbiddenException>(),
      );
      expect(
        await _captured(client.get<int>('/c', fromJson: (_) => 0)),
        isA<NotFoundException>(),
      );
      expect(
        await _captured(client.get<int>('/d', fromJson: (_) => 0)),
        isA<UpstreamUnavailableException>(),
      );
    });

    test('500 becomes a server failure', () async {
      final client = clientWith([_Reply(500, _failure('INTERNAL_ERROR'))]);

      final error = await _captured(
        client.get<int>('/things', fromJson: (_) => 0),
      );

      expect(error, isA<ServerException>());
      expect(error.code, MenoErrorCode.internal);
    });

    test('429 is surfaced with the server retry hint, not retried', () async {
      final client = clientWith([
        _Reply(429, _failure('RATE_LIMITED'), retryAfter: 2),
      ]);

      final error = await _captured(
        client.get<int>('/things', fromJson: (_) => 0),
      );

      expect(error, isA<RateLimitedException>());
      expect(
        (error as RateLimitedException).retryAfter,
        const Duration(seconds: 2),
      );
      expect(adapter.sent, hasLength(1));
    });
  });

  group('retrying', () {
    test('a read retries a transport failure and then succeeds', () async {
      final client = clientWith([
        const _Reply.offline(),
        _Reply(200, _success({'id': 1})),
      ]);

      final id = await client.get<int>(
        '/things/1',
        fromJson: (data) => (data as Map<String, dynamic>)['id'] as int,
      );

      expect(id, 1);
      expect(adapter.sent, hasLength(2));
    });

    test('a mutation that opts in replays with one idempotency key', () async {
      final client = clientWith([
        _Reply(503, _failure('UPSTREAM_UNAVAILABLE')),
        _Reply(200, _success(null)),
      ]);

      await client.postUnit('/broadcasts', data: const {}, retry: true);

      expect(adapter.sent, hasLength(2));
      final keys = [
        for (final request in adapter.sent)
          request.headers[HttpClient.idempotencyHeader],
      ];
      expect(keys, hasLength(2));
      expect(keys.first, isNotNull);
      expect(keys.first, keys.last);
    });

    test(
      'a mutation that does not opt in carries no idempotency key',
      () async {
        final client = clientWith([_Reply(200, _success(null))]);

        await client.postUnit('/broadcasts', data: const {});

        expect(adapter.sent, hasLength(1));
        expect(
          adapter.sent.single.headers[HttpClient.idempotencyHeader],
          isNull,
        );
      },
    );

    test('an upload is never replayed', () async {
      final client = clientWith([
        _Reply(503, _failure('UPSTREAM_UNAVAILABLE')),
      ]);

      final error = await _captured(
        client.upload<int>(
          '/files',
          fromJson: (_) => 0,
          formData: FormData.fromMap({'name': 'a.png'}),
        ),
      );

      expect(error, isA<UpstreamUnavailableException>());
      expect(adapter.sent, hasLength(1));
    });

    test(
      'a failure that keeps failing stops after the attempt limit',
      () async {
        final client = clientWith([
          _Reply(503, _failure('UPSTREAM_UNAVAILABLE')),
        ]);

        final error = await _captured(
          client.get<int>('/things', fromJson: (_) => 0),
        );

        expect(error, isA<UpstreamUnavailableException>());
        expect(adapter.sent, hasLength(3));
      },
    );
  });
}
