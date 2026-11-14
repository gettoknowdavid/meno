---
name: rhttp
description: Expert guidance on the rhttp Flutter package (pub.dev/packages/rhttp), a Rust-powered HTTP client (reqwest + flutter_rust_bridge FFI) with HTTP/1.1, HTTP/2, HTTP/3, interceptors, retry, certificate pinning, proxies, custom DNS, cookies, cancellation and progress callbacks. Use this skill whenever the user mentions rhttp, Rhttp.init, RhttpClient, RhttpCompatibleClient, IoCompatibleClient, ClientSettings, HttpBody, RetryInterceptor, RhttpException, or wants to replace dart:io HttpClient / http / dio with a faster Rust-backed client in Flutter, needs HTTP/3 in Flutter, or is debugging rhttp setup (Rust toolchain, Android NDK, Proguard, rootCertSource). Also use it when migrating rhttp across versions (0.5 to 0.18 breaking changes), even if the user does not name the package explicitly but describes Rust-based HTTP in Flutter.
---

# rhttp

rhttp is a Dart wrapper around Rust's `reqwest`, called through `flutter_rust_bridge` FFI. Current version when this skill was written: **0.18.0** (June 2026). Source lives on Codeberg (`codeberg.org/Tienisto/rhttp`); the old GitHub repo is archived. Platforms: Android, iOS, macOS, Windows, Linux. **Web is not supported.**

Why it matters when writing code: the API is deliberately strongly typed, and the response body type is chosen _before_ the request so Rust does the decoding work. Most mistakes come from forgetting init, creating a client per request, or using pre-0.18 setting names.

## Setup checklist

1. Install Rust via rustup (Rust 1.80.0+). The pinned toolchain is installed automatically when building.
2. Android: command-line tools and the latest NDK must be installed. Add the `INTERNET` permission to the manifest.
3. `pubspec.yaml`: `rhttp: ^0.18.0`
4. Call `await Rhttp.init();` in `main()` before `runApp` (after `WidgetsFlutterBinding.ensureInitialized()` if other plugins need it). Skipping this is the most common runtime failure.

```dart
import 'package:flutter/material.dart';
import 'package:rhttp/rhttp.dart';

Future<void> main() async {
  await Rhttp.init();
  runApp(const MyApp());
}
```

For build failures (Rust/NDK/Proguard/cert issues) read `references/setup-troubleshooting.md`.

## Core API shape

Static helpers on `Rhttp` create a throwaway client per call. Use them for one-offs only.

```dart
final res = await Rhttp.get('https://example.com');      // HttpTextResponse
res.statusCode; res.body; res.headerMap; res.remoteIp; res.version;

await Rhttp.post(url, body: HttpBody.json({'k': 'v'}));
await Rhttp.getBytes(url);   // HttpBytesResponse, body is Uint8List
await Rhttp.getStream(url);  // HttpStreamResponse, body is Stream<Uint8List>
await Rhttp.requestText(method: HttpMethod.post, url: url);
```

Method variants exist per verb with `Text` (default), `Bytes`, `Stream` suffixes. `HttpMethod` accepts any string for custom verbs. `res.bodyToJson` decodes a text body.

Common request parameters: `query: {}` (also `queryRaw`), `headers: HttpHeaders.map({...})`, `body`, `settings`, `interceptors`, `cancelToken`, `onSendProgress`, `onReceiveProgress`.

### Bodies

| Constructor                                                                                                           | Notes                                                     |
| --------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| `HttpBody.text('...')`                                                                                                | raw string                                                |
| `HttpBody.json(obj)`                                                                                                  | any JSON-encodable `Object?`; sets Content-Type if absent |
| `HttpBody.bytes(Uint8List)`                                                                                           | binary                                                    |
| `HttpBody.stream(stream, length: n)`                                                                                  | pass `length` so Content-Length is set                    |
| `HttpBody.form({...})`                                                                                                | flat map, `x-www-form-urlencoded`                         |
| `HttpBody.multipart({'name': MultipartItem.text(text: ...), 'file': MultipartItem.bytes(bytes: ..., fileName: ...)})` | boundary generated, header overridden                     |

## Reuse a client (the default recommendation)

A client pools connections and carries settings and interceptors. Create one at startup and share it, rather than calling `Rhttp.get` repeatedly.

```dart
final client = await RhttpClient.create(
  settings: const ClientSettings(
    baseUrl: 'https://api.example.com',
    userAgent: 'MyApp/1.0',
    timeoutSettings: TimeoutSettings(
      timeout: Duration(seconds: 15),
      connectTimeout: Duration(seconds: 5),
      keepAliveTimeout: Duration(seconds: 60),
    ),
  ),
  interceptors: [/* see references/interceptors-errors-compat.md */],
);

final res = await client.get('/users', query: {'page': '1'});
client.dispose(); // when done; later use throws RhttpClientDisposedException
```

`RhttpClient.createSync()` avoids `await` for DI containers and providers, but only call it at startup because it blocks the calling thread.

## Cancellation and progress

```dart
final token = CancelToken();
final future = client.get(url, cancelToken: token);
token.cancel();                  // future throws RhttpCancelException
```

One token can cancel many requests. Progress callbacks need `Uint8List` or `Stream` bodies; `total` is `-1` when unknown.

```dart
await client.post(url,
  body: HttpBody.bytes(bytes),
  onSendProgress: (sent, total) {},
  onReceiveProgress: (received, total) {});
```

## Errors

Everything extends `RhttpException`. By default 4xx/5xx throw `RhttpStatusCodeException` (has `statusCode` and `body`). Set `ClientSettings(throwOnStatusCode: false)` to receive the response instead.

```dart
try {
  await client.get(url);
} on RhttpStatusCodeException catch (e) {
  handle(e.statusCode, e.body);
} on RhttpTimeoutException {
} on RhttpConnectionException {   // offline, unreachable
} on RhttpCancelException {
} on RhttpException catch (e) {   // catch-all
}
```

Full exception table is in `references/interceptors-errors-compat.md`.

## Where to look next

| Need                                                                                           | Read                                       |
| ---------------------------------------------------------------------------------------------- | ------------------------------------------ |
| TLS, pinning, mTLS, root certs, proxy, DNS, redirects, cookies, HTTP version, keep-alive       | `references/client-settings.md`            |
| Interceptors, RetryInterceptor token refresh, exceptions, `http`/`dio`/`dart:io` compatibility | `references/interceptors-errors-compat.md` |
| Install/build errors, Android NDK, Proguard, version migration                                 | `references/setup-troubleshooting.md`      |

## Behaviors worth remembering

- `ClientSettings` can be passed per request or per client. Per-request settings on `Rhttp.get` build a temporary client, so they do not reuse a shared pool.
- Default root certs come from Mozilla's webpki bundle, not the OS store. Corporate or private CAs need `RootCertSource.platform` or `trustedRootCertificates`.
- `RhttpCompatibleClient` forces `throwOnStatusCode: false` to match the `http` package, and does not support `CancelToken` (use `Abortable` from `http` 1.5+).
- DevTools Network tab shows rhttp traffic automatically.
- Default redirect limit is 10; system proxy is on by default; connections are not kept alive unless `keepAliveTimeout > 0`.
- HTTP/3 is opt-in via `httpVersionPref: HttpVersionPref.http3`.
- APK size grows about 2 MB (arm64) or 6 MB (all ABIs).
