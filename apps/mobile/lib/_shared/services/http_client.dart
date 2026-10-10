import 'dart:async';
import 'dart:math';

import 'package:dio/dio.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/meno_response.dart';
import 'package:uuid/uuid.dart';

/// Decodes a value out of a response envelope's `data`.
typedef FromJson<T> = T Function(dynamic data);

/// The app's single HTTP client.
///
/// Transport only: it unwraps the API's envelope, turns failures into
/// [MenoException]s and logs what it did. It knows nothing about endpoints, so
/// every feature talks to the API through its own thin service built on this.
///
/// ## The envelope
///
/// Every response — success or failure — is parsed as `{code, message, status,
/// data?}`. The HTTP status line is authoritative and is never repeated in the
/// body, so [Options.validateStatus] accepts every status: the body is what
/// says why the request failed, and throwing it away would leave only a number.
/// A body that is not the envelope (an error page from a proxy, a truncated
/// response) becomes a [MalformedResponseException].
///
/// ## Retrying
///
/// Retrying is opt-in per request and defaults to on for `GET` (repeating one
/// costs nothing) and off for mutations. A mutation that opts in is sent with
/// an `Idempotency-Key`, which the API uses to run it once even if the first
/// attempt actually arrived — see `apps/api/src/middleware/idempotency.rs`.
///
/// Only failures that mean *the request never reached a handler* are retried:
/// no connectivity, a timeout, or a 503 from a dependency. Anything the API
/// answered about the request itself — including 429 — is surfaced immediately,
/// because repeating it would get the same answer.
final class HttpClient implements Disposable {
  /// Creates a client for [baseUrl].
  ///
  /// [dio] lets a caller supply the transport — tests hand it a dio wired to a
  /// stub adapter, and a later session interceptor may need the same instance.
  /// The client owns the dio it is given: its options, timeouts and headers are
  /// set here, so passing one only replaces the transport, not the contract.
  new({
    required this.logger,
    required String baseUrl,
    Duration connectTimeout = const Duration(seconds: 30),
    Duration receiveTimeout = const Duration(seconds: 30),
    Duration sendTimeout = const Duration(seconds: 30),
    Map<String, dynamic>? headers,
    List<Interceptor> interceptors = const [],
    this.maxAttempts = 3,
    this.retryBackoff = const Duration(milliseconds: 500),
    Dio? dio,
  }) : _dio = dio ?? Dio() {
    _dio.options
      ..baseUrl = baseUrl
      ..connectTimeout = connectTimeout
      ..receiveTimeout = receiveTimeout
      ..sendTimeout = sendTimeout
      ..headers = {
        ..._dio.options.headers,
        Headers.acceptHeader: Headers.jsonContentType,
        ...?headers,
      }
      // Every response, success or failure, is parsed as the envelope, so dio
      // must not turn a 4xx or 5xx into a `badResponse` exception and take the
      // body down with it.
      ..validateStatus = (status) => status != null;
    _dio.interceptors.addAll(interceptors);
  }

  /// The header the API expects on a mutation that may be replayed.
  static const String idempotencyHeader = 'Idempotency-Key';

  /// The header the API sends with a retry hint on a 503.
  static const String retryAfterHeader = 'Retry-After';

  /// Longest a single retry waits, however large the server's hint.
  static const Duration _maxRetryWait = Duration(seconds: 10);

  final Dio _dio;

  /// Where the client reports what it did.
  final MenoLogger logger;

  /// How many attempts a retryable request gets, the first one included.
  final int maxAttempts;

  /// The wait before the second attempt; each further attempt doubles it.
  final Duration retryBackoff;

  final Random _random = Random();
  final Uuid _uuid = const Uuid();

  // ======================================================================
  // READS
  // ======================================================================

  /// Runs a `GET` and decodes the envelope's `data` with [fromJson].
  Future<T> get<T>(
    String path, {
    required FromJson<T> fromJson,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    bool retry = true,
  }) {
    return _send<T>(
      method: 'GET',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      send: (options) => _dio.get<dynamic>(
        path,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      ),
      parse: fromJson,
    );
  }

  /// Runs a `GET` and decodes the envelope's `data` as a list of [T].
  Future<List<T>> getList<T>(
    String path, {
    required FromJson<T> fromJson,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    bool retry = true,
  }) {
    return _send<List<T>>(
      method: 'GET',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      send: (options) => _dio.get<dynamic>(
        path,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      ),
      parse: (data) {
        if (data is! List) {
          throw MalformedResponseException(
            message: 'Expected a list but got ${data.runtimeType}.',
          );
        }
        return [for (final item in data) fromJson(item)];
      },
    );
  }

  // ======================================================================
  // WRITES
  // ======================================================================

  /// Runs a `POST` and decodes the envelope's `data` with [fromJson].
  Future<T> post<T>(
    String path, {
    required FromJson<T> fromJson,
    Object? data,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    bool retry = false,
  }) {
    return _send<T>(
      method: 'POST',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      jsonBody: data != null,
      send: (options) => _dio.post<dynamic>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      ),
      parse: fromJson,
    );
  }

  /// Runs a `PUT` and decodes the envelope's `data` with [fromJson].
  Future<T> put<T>(
    String path, {
    required FromJson<T> fromJson,
    Object? data,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    bool retry = false,
  }) {
    return _send<T>(
      method: 'PUT',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      jsonBody: data != null,
      send: (options) => _dio.put<dynamic>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      ),
      parse: fromJson,
    );
  }

  /// Runs a `PATCH` and decodes the envelope's `data` with [fromJson].
  Future<T> patch<T>(
    String path, {
    required FromJson<T> fromJson,
    Object? data,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    bool retry = false,
  }) {
    return _send<T>(
      method: 'PATCH',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      jsonBody: data != null,
      send: (options) => _dio.patch<dynamic>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      ),
      parse: fromJson,
    );
  }

  /// Runs a `DELETE` and decodes the envelope's `data` with [fromJson].
  Future<T> delete<T>(
    String path, {
    required FromJson<T> fromJson,
    Object? data,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    bool retry = false,
  }) {
    return _send<T>(
      method: 'DELETE',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      jsonBody: data != null,
      send: (options) => _dio.delete<dynamic>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      ),
      parse: fromJson,
    );
  }

  // ======================================================================
  // WRITES WITH NO PAYLOAD
  // ======================================================================

  /// Runs a `POST` whose endpoint returns no payload.
  Future<void> postUnit(
    String path, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    bool retry = false,
  }) {
    return _send<void>(
      method: 'POST',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      jsonBody: data != null,
      send: (options) => _dio.post<dynamic>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      ),
      parse: (_) {},
    );
  }

  /// Runs a `PUT` whose endpoint returns no payload.
  Future<void> putUnit(
    String path, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    bool retry = false,
  }) {
    return _send<void>(
      method: 'PUT',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      jsonBody: data != null,
      send: (options) => _dio.put<dynamic>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      ),
      parse: (_) {},
    );
  }

  /// Runs a `DELETE` whose endpoint returns no payload.
  Future<void> deleteUnit(
    String path, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    bool retry = false,
  }) {
    return _send<void>(
      method: 'DELETE',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      jsonBody: data != null,
      send: (options) => _dio.delete<dynamic>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      ),
      parse: (_) {},
    );
  }

  // ======================================================================
  // FILES
  // ======================================================================

  /// Uploads [formData] and decodes the envelope's `data` with [fromJson].
  ///
  /// Never retried: a body that took a minute to send is not worth sending
  /// again on a guess.
  Future<T> upload<T>(
    String path, {
    required FromJson<T> fromJson,
    required FormData formData,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    ProgressCallback? onSendProgress,
    CancelToken? cancelToken,
  }) {
    return _send<T>(
      method: 'POST',
      path: path,
      cancelToken: cancelToken,
      headers: headers,
      contentType: null,
      send: (options) => _dio.post<dynamic>(
        path,
        data: formData,
        queryParameters: queryParameters,
        options: options,
        onSendProgress: onSendProgress,
        cancelToken: cancelToken,
      ),
      parse: fromJson,
    );
  }

  /// Downloads [urlPath] to [savePath].
  ///
  /// The only call that does not parse an envelope — the body is the file — so
  /// it asks dio to fail on a non-2xx status instead, which keeps the partial
  /// file out of the caller's way.
  Future<void> download(
    String urlPath,
    String savePath, {
    ProgressCallback? onReceiveProgress,
    Map<String, dynamic>? queryParameters,
    CancelToken? cancelToken,
    bool deleteOnError = true,
  }) async {
    final stopwatch = Stopwatch()..start();
    try {
      await _dio.download(
        urlPath,
        savePath,
        queryParameters: queryParameters,
        onReceiveProgress: onReceiveProgress,
        cancelToken: cancelToken,
        deleteOnError: deleteOnError,
        options: Options(
          validateStatus: (status) => status == null || status < 300,
        ),
      );
      logger.d(
        'GET $urlPath → file (${stopwatch.elapsedMilliseconds}ms)',
        tag: MenoLogTag.network.label,
      );
    } on DioException catch (error, stackTrace) {
      final mapped = _mapDioError(error);
      _logFailure('GET', urlPath, mapped, stackTrace);
      throw mapped;
    }
  }

  // ======================================================================
  // UTILITIES
  // ======================================================================

  /// A token that cancels one request, or every request it is handed to.
  CancelToken createCancelToken() => CancelToken();

  @override
  FutureOr<dynamic> onDispose() {
    _dio.close(force: true);
  }

  // ======================================================================
  // CORE
  // ======================================================================

  /// Sends one request, retrying it when that is safe and asked for.
  Future<T> _send<T>({
    required String method,
    required String path,
    required Future<Response<dynamic>> Function(Options options) send,
    required FromJson<T> parse,
    bool retry = false,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    bool jsonBody = false,
    String? contentType = Headers.jsonContentType,
  }) async {
    final options = Options(
      headers: {
        if (contentType != null && jsonBody)
          Headers.contentTypeHeader: contentType,
        ...?headers,
      },
    );

    // A mutation that may be replayed needs a key the API can recognise, and
    // that key has to be the same on every attempt: that is what makes the
    // replay the same request rather than a second one.
    if (retry && _isMutation(method)) {
      options.headers?[idempotencyHeader] ??= _uuid.v4();
    }

    var attempt = 1;
    while (true) {
      final stopwatch = Stopwatch()..start();
      try {
        final response = await send(options);
        final result = _decode(response, parse);
        logger.d(
          '$method $path → ${response.statusCode}'
          ' (${stopwatch.elapsedMilliseconds}ms)',
          tag: MenoLogTag.network.label,
        );
        return result;
      } on Object catch (error, stackTrace) {
        // The transport reports its own failures as dio exceptions; everything
        // else in here — the envelope, the caller's decoder — already speaks
        // MenoException. Anything further out is a bug rather than a request
        // failure, so it keeps its own type and stack.
        final failure = switch (error) {
          MenoException() => error,
          DioException() => _mapDioError(error),
          _ => Error.throwWithStackTrace(error, stackTrace),
        };
        final wait = _retryWait(failure, attempt, retry, cancelToken);
        if (wait == null) {
          _logFailure(method, path, failure, stackTrace);
          Error.throwWithStackTrace(failure, stackTrace);
        }
        logger.w(
          '$method $path failed with ${failure.code};'
          ' retrying in ${wait.inMilliseconds}ms (attempt ${attempt + 1})',
          tag: MenoLogTag.network.label,
        );
        await _pause(wait, cancelToken);
        if (cancelToken?.isCancelled ?? false) {
          throw const CancelledException();
        }
        attempt++;
      }
    }
  }

  /// Unwraps [response], throwing the failure it carries when it failed.
  T _decode<T>(Response<dynamic> response, FromJson<T> parse) {
    final envelope = MenoResponse<dynamic>.fromJson(
      response.data,
      (data) => data,
    );

    if (!envelope.status) {
      throw MenoException.fromResponse(
        statusCode: response.statusCode,
        code: envelope.code,
        message: envelope.message,
        requestId: envelope.meta?.requestId,
        fieldErrors: _fieldErrors(envelope.data),
        retryAfter: _retryAfter(response),
      );
    }

    try {
      return parse(envelope.data);
    } on MenoException {
      rethrow;
    } on Object catch (error) {
      // The envelope was valid but its payload is not what the caller expects:
      // a DTO that does not match the API, or an empty payload where data was
      // required. Both are contract failures, not data.
      throw MalformedResponseException(
        cause: error,
        message:
            'Could not read the payload of ${response.requestOptions.path}.',
      );
    }
  }

  /// Turns a transport failure into the exception that describes it.
  MenoException _mapDioError(DioException error) {
    switch (error.type) {
      case DioExceptionType.connectionTimeout:
      case DioExceptionType.sendTimeout:
      case DioExceptionType.receiveTimeout:
      case DioExceptionType.transformTimeout:
        return RequestTimeoutException(cause: error);
      case DioExceptionType.connectionError:
        return NetworkException(cause: error);
      case DioExceptionType.badCertificate:
        return const NetworkException(message: 'Secure connection failed.');
      case DioExceptionType.cancel:
        return CancelledException(cause: error);
      case DioExceptionType.badResponse:
        // Only reachable from `download`, which asks for a non-2xx status to
        // fail. The body may still be the envelope, so try it first.
        return _failureFromBody(error.response) ??
            ServerException(
              message:
                  error.response?.statusMessage ?? 'The request was rejected.',
              statusCode: error.response?.statusCode ?? 0,
              cause: error,
            );
      case DioExceptionType.unknown:
        // dio classifies a socket failure as `connectionError`, so reaching
        // this means something else went wrong, not that the device is offline.
        return UnknownException(
          message: 'The request could not be sent. Please try again.',
          cause: error.error ?? error,
        );
    }
  }

  /// The failure an error response's body describes, when it carries one.
  MenoException? _failureFromBody(Response<dynamic>? response) {
    if (response == null) return null;
    final body = response.data;
    if (body is! Map<String, dynamic>) return null;
    try {
      final envelope = MenoResponse<dynamic>.fromJson(body, (data) => data);
      if (envelope.status) return null;
      return MenoException.fromResponse(
        statusCode: response.statusCode,
        code: envelope.code,
        message: envelope.message,
        requestId: envelope.meta?.requestId,
        fieldErrors: _fieldErrors(envelope.data),
        retryAfter: _retryAfter(response),
      );
    } on MenoException {
      return null;
    }
  }

  /// How long to wait before the next attempt, or null to give up.
  Duration? _retryWait(
    MenoException error,
    int attempt,
    bool retry,
    CancelToken? cancelToken,
  ) {
    if (!retry || attempt >= maxAttempts) return null;
    if (cancelToken?.isCancelled ?? false) return null;
    if (!_isRetryable(error)) return null;

    final hinted = switch (error) {
      UpstreamUnavailableException() => error.retryAfter,
      _ => null,
    };
    // A hint longer than the app is willing to wait means "come back later":
    // retrying inside this call would just hold the caller for no reason.
    if (hinted != null && hinted > _maxRetryWait) return null;

    final backoff = retryBackoff * (1 << (attempt - 1));
    final capped = backoff > _maxRetryWait ? _maxRetryWait : backoff;
    final wait = hinted != null && hinted > capped ? hinted : capped;

    // Spread retries out: a synchronised backoff turns one outage into a
    // stampede of the API the moment it recovers.
    return wait * (0.5 + _random.nextDouble() / 2);
  }

  /// Whether repeating the request could plausibly succeed.
  bool _isRetryable(MenoException error) =>
      error is NetworkException ||
      error is RequestTimeoutException ||
      error is UpstreamUnavailableException;

  /// Waits [wait], returning early when the caller cancels.
  Future<void> _pause(Duration wait, CancelToken? cancelToken) {
    if (cancelToken == null) return Future<void>.delayed(wait);

    final done = Completer<void>();
    final timer = Timer(wait, () {
      if (!done.isCompleted) done.complete();
    });
    unawaited(
      cancelToken.whenCancel.then((_) {
        timer.cancel();
        if (!done.isCompleted) done.complete();
      }),
    );
    return done.future;
  }

  /// The per-field detail a `VALIDATION_FAILED` envelope carries in `data`.
  ///
  /// The API sends `{field: [message, …]}`; a value of any other shape is
  /// ignored rather than guessed at.
  Map<String, List<String>>? _fieldErrors(dynamic data) {
    if (data is! Map) return null;
    final fields = <String, List<String>>{};
    for (final entry in data.entries) {
      final value = entry.value;
      if (value is List) {
        fields[entry.key.toString()] = [for (final item in value) '$item'];
      }
    }
    return fields.isEmpty ? null : fields;
  }

  /// The server's retry hint, when it sent one.
  Duration? _retryAfter(Response<dynamic> response) {
    final header = response.headers.value(retryAfterHeader);
    final seconds = header == null ? null : int.tryParse(header);
    return seconds == null ? null : Duration(seconds: seconds);
  }

  /// Whether [method] changes something on the server.
  bool _isMutation(String method) =>
      method == 'POST' ||
      method == 'PUT' ||
      method == 'PATCH' ||
      method == 'DELETE';

  void _logFailure(
    String method,
    String path,
    MenoException error,
    StackTrace stackTrace,
  ) {
    final requestId = error.requestId;
    logger.e(
      '$method $path failed: ${error.code}'
      '${requestId == null ? '' : ' [requestId: $requestId]'}',
      tag: MenoLogTag.network.label,
      error: error,
      stackTrace: stackTrace,
    );
  }
}
