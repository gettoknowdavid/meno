//
// ignore_for_file: discarded_futures

import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter/services.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/session_store.dart';

/// Decides what credential each request travels with, and never changes that
/// decision once the request is in flight.
///
/// Four rules, each of which was a defect before:
///
/// * **The credential is chosen from the request's [AuthMode], not its path.**
///   A path string says nothing about identity.
/// * **A request belongs to the account that was active when it was
///   dispatched.** The account id is captured before the first network await,
///   stored on the request, and reused by later attempts — a wrapper retry must
///   never recapture. A request started as A can therefore never be replayed
///   with B's token after a switch; it ends as a typed cancellation instead.
/// * **A token only ever goes to the approved API origin.** Anything else is
///   refused before a byte is sent, and redirects are never followed.
/// * **Every hook completes exactly once**, with a single decision — pass,
///   resolve, or reject-with-this-reason — chosen in that invocation's own
///   stack frame. A storage exception can no longer leave a caller waiting
///   forever, and a replay's transport failure surfaces as that failure rather
///   than as the original 401.
///
/// Refresh tokens are single-use and rotated by the API, so every renewal is
/// classified ([RefreshOutcome]) before anything acts on it, shared per
/// account, and never retried when its outcome is uncertain.
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
  ///
  /// One dispatch per account: N concurrent 401s spend exactly one refresh
  /// token, and every caller receives the same classified outcome.
  final Map<Id, Future<RefreshOutcome<UserCredential>>> _renewals = {};

  @override
  Future<void> onRequest(
    RequestOptions options,
    RequestInterceptorHandler handler,
  ) async {
    final done = _CompleteOnce();
    try {
      final mode = options.extra[AuthMode.extraKey];

      switch (mode) {
        case AnonymousAuth():
          // Nothing is attached, and anything a caller attached by hand is
          // removed: an anonymous request is anonymous whatever it was handed.
          _stripAuthorization(options);
          return done.run(() => handler.next(options));

        case BearerAuth(:final token):
          // One captured token, once. No refresh, no replay, approved origin
          // only.
          if (!config.isApprovedApiOrigin(options.uri)) {
            return done.run(
              () => _rejectRequest(
                options,
                handler,
                const WrongOriginException(),
              ),
            );
          }
          if (token.isEmpty) {
            return done.run(
              () => _rejectRequest(
                options,
                handler,
                const SessionUnavailableException(),
              ),
            );
          }
          options.headers[authorizationHeader] = 'Bearer $token';
          return done.run(() => handler.next(options));

        case AccountAuth():
          break;

        case null:
          // A request that did not state its identity gets none. Silently
          // attaching the active account's token is the behaviour that leaked
          // credentials to whatever URL a caller happened to pass.
          _stripAuthorization(options);
          return done.run(() => handler.next(options));
      }

      if (!config.isApprovedApiOrigin(options.uri)) {
        return done.run(
          () => _rejectRequest(options, handler, const WrongOriginException()),
        );
      }

      // A request that already carries a captured identity keeps it: a wrapper
      // retry re-enters this hook, and recapturing would let it pick up
      // whichever account is active *now* instead of the one it started as.
      final previous = options.extra[identityKey];
      final _Identity identity;
      UserCredential credential;

      if (previous is _Identity) {
        identity = previous;
        final byId = await store.readCredential(identity.accountId);
        if (byId == null || !byId.hasUsableSession) {
          return done.run(
            () => _rejectRequest(
              options,
              handler,
              const SessionUnavailableException(),
            ),
          );
        }
        credential = byId;
      } else {
        // The capture: everything after this point uses [identity], never
        // "whichever account is active now".
        final captured = await store.readActiveCredential();
        if (captured == null || !captured.hasUsableSession) {
          return done.run(
            () => _rejectRequest(
              options,
              handler,
              const SessionUnavailableException(),
            ),
          );
        }
        identity = _Identity(captured.userId);
        credential = captured;
        options.extra[identityKey] = identity;
      }

      if (_isBeyondRecovery(credential)) {
        return done.run(
          () => _rejectRequest(
            options,
            handler,
            const SessionUnavailableException(),
          ),
        );
      }

      if (_needsRefresh(credential)) {
        final outcome = await _renew(identity.accountId, credential);
        switch (outcome) {
          case RefreshSucceeded(credential: final issued):
            if (!await _isStillActive(identity)) {
              return done.run(
                () => _rejectRequest(
                  options,
                  handler,
                  const CancelledException(),
                ),
              );
            }
            credential = issued;
          case RefreshRejected():
            // The server documented the session as dead. Sign it out rather
            // than leaving every later request to fail one at a time.
            if (await _isStillActive(identity)) await store.clearActive();
            return done.run(
              () => _rejectRequest(
                options,
                handler,
                const SessionUnavailableException(),
              ),
            );
          case RefreshUnavailable(:final retryAfter):
            // Nothing was spent: the stored pair stays the truth. The request
            // may still go out on the access token while it is valid; if it is
            // already dead, the caller gets the retryable typed failure.
            if (_isExpired(credential)) {
              return done.run(
                () => _rejectRequest(
                  options,
                  handler,
                  UpstreamUnavailableException(
                    message: 'The session could not be renewed right now.',
                    retryAfter: retryAfter,
                  ),
                ),
              );
            }
          case RefreshUncertain(:final reason):
            // The refresh token may already have been spent: this account must
            // sign in again, and the same token must never be sent twice.
            logger.w(
              'Session renewal for ${identity.accountId.orEmpty} is '
              'uncertain ($reason); signing the account out',
              tag: MenoLogTag.auth.label,
            );
            if (await _isStillActive(identity)) await store.clearActive();
            return done.run(
              () => _rejectRequest(
                options,
                handler,
                const SessionUnavailableException(),
              ),
            );
        }
      }

      options.headers[authorizationHeader] =
          'Bearer ${credential.session.accessToken}';
      return done.run(() => handler.next(options));
    } on Object catch (error, stackTrace) {
      // Anything unexpected — a storage failure, a bug in here — still
      // completes this request exactly once, with the classified reason.
      logger.e(
        'Auth dispatch failed before the request was sent',
        tag: MenoLogTag.auth.label,
        error: error,
        stackTrace: stackTrace,
      );
      return done.run(() => _rejectRequest(options, handler, _classify(error)));
    } finally {
      // No path may fall off the end without a decision.
      done.run(
        () => _rejectRequest(
          options,
          handler,
          const SessionUnavailableException(),
        ),
      );
    }
  }

  @override
  Future<void> onResponse(
    Response<dynamic> response,
    ResponseInterceptorHandler handler,
  ) async {
    final request = response.requestOptions;
    final done = _CompleteOnce();
    try {
      if (request.extra[AuthMode.extraKey] is! AccountAuth) {
        return done.run(() => handler.next(response));
      }
      if (response.statusCode != 401 || request.extra[retriedKey] == true) {
        return done.run(() => handler.next(response));
      }

      final identity = request.extra[identityKey];
      if (identity is! _Identity) {
        // No captured identity means the request was not dispatched by this
        // policy, so there is nothing to bind a renewal to.
        return done.run(() => handler.next(response));
      }

      // The request belongs to an account that is no longer active: the switch
      // already invalidated it. Refuse instead of renewing somebody else's
      // session or replaying with the new account's token.
      if (!await _isStillActive(identity)) {
        return done.run(
          () => _rejectResponse(response, handler, const CancelledException()),
        );
      }

      final stored = await store.readCredential(identity.accountId);
      final outcome = await _renew(
        identity.accountId,
        stored ?? UserCredential.empty,
      );

      switch (outcome) {
        case RefreshSucceeded(credential: final issued):
          if (!await _isStillActive(identity)) {
            return done.run(
              () => _rejectResponse(
                response,
                handler,
                const CancelledException(),
              ),
            );
          }
          logger.d(
            'Renewed the session for ${identity.accountId.orEmpty}; '
            'replaying the request',
            tag: MenoLogTag.network.label,
          );
          final replayed = await _replay(request, issued.session.accessToken);
          return done.run(() => handler.resolve(replayed));

        case RefreshRejected():
          if (await _isStillActive(identity)) await store.clearActive();
          return done.run(
            () => _rejectResponse(
              response,
              handler,
              const SessionUnavailableException(),
            ),
          );

        case RefreshUnavailable():
          // Nothing was rotated and the refusal was real: the original 401 is
          // the honest answer, and the pair stays usable for later requests.
          return done.run(() => handler.next(response));

        case RefreshUncertain(:final reason):
          logger.w(
            'Session renewal for ${identity.accountId.orEmpty} is '
            'uncertain ($reason); signing the account out',
            tag: MenoLogTag.auth.label,
          );
          if (await _isStillActive(identity)) await store.clearActive();
          return done.run(
            () => _rejectResponse(
              response,
              handler,
              const SessionUnavailableException(),
            ),
          );
      }
    } on Object catch (error, stackTrace) {
      // A replay that could not be sent surfaces *its own* failure — offline
      // must not look like bad credentials — and a storage failure no longer
      // escapes without completing this handler.
      logger.e(
        'Auth dispatch failed while handling a refusal',
        tag: MenoLogTag.auth.label,
        error: error,
        stackTrace: stackTrace,
      );
      return done.run(
        () => _rejectResponse(response, handler, _classify(error)),
      );
    } finally {
      done.run(
        () => _rejectResponse(
          response,
          handler,
          const SessionUnavailableException(),
        ),
      );
    }
  }

  /// Replays [request] on the bare client with [token], never refreshing or
  /// replaying again on its behalf.
  ///
  /// A second 401 simply resolves: the caller's own decode turns it into a
  /// typed refusal. A transport failure throws, so the caller sees "offline",
  /// not "bad credentials".
  Future<Response<dynamic>> _replay(RequestOptions request, String token) {
    request.headers[authorizationHeader] = 'Bearer $token';
    request.extra[retriedKey] = true;
    return _dio.fetch<dynamic>(request);
  }

  /// Whether [credential] is expired with no way back.
  bool _isBeyondRecovery(UserCredential credential) {
    if (!_isExpired(credential)) return false;
    return !credential.session.isRefreshable;
  }

  /// Whether [credential]'s access token is past its expiry.
  bool _isExpired(UserCredential credential) {
    final expiry = credential.session.expiresAt;
    if (expiry == null) return false;
    return !expiry.isAfter(clock.nowUtc());
  }

  /// Whether [credential] is inside the renewal window.
  bool _needsRefresh(UserCredential credential) {
    if (!credential.session.isRefreshable) return false;
    final expiry = credential.session.expiresAt;
    if (expiry == null) return false;
    return !expiry.isAfter(clock.nowUtc().add(refreshMargin));
  }

  /// Whether [identity] is still the account the app acts as.
  Future<bool> _isStillActive(_Identity identity) async =>
      await store.readActiveUserId() == identity.accountId;

  /// Renews the session of the account [accountId], sharing one attempt — and
  /// one classified outcome — between every caller for that account.
  Future<RefreshOutcome<UserCredential>> _renew(
    Id accountId,
    UserCredential stale,
  ) {
    final inFlight = _renewals[accountId];
    if (inFlight != null) return inFlight;

    final renewal = _performRenewal(accountId, stale);
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

  /// Exchanges the captured account's refresh token for a fresh pair and
  /// classifies what came back.
  ///
  /// Classification is deliberate, never a guess: a lost response must never
  /// become "unavailable" (the token may already be spent), and an unclassified
  /// non-success must never become "outage" (the backend rotates the token
  /// before its later fallible work).
  Future<RefreshOutcome<UserCredential>> _performRenewal(
    Id accountId,
    UserCredential stale,
  ) async {
    final refreshToken = stale.session.refreshToken;
    if (refreshToken == null || refreshToken.isEmpty) {
      return const RefreshUncertain('missingRefreshToken');
    }

    try {
      final response = await _dio.post<dynamic>(
        refreshPath,
        data: {'refresh_token': refreshToken},
      );

      final status = response.statusCode;
      if (status == 200) {
        final data = response.data;
        if (data is! Map<String, dynamic>) {
          return const RefreshUncertain('malformedSuccess');
        }
        final envelope = MenoResponse<dynamic>.fromJson(
          data,
          (payload) => payload,
        );
        final payload = envelope.data;
        if (!envelope.status || payload is! Map<String, dynamic>) {
          return const RefreshUncertain('malformedSuccess');
        }

        final renewed = AuthResponseDto.fromJson(payload).toDomain;
        if (renewed.userId != accountId) {
          // A pair minted for another identity is never adopted.
          return const RefreshUncertain('accountMismatch');
        }

        // The account may have been switched away from while the renewal was
        // in flight. The new tokens are still worth keeping, but not as the
        // active account.
        if (await _isStillActive(_Identity(accountId))) {
          await store.saveActive(renewed);
        } else {
          await store.saveToVault(renewed);
        }
        return RefreshSucceeded(renewed);
      }

      if (status == 429) {
        // Rate-limited before rotation: nothing was spent.
        return RefreshUnavailable(retryAfter: _retryAfter(response));
      }

      if (status == 401) {
        return const RefreshRejected();
      }

      if (status == 400 && _namesTokenRefusal(response.data)) {
        return const RefreshRejected();
      }

      // Everything else — a 5xx, an unclassified body — may have rotated the
      // token before failing. Never retried.
      return const RefreshUncertain('unclassifiedStatus');
    } on DioException catch (error) {
      return switch (error.type) {
        // Provably never reached a handler: the pair is untouched.
        DioExceptionType.connectionTimeout ||
        DioExceptionType.sendTimeout ||
        DioExceptionType.connectionError => const RefreshUnavailable(),
        // The request may have been dispatched and its answer lost: the token
        // may already be spent.
        DioExceptionType.receiveTimeout => const RefreshUncertain(
          'responseLost',
        ),
        DioExceptionType.cancel => const RefreshUncertain('cancelled'),
        _ => const RefreshUncertain('transportFailure'),
      };
    } on Object catch (error, stackTrace) {
      logger.e(
        'Session renewal for ${accountId.orEmpty} could not be read',
        tag: MenoLogTag.auth.label,
        error: error,
        stackTrace: stackTrace,
      );
      return const RefreshUncertain('unreadableResponse');
    }
  }

  /// Whether a 400 body names the refresh token itself as the problem.
  bool _namesTokenRefusal(Object? data) {
    if (data is! Map<String, dynamic>) return false;
    final code = data['code'];
    if (code is! String) return false;
    final upper = code.toUpperCase();
    return upper.contains('TOKEN') || upper.contains('REFRESH');
  }

  /// The server's retry hint, when it sent one.
  Duration? _retryAfter(Response<dynamic> response) {
    final header = response.headers.value('retry-after');
    final seconds = header == null ? null : int.tryParse(header);
    return seconds == null ? null : Duration(seconds: seconds);
  }

  /// Turns anything thrown inside a hook into the typed reason the caller
  /// receives.
  MenoException _classify(Object error) => switch (error) {
    MenoException() => error,
    DioException() => _mapDioError(error),
    PlatformException() => const StorageException(),
    _ => const UnknownException(),
  };

  /// The failure a transport error describes, keeping a typed reason the
  /// interceptor already chose.
  MenoException _mapDioError(DioException error) {
    final reason = error.error;
    if (reason is MenoException) return reason;
    return switch (error.type) {
      DioExceptionType.connectionTimeout ||
      DioExceptionType.sendTimeout ||
      DioExceptionType.receiveTimeout => RequestTimeoutException(cause: error),
      DioExceptionType.connectionError => NetworkException(cause: error),
      DioExceptionType.badCertificate => const InsecureConnectionException(),
      DioExceptionType.cancel => const CancelledException(),
      _ => UnknownException(cause: error.error ?? error),
    };
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
    handler.reject(
      DioException(
        requestOptions: options,
        error: error,
        type: DioExceptionType.unknown,
      ),
    );
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
        type: DioExceptionType.unknown,
      ),
    );
  }
}

/// The account a request belongs to, captured at dispatch.
final class _Identity {
  const new(this.accountId);

  final Id accountId;
}

/// Runs a dio handler exactly once.
///
/// Every hook computes its decision and completes through this guard, so a
/// late `finally`, an early `return`, or an exception in between can never
/// produce a second completion — or none at all.
final class _CompleteOnce {
  bool _done = false;

  /// Whether a decision has already been made in this invocation.
  bool get isDone => _done;

  /// Runs [completion] only if no decision has been made yet.
  void run(void Function() completion) {
    if (_done) return;
    _done = true;
    completion();
  }
}
