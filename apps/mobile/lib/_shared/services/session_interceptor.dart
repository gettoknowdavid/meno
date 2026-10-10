//
// ignore_for_file: discarded_futures

import 'dart:async';

import 'package:dio/dio.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';

/// Attaches the active account's token to every request, and renews it when the
/// API says it is no longer good.
///
/// [HttpClient] asks dio to accept every status, because the API's envelope —
/// not the status line — is what explains a failure. A `401` therefore arrives
/// as a successful response and dio's error hook never sees it. The refusal is
/// detected where it actually lands: in the response, by reading the status the
/// envelope does not repeat.
///
/// Refresh tokens are single-use and rotated by the API, so two concurrent
/// refreshes for the same account would have the second one present a token the
/// first already spent. Renewals are therefore shared: the in-flight future is
/// keyed by account id and reused until it settles, so a screen with six
/// requests in the air spends exactly one refresh token.
///
/// The tokens being refreshed belong to *an account*, not to "the app". The
/// account id is captured when the request goes out, and a renewal that lands
/// after the user switched accounts is filed in the vault for the account it
/// belongs to instead of becoming the active session. Switching accounts
/// therefore never has to wait for, or invalidate, a renewal already in flight.
final class SessionInterceptor extends Interceptor {
  /// Creates the interceptor.
  ///
  /// [baseUrl] is the API root the renewals are sent to. [refreshDio] exists
  /// for tests; when omitted a bare client is built, deliberately sharing no
  /// interceptors with [HttpClient] — a renewal that recursed through this
  /// interceptor would refresh in order to refresh.
  new({
    required this._store,
    required String baseUrl,
    required this._logger,
    this.refreshPath = defaultRefreshPath,
    Dio? refreshDio,
  }) : _dio =
           refreshDio ??
           Dio(
             BaseOptions(
               baseUrl: baseUrl,
               connectTimeout: const Duration(seconds: 30),
               receiveTimeout: const Duration(seconds: 30),
               sendTimeout: const Duration(seconds: 30),
               headers: {Headers.acceptHeader: Headers.jsonContentType},
               // Every status is inspected by hand, including the 401 a renewal
               // may be refused with.
               validateStatus: (status) => status != null,
             ),
           );

  /// Where a new token pair is bought.
  static const String defaultRefreshPath = '/auth/refresh';

  /// The header the token travels in.
  static const String authorizationHeader = 'Authorization';

  /// Marks a request as having already been replayed once.
  ///
  /// Without it, an endpoint that answers 401 to a *fresh* token would refresh
  /// and replay forever.
  static const String retriedKey = 'meno.session.retried';

  /// Paths that must never trigger a renewal.
  ///
  /// They either create a session (`login`, `register`, the Google exchange),
  /// end one (`logout`), or are how a locked-out user gets back in
  /// (`forgot-password`, `reset-password`, the one-time codes). Renewing from
  /// them would recurse, or resurrect a session the user just closed.
  /// `logout-all` is deliberately absent: it is protected, so it needs a live
  /// token and is worth renewing for rather than failing.
  static const List<String> _sessionlessPaths = [
    defaultRefreshPath,
    '/auth/login',
    '/auth/register',
    '/auth/google/mobile',
    '/auth/logout',
    '/auth/verify-email',
    '/auth/resend-otp',
    '/auth/forgot-password',
    '/auth/reset-password',
  ];

  final SessionStore _store;
  final MenoLogger _logger;
  final Dio _dio;

  /// Where a new token pair is bought.
  final String refreshPath;

  /// Renewals in flight, keyed by the account they belong to.
  final Map<String, Future<UserCredential?>> _renewals = {};

  // ======================================================================
  // REQUEST
  // ======================================================================

  @override
  Future<void> onRequest(
    RequestOptions options,
    RequestInterceptorHandler handler,
  ) async {
    if (_isSessionless(options.path)) return handler.next(options);

    var credential = await _store.readActiveCredential();
    if (credential == null || credential.session.accessToken.isEmpty) {
      // Nobody is signed in: the request goes out as an anonymous one and the
      // API decides whether that is allowed.
      return handler.next(options);
    }

    if (credential.session.shouldRefresh()) {
      final renewed = await _renew(credential);
      if (renewed != null) {
        credential = renewed;
      } else {
        // The renewal was refused, so the stored session is dead. Drop it and
        // let this request go out unauthenticated: the API's own 401 is the
        // better answer here.
        await _store.clearActive();
        return handler.next(options);
      }
    }

    options.headers[authorizationHeader] =
        'Bearer ${credential.session.accessToken}';
    return handler.next(options);
  }

  // ======================================================================
  // RESPONSE
  // ======================================================================

  @override
  Future<void> onResponse(
    Response<dynamic> response,
    ResponseInterceptorHandler handler,
  ) async {
    final request = response.requestOptions;

    if (response.statusCode != 401 ||
        _isSessionless(request.path) ||
        request.extra[retriedKey] == true) {
      return handler.next(response);
    }

    final credential = await _store.readActiveCredential();
    if (credential == null || !credential.session.isRefreshable) {
      return handler.next(response);
    }

    final renewed = await _renew(credential);
    if (renewed == null) {
      // The account cannot be renewed: it is signed out here rather than left
      // in a state where every request fails a little later.
      await _store.clearActive();
      return handler.next(response);
    }

    _logger.d(
      'Renewed the session for ${credential.userId}; replaying the request',
      tag: MenoLogTag.network.label,
    );

    // Replayed on the bare client so this interceptor is not consulted again
    // for a request that has already been through it. The header is replaced
    // with the token that was just minted.
    request.headers[authorizationHeader] =
        'Bearer ${renewed.session.accessToken}';
    request.extra[retriedKey] = true;

    try {
      handler.resolve(await _dio.fetch<dynamic>(request));
    } on DioException catch (error) {
      _logger.w(
        '''Replaying ${request.method} ${request.path} failed: ${error.type.name}''',
        tag: MenoLogTag.network.label,
      );
      // The original refusal is the better answer: the replay's transport
      // failure says nothing about why the request was rejected.
      handler.next(response);
    }
  }

  /// Renews [stale], sharing one attempt between every caller for that account.
  Future<UserCredential?> _renew(UserCredential stale) {
    final inFlight = _renewals[stale.userId];
    if (inFlight != null) return inFlight;

    final renewal = _performRenewal(stale);
    _renewals[stale.userId] = renewal;
    unawaited(
      renewal.whenComplete(() {
        if (identical(_renewals[stale.userId], renewal)) {
          _renewals.remove(stale.userId);
        }
      }),
    );
    return renewal;
  }

  /// Exchanges [stale]'s refresh token for a fresh pair.
  ///
  /// Returns null for every refusal — a spent token, an API error envelope, or a
  /// body that is not the documented shape. None is recoverable at this level,
  /// and all mean the same thing to the caller: sign in again.
  Future<UserCredential?> _performRenewal(UserCredential stale) async {
    final refreshToken = stale.session.refreshToken;
    if (refreshToken == null || refreshToken.isEmpty) return null;

    try {
      final response = await _dio.post<dynamic>(
        refreshPath,
        data: {'refresh_token': refreshToken},
      );

      final data = response.data;
      if (response.statusCode != 200 || data is! Map<String, dynamic>) {
        _logger.w(
          'Session renewal for ${stale.userId} answered '
          '${response.statusCode ?? 'no status'}',
          tag: MenoLogTag.network.label,
        );
        return null;
      }

      final envelope = MenoResponse<dynamic>.fromJson(
        data,
        (payload) => payload,
      );
      final payload = envelope.data;
      if (!envelope.status || payload is! Map<String, dynamic>) {
        _logger.w(
          'Session renewal for ${stale.userId} was refused: ${envelope.code}',
          tag: MenoLogTag.network.label,
        );
        return null;
      }

      final renewed = AuthResponseDto.fromJson(payload).toDomain;

      // The account may have been switched away from while the renewal was in
      // flight. The new tokens are still worth keeping, but not as the active
      // account.
      if (await _store.readActiveUserId() == renewed.userId) {
        await _store.saveActive(renewed);
      } else {
        await _store.saveToVault(renewed);
      }

      return renewed;
    } on DioException catch (error) {
      _logger.w(
        'Session renewal for ${stale.userId} failed: ${error.type.name}',
        tag: MenoLogTag.network.label,
      );
      return null;
    } on Object catch (error, stackTrace) {
      _logger.e(
        'Session renewal for ${stale.userId} could not be read',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
      return null;
    }
  }

  /// Whether [path] belongs to an endpoint that must not renew a session.
  ///
  /// Matched on the segment boundary rather than by `contains`, so `/auth/logout-all`
  /// is not mistaken for `/auth/logout`.
  static bool _isSessionless(String path) => _sessionlessPaths.any(
    (sessionless) => path == sessionless || path.startsWith('$sessionless/'),
  );
}
