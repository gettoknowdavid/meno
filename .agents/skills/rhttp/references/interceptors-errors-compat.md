# rhttp interceptors, errors, compatibility layers

Contents: interceptors, RetryInterceptor and token refresh, exception table, http / dio / dart:io compatibility.

## Interceptors

Pass them to `RhttpClient.create(interceptors: [...])` or to an individual request via `interceptors:`. Any exception that is not an `RhttpException` is wrapped in `RhttpInterceptorException`.

```dart
class AuthInterceptor extends Interceptor {
  AuthInterceptor(this.tokenReader);
  final String? Function() tokenReader;

  @override
  Future<InterceptorResult<HttpRequest>> beforeRequest(HttpRequest request) async {
    final token = tokenReader();
    if (token == null) return Interceptor.next();
    return Interceptor.next(request.addHeader(
      name: HttpHeaderName.authorization,
      value: 'Bearer $token',
    ));
  }

  @override
  Future<InterceptorResult<HttpResponse>> afterResponse(HttpResponse response) async =>
      Interceptor.next();

  @override
  Future<InterceptorResult<RhttpException>> onError(RhttpException exception) async =>
      Interceptor.next();
}
```

Ways to finish a hook:

- `Interceptor.next([value])` continue the chain, optionally with a modified request/response/exception
- `Interceptor.stop()` stop the chain
- `Interceptor.resolve(response)` short-circuit with a response
- `throw RhttpException(...)` fail the request, stack trace preserved

Override only what you need with `SimpleInterceptor`. The README documents the `onError:` constructor callback, and subclassing to override a single hook (the official example's `LoggingInterceptor extends SimpleInterceptor` overrides only `beforeRequest`):

```dart
SimpleInterceptor(
  onError: (e) async {
    if (e is RhttpStatusCodeException && e.statusCode == 401) { /* sign out */ }
    return Interceptor.next();
  },
)

class LoggingInterceptor extends SimpleInterceptor {
  @override
  Future<InterceptorResult<HttpRequest>> beforeRequest(HttpRequest request) async {
    debugPrint('Request: ${request.url}, headers: ${request.headers}');
    return Interceptor.next();
  }
}
```

The official example lists `[AuthInterceptor, RefreshTokenInterceptor, LoggingInterceptor]` in that order; follow that ordering (auth first, then retry, then logging) unless there is a reason not to.

## RetryInterceptor and token refresh

Extend `RetryInterceptor`. On retry, all other interceptors run again (so `AuthInterceptor` will re-read the fresh token), but not the retry interceptor itself. Returning `null` from `beforeRetry` retries the request unchanged (after your side effects); return an `HttpRequest` to retry with a modified one.

```dart
class RefreshTokenInterceptor extends RetryInterceptor {
  RefreshTokenInterceptor(this.refresh);
  final Future<void> Function() refresh;

  @override
  int get maxRetries => 1;

  @override
  bool shouldRetry(HttpResponse? response, RhttpException? exception) =>
      exception is RhttpStatusCodeException &&
      (exception.statusCode == 401 || exception.statusCode == 403);

  @override
  Future<HttpRequest?> beforeRetry(
    int attempt,
    HttpRequest request,
    HttpResponse? response,
    RhttpException? exception,
  ) async {
    await refresh();
    return null;
  }
}
```

Pitfalls:

- The refresh call itself must not use a client that has the same `RefreshTokenInterceptor`, or a failing refresh loops. Use `Rhttp.post(...)` (temporary client) or a separate plain client, as the official Riverpod example does.
- This relies on `throwOnStatusCode: true` (the default). With it disabled, 401 arrives as a normal response, so check `response?.statusCode` in `shouldRetry` instead.

## Exceptions

All extend `RhttpException`.

| Exception                          | Meaning                                                                  |
| ---------------------------------- | ------------------------------------------------------------------------ |
| `RhttpCancelException`             | cancelled via `CancelToken`                                              |
| `RhttpTimeoutException`            | timeout hit                                                              |
| `RhttpRedirectException`           | too many redirects                                                       |
| `RhttpStatusCodeException`         | 4xx / 5xx (has `statusCode`, `body`)                                     |
| `RhttpInvalidCertificateException` | TLS certificate invalid                                                  |
| `RhttpConnectionException`         | no internet / unreachable                                                |
| `RhttpClientDisposedException`     | client already disposed (was `RhttpInvalidClientException` before 0.7.0) |
| `RhttpInterceptorException`        | interceptor threw a non-rhttp exception                                  |
| `RhttpUnknownException`            | anything else                                                            |

`RhttpCompatibleClient` additionally wraps failures in `RhttpWrappedClientException`.

## Compatibility layers

### package:http

```dart
final http.Client client = await RhttpCompatibleClient.create(); // or createSync()
final res = await client.get(Uri.parse('https://example.com'));
```

Downsides: `body` is `Object?` (less type safety), a `Map` body is always form-encoded, no `CancelToken` (supports `Abortable` from http 1.5+). It sets `throwOnStatusCode` to false like the stock client. `close()` cancels running requests.

### dio

Use `dio_compatibility_layer` with the compatible client:

```dart
Future<Dio> createDio() async {
  final dio = Dio();
  dio.httpClientAdapter = ConversionLayerAdapter(await RhttpCompatibleClient.create());
  return dio;
}
```

### dart:io HttpClient

```dart
final client = await IoCompatibleClient.create();
final request = await client.getUrl(Uri.parse('https://example.com'));
final response = await request.close();
print(await response.transform(utf8.decoder).join());
```
