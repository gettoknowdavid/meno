//
// ignore_for_file: discarded_futures

import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/session_service.dart';

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

  final SessionService store;
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
  final Map<_RenewalKey, Future<RefreshOutcome<CredentialRecord>>> _renewals =
      {};

  @override
  Future<void> onRequest(
    RequestOptions options,
    RequestInterceptorHandler handler,
  ) async {
    final done = _CompleteOnce();
    try {
      // A token only ever comes from the vault, and a request can arrive
      // before startup finished waiting on it — initialize() is idempotent.
      await store.initialize();

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
      CredentialRecord credential;

      if (previous is _Identity) {
        identity = previous;
        final byId = store.usableCredential(identity.accountId);
        if (byId == null) {
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
        final activeId = store.readSnapshot().activeAccountId;
        final captured = activeId == null
            ? null
            : store.usableCredential(activeId);
        if (captured == null) {
          return done.run(
            () => _rejectRequest(
              options,
              handler,
              const SessionUnavailableException(),
            ),
          );
        }
        identity = _Identity(captured.accountId);
        credential = captured;
        options.extra[identityKey] = identity;
      }

      if (_needsRefresh(credential)) {
        final outcome = await _renew(credential);
        switch (outcome) {
          case RefreshSucceeded(credential: final issued):
            if (!_isStillActive(identity)) {
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
            // The server documented the session as dead, and settling the
            // outcome removed the record: the account is signed out rather than
            // left to fail every later request one at a time.
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
            // sign in again, and the same token must never be sent twice. The
            // vault has already marked the record unusable, which is what makes
            // the projection sign the account out.
            logger.w(
              'Session renewal for ${identity.accountId.orEmpty} is '
              'uncertain ($reason); the account must sign in again',
              tag: MenoLogTag.auth.label,
            );
            return done.run(
              () => _rejectRequest(
                options,
                handler,
                const SessionUnavailableException(),
              ),
            );
        }
      }

      options.headers[authorizationHeader] = 'Bearer ${credential.accessToken}';
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
      if (!_isStillActive(identity)) {
        return done.run(
          () => _rejectResponse(response, handler, const CancelledException()),
        );
      }

      final stored = _recordOf(identity.accountId);
      if (stored == null) {
        // The record went away while this request was in flight, so there is
        // nothing left to renew the refusal with.
        return done.run(
          () => _rejectResponse(
            response,
            handler,
            const SessionUnavailableException(),
          ),
        );
      }

      final outcome = await _renew(stored);

      switch (outcome) {
        case RefreshSucceeded(credential: final issued):
          if (!_isStillActive(identity)) {
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
          final replayed = await _replay(request, issued.accessToken);
          return done.run(() => handler.resolve(replayed));

        case RefreshRejected():
          // Settling the outcome dropped the record, so the account is signed
          // out by the projection; nothing else has to be cleared here.
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
            'uncertain ($reason); the account must sign in again',
            tag: MenoLogTag.auth.label,
          );
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

  /// The committed record for [accountId], or null when there is none or it
  /// must not be spent again.
  ///
  /// Unlike `usableCredential` this keeps an expired record — renewing it is
  /// what the interceptor is for — but still refuses an uncertain one, whose
  /// refresh token may already have been consumed.
  CredentialRecord? _recordOf(Id accountId) {
    final record = store.readSnapshot().credentials[accountId];
    if (record == null || record.renewalState == RenewalState.uncertain) {
      return null;
    }
    return record;
  }

  /// Whether [credential]'s access token is past its expiry.
  bool _isExpired(CredentialRecord credential) =>
      !credential.accessExpiresAt.isAfter(clock.nowUtc());

  /// Whether [credential] is inside the renewal window.
  bool _needsRefresh(CredentialRecord credential) {
    final refreshToken = credential.refreshToken;
    if (refreshToken == null || refreshToken.isEmpty) return false;
    final threshold = clock.nowUtc().add(refreshMargin);
    return !credential.accessExpiresAt.isAfter(threshold);
  }

  /// Whether [identity] is still the account the app acts as.
  bool _isStillActive(_Identity identity) =>
      store.readSnapshot().activeAccountId == identity.accountId;

  /// Renews [stale], sharing one attempt — and one classified outcome — between
  /// every caller holding the same version of that record.
  ///
  /// The key is the whole identity, not just the account: a caller holding an
  /// older revision must not join an attempt that already advanced it, or the
  /// same single-use refresh token would be presented twice.
  Future<RefreshOutcome<CredentialRecord>> _renew(CredentialRecord stale) {
    final key = _RenewalKey(stale.accountId, stale.epoch, stale.revision);
    final inFlight = _renewals[key];
    if (inFlight != null) return inFlight;

    final renewal = _performRenewal(stale);
    _renewals[key] = renewal;
    unawaited(
      renewal.whenComplete(() {
        if (identical(_renewals[key], renewal)) {
          _renewals.remove(key);
        }
      }),
    );
    return renewal;
  }

  /// One attempt, ordered so a possibly spent token is never presented twice.
  Future<RefreshOutcome<CredentialRecord>> _performRenewal(
    CredentialRecord stale,
  ) async {
    final accountId = stale.accountId;
    final epoch = stale.epoch;
    final revision = stale.revision;

    // A newer revision is already committed: that pair is the answer, and the
    // token this attempt would have presented is already spent.
    final current = _recordOf(accountId);
    if (current != null && current.revision > revision) {
      return RefreshSucceeded(current);
    }

    final refreshToken = stale.refreshToken;
    if (refreshToken == null || refreshToken.isEmpty) {
      return const RefreshUncertain('missingRefreshToken');
    }

    // Written down before the request leaves: a crash from here on reads back
    // as an uncertain outcome, never as a token that is safe to spend again.
    final marked = await store.markRenewalInFlight(
      accountId,
      epoch: epoch,
      revision: revision,
    );
    if (marked is! MutationCommitted) {
      return const RefreshUncertain('dispatchNotPrepared');
    }

    final outcome = await _exchange(accountId, refreshToken);

    // Applied inside the vault's queue and only while the record is still the
    // one this attempt started from.
    final settled = await store.settleRenewal(
      accountId,
      epoch: epoch,
      revision: revision,
      outcome: outcome,
    );
    switch (settled) {
      case MutationCommitted():
        return outcome;
      case MutationStale():
        // The record moved on — a logout, a switch or a newer login — while
        // this attempt was in flight, so nothing was written.
        return const RefreshUncertain('renewalSuperseded');
      case MutationStorageFailed():
      case MutationRejected():
        return const RefreshUncertain('renewalNotSettled');
    }
  }

  /// The network half: exchanges [refreshToken] and classifies what came back.
  ///
  /// Classification is deliberate, never a guess: a lost response must never
  /// become "unavailable" (the token may already be spent), and an unclassified
  /// non-success must never become "outage" (the backend rotates the token
  /// before its later fallible work).
  Future<RefreshOutcome<CredentialRecord>> _exchange(
    Id accountId,
    String refreshToken,
  ) async {
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

        // Mapped at receipt, once, against this reading of the clock (Step 6).
        final renewed = AuthResponseDto.fromJson(payload)
            .toCredentialRecord(clock);
        if (renewed.accountId != accountId) {
          // A pair minted for another identity is never adopted.
          return const RefreshUncertain('accountMismatch');
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
    handler.reject(DioException(requestOptions: options, error: error));
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
    );
  }
}

/// The account a request belongs to, captured at dispatch.
final class _Identity {
  const new(this.accountId);

  final Id accountId;
}

/// One renewal attempt, named by everything that makes it conditional.
@immutable
final class _RenewalKey {
  const new(this.accountId, this.epoch, this.revision);

  final Id accountId;
  final String epoch;
  final int revision;

  @override
  bool operator ==(Object other) =>
      other is _RenewalKey &&
      other.accountId == accountId &&
      other.epoch == epoch &&
      other.revision == revision;

  @override
  int get hashCode => Object.hash(accountId, epoch, revision);
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
