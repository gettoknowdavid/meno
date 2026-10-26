//
// ignore_for_file: discarded_futures

import 'dart:async';

import 'package:dio/dio.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';

/// Decides what credential each request travels with, and never changes that
/// decision once the request is in flight.
///
/// Three rules, each of which was a defect before:
///
/// * **The credential is chosen from the request's [AuthMode], not its path.**
///   A path string says nothing about identity.
/// * **A request belongs to the account that was active when it was
///   dispatched.** The account id is captured before the first network await
///   and every later step — renewal, replay, refusal — uses that id. A request
///   started as A can therefore never be replayed with B's token after a
///   switch; it ends as a typed cancellation instead.
/// * **A token only ever goes to the approved API origin.** Anything else is
///   refused before a byte is sent, and redirects are never followed.
///
/// Refresh tokens are single-use and rotated by the API, so two concurrent
/// refreshes for the same account would have the second one present a token the
/// first already spent. Renewals are shared per account: the in-flight future
/// is reused until it settles.
final class SessionInterceptor extends Interceptor {
  new({
    required this.store,
    required this.config,
    required this.clock,
    required this.logger,
    this.refreshMargin = const Duration(minutes: 1),
    this.refreshPath = defaultRefreshPath,
    Dio? refreshDio,
  }) : _dio =
           refreshDio ??
           Dio(
             BaseOptions(
               baseUrl: config.menoApiUrl,
               connectTimeout: const Duration(seconds: 30),
               receiveTimeout: const Duration(seconds: 30),
               sendTimeout: const Duration(seconds: 30),
               headers: {Headers.acceptHeader: Headers.jsonContentType},
               // Every status is inspected by hand, including the 401 a renewal
               // may be refused with.
               validateStatus: (status) => status != null,
               // A redirect would move the request — and its credential — to
               // an address nobody approved.
               followRedirects: false,
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

  /// The identity captured when a request was dispatched.
  static const String identityKey = 'meno.session.identity';

  final SessionStore store;
  final Config config;
  final Clock clock;
  final MenoLogger logger;
  final Dio _dio;

  /// How long before expiry a token is renewed.
  final Duration refreshMargin;

  /// Where a new token pair is bought.
  final String refreshPath;

  /// Renewals in flight, keyed by the account they belong to.
  final Map<Id, Future<UserCredential?>> _renewals = {};

  @override
  Future<void> onRequest(
    RequestOptions options,
    RequestInterceptorHandler handler,
  ) async {
    final mode = options.extra[AuthMode.extraKey];

    switch (mode) {
      case AnonymousAuth():
        // Nothing is attached, and anything a caller attached by hand is
        // removed: an anonymous request is anonymous whatever it was handed.
        _stripAuthorization(options);
        return handler.next(options);

      case BearerAuth(:final token):
        // One captured token, once. No refresh, no replay, approved origin
        // only.
        if (!config.isApprovedApiOrigin(options.uri)) {
          return _rejectRequest(options, handler, const WrongOriginException());
        }
        if (token.isEmpty) {
          return _rejectRequest(
            options,
            handler,
            const SessionUnavailableException(),
          );
        }
        options.headers[authorizationHeader] = 'Bearer $token';
        return handler.next(options);

      case AccountAuth():
        break;

      case null:
        // A request that did not state its identity gets none. Silently
        // attaching the active account's token is the behaviour that leaked
        // credentials to whatever URL a caller happened to pass.
        _stripAuthorization(options);
        return handler.next(options);
    }

    if (!config.isApprovedApiOrigin(options.uri)) {
      return _rejectRequest(options, handler, const WrongOriginException());
    }

    // The capture: everything after this point uses [identity], never
    // "whichever account is active now".
    final captured = await store.readActiveCredential();
    if (captured == null || !captured.hasUsableSession) {
      return _rejectRequest(
        options,
        handler,
        const SessionUnavailableException(),
      );
    }

    final identity = _Identity(captured.userId);
    options.extra[identityKey] = identity;

    var credential = captured;
    if (_isBeyondRecovery(credential)) {
      return _rejectRequest(
        options,
        handler,
        const SessionUnavailableException(),
      );
    }

    if (_needsRefresh(credential)) {
      final renewed = await _renew(identity.accountId);
      if (renewed == null) {
        // The account cannot be renewed any more. Sign it out rather than
        // leaving every later request to fail one at a time.
        if (await _isStillActive(identity)) await store.clearActive();
        return _rejectRequest(
          options,
          handler,
          const SessionUnavailableException(),
        );
      }

      // A switch during the renewal means this request's identity is gone.
      // It must not adopt the new account's credential.
      if (!await _isStillActive(identity)) {
        return _rejectRequest(options, handler, const CancelledException());
      }
      credential = renewed;
    }

    options.headers[authorizationHeader] =
        'Bearer ${credential.session.accessToken}';
    return handler.next(options);
  }

  @override
  Future<void> onResponse(
    Response<dynamic> response,
    ResponseInterceptorHandler handler,
  ) async {
    final request = response.requestOptions;

    if (request.extra[AuthMode.extraKey] is! AccountAuth) {
      return handler.next(response);
    }
    if (response.statusCode != 401 || request.extra[retriedKey] == true) {
      return handler.next(response);
    }

    final identity = request.extra[identityKey];
    if (identity is! _Identity) {
      // No captured identity means the request was not dispatched by this
      // policy, so there is nothing to bind a renewal to.
      return handler.next(response);
    }

    // The request belongs to an account that is no longer active: the switch
    // already invalidated it. Refuse instead of renewing somebody else's
    // session or replaying with the new account's token.
    if (!await _isStillActive(identity)) {
      return _rejectResponse(response, handler, const CancelledException());
    }

    final renewed = await _renew(identity.accountId);
    if (renewed == null) {
      if (await _isStillActive(identity)) await store.clearActive();
      return _rejectResponse(
        response,
        handler,
        const SessionUnavailableException(),
      );
    }

    if (!await _isStillActive(identity)) {
      return _rejectResponse(response, handler, const CancelledException());
    }

    logger.d(
      'Renewed the session for ${identity.accountId.orEmpty}; '
      'replaying the request',
      tag: MenoLogTag.network.label,
    );

    // Replayed on the bare client so this interceptor is not consulted again
    // for a request that has already been through it. The header is replaced
    // with the token that was just minted for the same identity.
    request.headers[authorizationHeader] =
        'Bearer ${renewed.session.accessToken}';
    request.extra[retriedKey] = true;

    try {
      handler.resolve(await _dio.fetch<dynamic>(request));
    } on DioException catch (error) {
      logger.w(
        '''Replaying ${request.method} ${request.path} failed: ${error.type.name}''',
        tag: MenoLogTag.network.label,
      );
      // The original refusal is the better answer: the replay's transport
      // failure says nothing about why the request was rejected.
      handler.next(response);
    }
  }

  /// Whether [credential] is expired with no way back.
  ///
  /// An expired token that can be renewed is not beyond recovery; one that
  /// cannot be is a dead session, and the request must not go out anonymously
  /// in its place.
  bool _isBeyondRecovery(UserCredential credential) {
    final expiry = credential.session.expiresAt;
    if (expiry == null) return false;
    final dead = !expiry.isAfter(clock.nowUtc());
    return dead && !credential.session.isRefreshable;
  }

  /// Whether [credential] is inside the renewal window.
  bool _needsRefresh(UserCredential credential) {
    if (!credential.session.isRefreshable) return false;
    final expiry = credential.session.expiresAt;
    if (expiry == null) return false;
    return !expiry.isAfter(clock.nowUtc().add(refreshMargin));
  }

  /// Whether [identity] is still the account the app acts as.
  ///
  /// This is the epoch check the current store can answer: once another
  /// account is active, every request captured as the previous one is
  /// obsolete.
  Future<bool> _isStillActive(_Identity identity) async =>
      await store.readActiveUserId() == identity.accountId;

  /// Renews the session of the account [accountId], sharing one attempt
  /// between every caller for that account.
  Future<UserCredential?> _renew(Id accountId) {
    final inFlight = _renewals[accountId];
    if (inFlight != null) return inFlight;

    final renewal = _performRenewal(accountId);
    _renewals[accountId] = renewal;
    unawaited(
      renewal.whenComplete(() {
        if (identical(_renewals[accountId], renewal)) {
          _renewals.remove(accountId);
        }
      }),
    );
    return renewal;
  }

  /// Exchanges the captured account's refresh token for a fresh pair.
  ///
  /// Returns null for every refusal — a spent token, an API error envelope, a
  /// body that is not the documented shape, or a pair minted for a different
  /// account. None is recoverable at this level, and all mean the same thing
  /// to the caller: sign in again.
  Future<UserCredential?> _performRenewal(Id accountId) async {
    final stored = await store.readCredential(accountId);
    final refreshToken = stored?.session.refreshToken;
    if (stored == null || refreshToken == null || refreshToken.isEmpty) {
      return null;
    }

    try {
      final response = await _dio.post<dynamic>(
        refreshPath,
        data: {'refresh_token': refreshToken},
      );

      final data = response.data;
      if (response.statusCode != 200 || data is! Map<String, dynamic>) {
        logger.w(
          'Session renewal for ${accountId.orEmpty} answered '
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
        logger.w(
          'Session renewal for ${accountId.orEmpty} was refused: '
          '${envelope.code}',
          tag: MenoLogTag.network.label,
        );
        return null;
      }

      final renewed = AuthResponseDto.fromJson(payload).toDomain;
      if (renewed.userId != accountId) {
        logger.w(
          'Session renewal for ${accountId.orEmpty} answered with a '
          'credential for ${renewed.userId.orEmpty}; refusing it',
          tag: MenoLogTag.network.label,
        );
        return null;
      }

      // The account may have been switched away from while the renewal was in
      // flight. The new tokens are still worth keeping, but not as the active
      // account.
      if (await _isStillActive(_Identity(accountId))) {
        await store.saveActive(renewed);
      } else {
        await store.saveToVault(renewed);
      }

      return renewed;
    } on DioException catch (error) {
      logger.w(
        'Session renewal for ${accountId.orEmpty} failed: '
        '${error.type.name}',
        tag: MenoLogTag.network.label,
      );
      return null;
    } on Object catch (error, stackTrace) {
      logger.e(
        'Session renewal for ${accountId.orEmpty} could not be read',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
      return null;
    }
  }

  /// Removes any `Authorization` header, whatever its casing.
  static void _stripAuthorization(RequestOptions options) {
    options.headers.removeWhere(
      (key, _) => key.toLowerCase() == authorizationHeader.toLowerCase(),
    );
  }

  /// Refuses a request before it is sent.
  void _rejectRequest(
    RequestOptions options,
    RequestInterceptorHandler handler,
    MenoException error,
  ) {
    handler.reject(DioException(requestOptions: options, error: error), true);
  }

  /// Refuses an answer, carrying the reason to the caller.
  void _rejectResponse(
    Response<dynamic> response,
    ResponseInterceptorHandler handler,
    MenoException error,
  ) {
    handler.reject(
      DioException(
        requestOptions: response.requestOptions,
        response: response,
        error: error,
      ),
      true,
    );
  }
}

/// The account a request belongs to, captured at dispatch.
final class _Identity {
  const new(this.accountId);

  final Id accountId;
}
