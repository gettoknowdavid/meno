import 'dart:async';
import 'dart:io';
import 'dart:math';

import 'package:dio/dio.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/meno_response.dart';

/// Decodes a value out of a response envelope's `data`.
typedef FromJson<T> = T Function(dynamic data);

/// The app's single HTTP client.
///
/// Transport only: it unwraps the API's envelope, turns failures into
/// [MenoException]s and logs what it did. It knows nothing about endpoints, so
/// each feature talks to the API through its own thin service built on this.
///
/// Every response — success or failure — is parsed as `{code, message, status,
/// data?}`. The HTTP status line is authoritative and is never repeated in the
/// body, so [Options.validateStatus] accepts every status. A body that is not
/// the envelope becomes a [MalformedResponseException].
///
/// Retrying is opt-in per request, on for `GET` and off for mutations by
/// default. A mutation that opts in is sent with an `Idempotency-Key`, which
/// the API uses to run it once even if the first attempt actually arrived.
/// Only failures that mean *the request never reached a handler* are retried:
/// no connectivity, a timeout, or a 503 from a dependency. Anything the API
/// answered about the request itself is surfaced immediately.
final class HttpClient implements Disposable {
  /// Creates a client for [baseUrl].
  ///
  /// [dio] lets a caller supply the transport — tests hand it a dio wired to a
  /// stub adapter. The client owns the dio it is given, so passing one only
  /// replaces the transport, not the contract.
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
      // Every response is parsed as the envelope, so dio must not turn a 4xx or
      // 5xx into a `badResponse` exception.
      ..validateStatus = ((status) => status != null)
      // A redirect would move the request — and whatever credential it
      // carries — to an address nobody approved.
      ..followRedirects = false;
    _dio.interceptors.addAll(interceptors);
  }

  /// The header the API expects on a mutation that may be replayed.
  ///
  /// Supplied by the caller as `idempotencyKey` and forwarded unchanged: one
  /// value per *logical* operation, reused by every attempt that operation
  /// makes. The value must be a UUID; anything else is refused with
  /// `INVALID_IDEMPOTENCY_KEY`.
  ///
  /// The API replays a stored outcome for 24 hours (`IDEMPOTENCY_TTL` in
  /// `apps/api/src/middleware/idempotency.rs`), so this is finite
  /// deduplication — enough to cover a retry, not a guarantee that a request
  /// can never run twice.
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

  /// Runs a `GET` and decodes the envelope's `data` with [fromJson].
  Future<T> get<T>(
    String path, {
    required FromJson<T> fromJson,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    CancelToken? cancelToken,
    AuthMode authMode = AuthMode.account,
    bool retry = true,
  }) {
    return _send<T>(
      method: 'GET',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      authMode: authMode,
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
    AuthMode authMode = AuthMode.account,
    bool retry = true,
  }) {
    return _send<List<T>>(
      method: 'GET',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      authMode: authMode,
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
        final items = <T>[];
        for (final item in data) {
          try {
            items.add(fromJson(item));
          } on Object catch (error, stackTrace) {
            logger.w(
              'Skipped a $path item that could not be read',
              tag: MenoLogTag.network.label,
              error: error,
              stackTrace: stackTrace,
            );
          }
        }
        return items;
      },
    );
  }

  /// Runs a `POST` and decodes the envelope's `data` with [fromJson].
  /// Runs a `POST` and decodes the envelope's `data` with [fromJson].
  Future<T> post<T>(
    String path, {
    required FromJson<T> fromJson,
    Object? data,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    String? idempotencyKey,
    CancelToken? cancelToken,
    AuthMode authMode = AuthMode.account,
    bool retry = false,
  }) {
    return _send<T>(
      method: 'POST',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      idempotencyKey: idempotencyKey,
      jsonBody: data != null,
      authMode: authMode,
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
    String? idempotencyKey,
    CancelToken? cancelToken,
    AuthMode authMode = AuthMode.account,
    bool retry = false,
  }) {
    return _send<T>(
      method: 'PUT',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      idempotencyKey: idempotencyKey,
      jsonBody: data != null,
      authMode: authMode,
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
    String? idempotencyKey,
    CancelToken? cancelToken,
    AuthMode authMode = AuthMode.account,
    bool retry = false,
  }) {
    return _send<T>(
      method: 'PATCH',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      idempotencyKey: idempotencyKey,
      jsonBody: data != null,
      authMode: authMode,
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
    String? idempotencyKey,
    CancelToken? cancelToken,
    AuthMode authMode = AuthMode.account,
    bool retry = false,
  }) {
    return _send<T>(
      method: 'DELETE',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      idempotencyKey: idempotencyKey,
      jsonBody: data != null,
      authMode: authMode,
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

  /// Runs a `POST` whose endpoint returns no payload.
  Future<void> postUnit(
    String path, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    String? idempotencyKey,
    CancelToken? cancelToken,
    AuthMode authMode = AuthMode.account,
    bool retry = false,
  }) {
    return _send<void>(
      method: 'POST',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      idempotencyKey: idempotencyKey,
      jsonBody: data != null,
      authMode: authMode,
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
    String? idempotencyKey,
    CancelToken? cancelToken,
    AuthMode authMode = AuthMode.account,
    bool retry = false,
  }) {
    return _send<void>(
      method: 'PUT',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      idempotencyKey: idempotencyKey,
      jsonBody: data != null,
      authMode: authMode,
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
    String? idempotencyKey,
    CancelToken? cancelToken,
    AuthMode authMode = AuthMode.account,
    bool retry = false,
  }) {
    return _send<void>(
      method: 'DELETE',
      path: path,
      retry: retry,
      cancelToken: cancelToken,
      headers: headers,
      idempotencyKey: idempotencyKey,
      jsonBody: data != null,
      authMode: authMode,
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

  /// Uploads [formData] and decodes the envelope's `data` with [fromJson].
  ///
  /// Never retried: a body that took a minute to send is not worth resending on
  /// a guess.
  Future<T> upload<T>(
    String path, {
    required FromJson<T> fromJson,
    required FormData formData,
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
    ProgressCallback? onSendProgress,
    CancelToken? cancelToken,
    AuthMode authMode = AuthMode.account,
  }) {
    return _send<T>(
      method: 'POST',
      path: path,
      cancelToken: cancelToken,
      headers: headers,
      contentType: null,
      authMode: authMode,
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
    AuthMode authMode = AuthMode.account,
    bool deleteOnError = true,
  }) async {
    final stopwatch = Stopwatch()..start();
    try {
      // Every status is accepted so the response reaches the same auth policy
      // as an ordinary request: a 401 on a download renews and replays under
      // the captured identity instead of becoming a raw `badResponse`.
      final response = await _dio.download(
        urlPath,
        savePath,
        queryParameters: queryParameters,
        onReceiveProgress: onReceiveProgress,
        cancelToken: cancelToken,
        deleteOnError: deleteOnError,
        options: Options(
          extra: {AuthMode.extraKey: authMode},
          validateStatus: (status) => status != null,
        ),
      );

      final status = response.statusCode;
      if (status != null && (status < 200 || status >= 300)) {
        // The body of a failed download is not the file the caller wanted:
        // remove it so a partial answer can never be mistaken for the result.
        await _deletePartial(savePath);
        throw status >= 300 && status < 400
            ? RedirectNotAllowedException(statusCode: status)
            : MenoException.fromResponse(
                statusCode: status,
                code: 'DOWNLOAD_$status',
                message: 'The file could not be downloaded.',
              );
      }

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

  /// A token that cancels one request, or every request it is handed to.
  CancelToken createCancelToken() => CancelToken();

  @override
  FutureOr<dynamic> onDispose() {
    _dio.close(force: true);
  }

  /// Sends one request, retrying it when that is safe and asked for.
  Future<T> _send<T>({
    required String method,
    required String path,
    required Future<Response<dynamic>> Function(Options options) send,
    required FromJson<T> parse,
    bool retry = false,
    Map<String, dynamic>? headers,
    String? idempotencyKey,
    CancelToken? cancelToken,
    AuthMode authMode = AuthMode.account,
    bool jsonBody = false,
    String? contentType = Headers.jsonContentType,
  }) async {
    final options = Options(
      extra: {AuthMode.extraKey: authMode},
      headers: {
        if (contentType != null && jsonBody)
          Headers.contentTypeHeader: contentType,
        ...?headers,
      },
    );

    // A key the caller supplied is forwarded as-is. One is never invented here:
    // a key generated inside the transport would be new on every call, so a
    // manual retry after an ambiguous failure would still duplicate the work
    // (audit H20). Where the logical operation begins is the only place that
    // knows which attempts belong together.
    if (idempotencyKey != null) {
      options.headers?[idempotencyHeader] = idempotencyKey;
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
    final status = response.statusCode;
    if (status != null && status >= 300 && status < 400) {
      // Never followed: the redirect target is not the approved origin the
      // request was dispatched to.
      throw RedirectNotAllowedException(
        statusCode: status,
        cause: response.headers.value('location'),
      );
    }

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
      // The envelope was valid but its payload is not what the caller expects.
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
        // A security failure, not an outage: retrying cannot help, and
        // reporting it as "no connection" hides what actually happened.
        return InsecureConnectionException(cause: error);
      case DioExceptionType.cancel:
        return CancelledException(cause: error);
      case DioExceptionType.badResponse:
        // Only reachable from `download`, which asks for a non-2xx status to
        // fail. A redirect is refused before its body is looked at.
        final status = error.response?.statusCode;
        if (status != null && status >= 300 && status < 400) {
          return RedirectNotAllowedException(statusCode: status, cause: error);
        }
        // The body may still be the envelope, so try it first.
        return _failureFromBody(error.response) ??
            ServerException(
              message:
                  error.response?.statusMessage ?? 'The request was rejected.',
              statusCode: error.response?.statusCode ?? 0,
              cause: error,
            );
      case DioExceptionType.unknown:
        // An interceptor may have already decided the reason. Keeping it means
        // a refusal arrives at the caller with its own type instead of being
        // flattened into "something went wrong".
        final reason = error.error;
        if (reason is MenoException) return reason;
        // Reaching this means something else went wrong, not that the device is
        // offline.
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

  /// Removes a file a failed download left behind.
  ///
  /// Best effort: the reason the download failed is what the caller sees, and
  /// a leftover file that cannot be deleted will be overwritten or ignored.
  Future<void> _deletePartial(String savePath) async {
    try {
      final file = File(savePath);
      if (file.existsSync()) file.deleteSync();
    } on Object catch (error) {
      logger.w(
        'A failed download could not remove its partial file',
        tag: MenoLogTag.network.label,
        error: error,
      );
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
    // A hint longer than the app is willing to wait means "come back later".
    if (hinted != null && hinted > _maxRetryWait) return null;

    final backoff = retryBackoff * (1 << (attempt - 1));
    final capped = backoff > _maxRetryWait ? _maxRetryWait : backoff;

    // Spread retries out so one outage does not become a stampede on
    // recovery. Jitter only ever *adds* to the base wait — it is applied
    // before the server's minimum, never to it.
    final jittered = capped * (0.5 + _random.nextDouble() / 2);

    // The server's `Retry-After` is a floor: the wait is at least that long,
    // whatever the jitter would have chosen (L03).
    if (hinted != null && hinted > jittered) return hinted;
    return jittered;
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
  /// The API sends `{field: [message, …]}`; any other shape is ignored.
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
