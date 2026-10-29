import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/socket_client.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/connection_credentials.dart';

/// The account's realtime lifecycle: install, start, stop.
///
/// # Why it exists
///
/// Nothing used to call `connect()` or `install()` in production, so delivery
/// began — if it began at all — only when some screen happened to subscribe
/// (audit H06). This owner is the one place that orders the three steps:
/// adapters first, then a connection, and a stop that completes only when
/// delivery has actually ended.
///
/// # The token is never held
///
/// [accessToken] reads the committed credential on every call. A socket
/// therefore authenticates a reconnect with the token as it is *now*, and an
/// account switch can never reuse the previous account's token (audit M03).
///
/// No media state, no credential stream, no restart on refresh: a WebSocket
/// authenticates at its handshake, so a new token matters at the next connect.
final class AccountConnectionOwner implements Disposable {
  new({
    required this._context,
    required this._socket,
    required this._adapters,
    required this._credentials,
    required this._logger,
    required this._clock,
  });

  final AccountContext _context;
  final SocketEndpoint _socket;
  final List<ConnectionAdapter> _adapters;
  final ConnectionCredentials _credentials;
  final MenoLogger _logger;
  final Clock _clock;

  /// Whether [stop] has begun. Once true it never goes back: a late [start]
  /// publishes nothing.
  bool _stopping = false;

  /// Whether [initialize] attached the adapters.
  bool _installed = false;

  /// The memoised stop, so two callers await one completion.
  Future<void>? _stopResult;

  /// The account this owner belongs to.
  AccountContext get context => _context;

  /// Whether delivery is stopping (or stopped).
  bool get isStopping => _stopping;

  /// The token a connect or reconnect should present, read now.
  ///
  /// Null when the account has no credential on this device, no token, or one
  /// that has already expired — a socket that presented an expired token would
  /// be closed as unauthorised and retried for nothing.
  Future<String?> accessToken() async {
    final credential = _credentials.usableCredential(_context.accountId);
    if (credential == null || credential.accessToken.isEmpty) return null;

    if (!credential.accessExpiresAt.isAfter(_clock.nowUtc())) return null;

    return credential.accessToken;
  }

  /// Attaches every adapter's handlers.
  ///
  /// Before any connection: a socket that opens first can deliver its first
  /// frames to nobody, and the event is gone when a handler arrives later.
  void initialize() {
    if (_stopping || _installed) return;
    _installed = true;

    for (final adapter in _adapters) {
      try {
        adapter.install();
      } on Object catch (error, stackTrace) {
        _logger.w(
          'A connection adapter could not be installed',
          tag: MenoLogTag.network.label,
          error: error,
          stackTrace: stackTrace,
        );
      }
    }
  }

  /// Starts delivery once the account is ready and its address verified.
  ///
  /// A refused start is not a sign-out: the HTTP session stays valid and the
  /// screens keep working, so the failure is logged rather than thrown.
  Future<void> start({required bool verified}) async {
    if (_stopping) return;

    initialize();
    if (_stopping || !verified) {
      _logger.i(
        'Realtime delivery is waiting for a verified account',
        tag: MenoLogTag.network.label,
      );
      return;
    }

    try {
      await _socket.start();
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Realtime delivery could not be started',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }

    // A stop that raced the start wins: nothing is published after it.
    if (_stopping) await _socket.stop();
  }

  /// Stops delivery and detaches the adapters, once.
  ///
  /// A second call joins the first future, so an account exit can await this
  /// and be certain no frame arrives afterwards.
  Future<void> stop() => _stopResult ??= _stop();

  Future<void> _stop() async {
    _stopping = true;

    await _socket.stop();

    for (final adapter in _adapters) {
      try {
        await Future<void>.sync(adapter.detach);
      } on Object catch (error, stackTrace) {
        _logger.w(
          'A connection adapter could not be detached',
          tag: MenoLogTag.network.label,
          error: error,
          stackTrace: stackTrace,
        );
      }
    }
  }

  @override
  FutureOr<dynamic> onDispose() async {
    await stop();
    _adapters.clear();
  }
}
