# rhttp ClientSettings reference

Contents: overview, timeouts and keep-alive, base URL, HTTP version, TLS, certificate pinning, root cert source, mTLS, proxy, redirects, DNS, cookies, user agent.

All settings go in `ClientSettings(...)`, passed to `RhttpClient.create(settings: ...)` or to a single request via `settings:`.

```dart
const ClientSettings(
  baseUrl: 'https://api.example.com',
  userAgent: 'MyApp/1.0',
  throwOnStatusCode: true,                 // default
  httpVersionPref: HttpVersionPref.http3,
  timeoutSettings: TimeoutSettings(...),
  tlsSettings: TlsSettings(...),
  proxySettings: ProxySettings.noProxy(),
  redirectSettings: RedirectSettings.limited(5),
  dnsSettings: DnsSettings.static(overrides: {...}),
  cookieSettings: CookieSettings(storeCookies: true),
);
```

## Timeouts and keep-alive

`timeout` and `connectTimeout` live in `TimeoutSettings` (moved from `ClientSettings` in 0.8.0; the old top-level fields no longer exist).

```dart
timeoutSettings: TimeoutSettings(
  timeout: Duration(seconds: 10),
  connectTimeout: Duration(seconds: 5),
  keepAliveTimeout: Duration(seconds: 60), // > 0 keeps idle sockets open (HTTP/1.1 and 2)
  keepAlivePing: Duration(seconds: 30),
)
```

By default sockets close right after the last request. On HTTP/2, concurrent requests share one connection.

## HTTP version

`HttpVersionPref.http3` (and the http1 / http1_1 / http2 variants). Only a preference; the server must support it.

## TLS

```dart
tlsSettings: TlsSettings(
  minTlsVersion: TlsVersion.tls12,
  maxTlsVersion: TlsVersion.tls13,   // only 1.2 and 1.3 supported
  sni: true,                          // default true
  verifyCertificates: true,           // false = insecure, testing only
)
```

## Certificate pinning

Provide the root certificate or the full chain (a reqwest limitation prevents pinning only a leaf). PEM strings:

```dart
tlsSettings: TlsSettings(
  trustedRootCertificates: ['''-----BEGIN CERTIFICATE-----
...
-----END CERTIFICATE-----'''],
)
```

## Root certificate source (0.18.0+)

`TlsSettings.rootCertSource` replaced the old `trustRootCertificates` flag in 0.18.0 (breaking).

| Value                     | Meaning                                                                          |
| ------------------------- | -------------------------------------------------------------------------------- |
| `RootCertSource.webpki`   | Mozilla bundle, default, consistent everywhere, updates only with the package    |
| `RootCertSource.platform` | OS trust store, needed for private or corporate CAs, behavior varies by platform |
| `RootCertSource.none`     | trust nothing except what you provide                                            |

If using `platform` on Android release builds, add the Proguard rule (see `setup-troubleshooting.md`).

## Mutual TLS

```dart
tlsSettings: TlsSettings(
  clientCertificate: ClientCertificate(certificate: certPem, privateKey: keyPem),
)
```

## Proxy

System proxy is on by default.

```dart
ProxySettings.noProxy()
ProxySettings.proxy('http://localhost:8080')
ProxySettings.static(url: 'http://localhost:8080', condition: ProxyCondition.onlyHttp)
ProxySettings.list([
  StaticProxy(url: 'http://localhost:8080', condition: ProxyCondition.onlyHttp),
  StaticProxy(url: 'http://localhost:8081', condition: ProxyCondition.onlyHttps),
])
```

## Redirects

Default: follow up to 10, then `RhttpRedirectException`.

```dart
redirectSettings: RedirectSettings.limited(5)   // or RedirectSettings.none()
```

## DNS

```dart
dnsSettings: DnsSettings.static(overrides: {
  'example.com': ['127.0.0.1'],
  'api.example.com': ['127.0.0.1:8080'],   // optional port
})

dnsSettings: DnsSettings.dynamic(resolver: (String host) async => ['1.2.3.4'])
```

## Cookies

Opt-in, stored in an in-memory reqwest jar (not persisted across app restarts):

```dart
cookieSettings: CookieSettings(storeCookies: true)
```

## User-Agent

`userAgent: 'MyApp/1.0'` sets the header for every request from the client.
