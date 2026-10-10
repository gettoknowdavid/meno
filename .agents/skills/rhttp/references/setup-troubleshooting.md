# rhttp setup, troubleshooting, migration

Contents: build prerequisites, Android notes, Proguard for platform certs, common runtime errors, version migration table, doc sources.

## Build prerequisites

- Rust via rustup, 1.80.0 or newer. The package respects `rust-toolchain.toml` and installs the pinned version automatically (0.11.1+).
- Android: command-line tools plus the latest NDK (upstream issue #44). 16 KB page alignment is supported since 0.14.0.
- The first build compiles Rust and is slow; Rust is always built in release mode.
- Android needs the `INTERNET` permission in `AndroidManifest.xml`.
- Web is unsupported. Guard with `kIsWeb` and fall back to `package:http` if the app also targets web.

## Android Proguard (only for `RootCertSource.platform`)

R8 strips the Java classes `rustls-platform-verifier` needs. Add to `android/app/proguard-rules.pro`:

```
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
```

If the file did not exist, also reference it in `android/app/build.gradle.kts`:

```kotlin
buildTypes {
    release {
        proguardFiles(
            getDefaultProguardFile("proguard-android-optimize.txt"),
            "proguard-rules.pro"
        )
    }
}
```

`webpki` (default) and `none` need no Proguard config.

## Common problems

| Symptom                                                  | Likely cause and fix                                                                                                 |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Crash or "not initialized" on first request              | `await Rhttp.init()` missing in `main()`                                                                             |
| Rust/cargo not found at build                            | install rustup; restart IDE so PATH updates                                                                          |
| NDK or cmdline-tools errors on Android                   | install latest NDK and command-line tools via SDK Manager                                                            |
| TLS failures behind corporate proxy or private CA        | default webpki bundle ignores OS store; use `RootCertSource.platform` (+ Proguard rule) or `trustedRootCertificates` |
| Certificate pinning does nothing with a leaf cert        | supply root cert or full chain                                                                                       |
| `RhttpClientDisposedException`                           | client used after `dispose()`; do not share a disposed client                                                        |
| 401/404 throws instead of returning                      | default `throwOnStatusCode: true`; set false or catch `RhttpStatusCodeException`                                     |
| Second client seems to overwrite first                   | fixed in 0.7.0; upgrade                                                                                              |
| Timeout setting not found                                | `timeout` and `connectTimeout` moved to `TimeoutSettings` (0.8.0)                                                    |
| `trustRootCertificates` undefined                        | replaced by `rootCertSource` in 0.18.0                                                                               |
| Android release builds fail TLS only with platform certs | missing Proguard rule                                                                                                |
| Works on mobile, fails on web                            | web unsupported                                                                                                      |

## Migration notes (breaking and notable changes)

| Version | Change                                                                               |
| ------- | ------------------------------------------------------------------------------------ |
| 0.5.0   | `requestGeneric` renamed to `request`; old `request` removed                         |
| 0.7.0   | `RhttpInvalidClientException` renamed `RhttpClientDisposedException`                 |
| 0.8.0   | `timeout`/`connectTimeout` moved into `TimeoutSettings`                              |
| 0.10.0  | switched to `rustls-tls-webpki-roots` to avoid corrupted system cert stores          |
| 0.11.0  | `HttpMethod` accepts any string; `ClientSettings.userAgent` added                    |
| 0.14.0  | `queryRaw` parameter; `RhttpCompatibleClient` supports `Abortable`                   |
| 0.15.0  | compat client joins duplicate header keys with commas; non-UTF8 header crash fixed   |
| 0.17.0  | reqwest 0.13.3; required extra Android setup (no longer needed in 0.18.0)            |
| 0.18.0  | `trustRootCertificates` replaced by `rootCertSource`; webpki roots behavior restored |

When a user reports an unknown symbol, check this table first, then the changelog at https://pub.dev/packages/rhttp/changelog.

## Sources

- README: https://pub.dev/packages/rhttp
- API reference: https://pub.dev/documentation/rhttp/latest/
- Repo (Codeberg; GitHub copy is archived): https://codeberg.org/Tienisto/rhttp
- Riverpod token refresh example: `rhttp/example/lib/interceptor_riverpod.dart` in the repo
