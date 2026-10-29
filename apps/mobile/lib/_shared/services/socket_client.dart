import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/socket.dart';

/// The transport surface the connection owner drives.
///
/// A narrow seam, so the owner can be tested without a real WebSocket and the
/// client stays the only thing that knows the protocol.
abstract interface class SocketEndpoint {
  /// Connects, and keeps the connection up until [stop].
  Future<void> start();

  /// Stops the connection and every attempt to restore it.
  Future<void> stop();

  /// Whether a frame can be sent right now.
  bool get isOpen;
}

/// One adapter whose handlers must exist before the socket connects.
///
/// The broadcast adapter implements this already (`install`/`detach`); the
/// account connection owner only decides *when*.
abstract interface class ConnectionAdapter {
  /// Attaches handlers. Idempotent.
  void install();

  /// Removes them. Idempotent.
  void detach();
}

/// The app's one realtime transport: a raw WebSocket speaking the backend's
/// own framing.
///
/// # Why not Socket.IO
///
/// `GET /ws` on the API accepts raw WebSocket JSON frames — `{event, data}` —
/// and nothing else (audit §3 C04). Socket.IO is a different protocol: its
/// handshake, its framing (`42[...]`), its acknowledgements. Selecting its
/// "websocket transport" changes the carrier, not the protocol, so the two ends
/// could never have understood each other. `dart:io`'s [WebSocket] talks to the
/// server as it is.
///
/// # What this class is, and is not
///
/// It is transport only: connect, authenticate, heartbeat, bounded reconnect,
/// and dispatch. It knows nothing about accounts beyond asking for a token, nor
/// about broadcasts beyond the event names in [SocketEvent]. Deciding *when*
/// delivery should be active belongs to the account connection owner; decoding
/// a broadcast payload belongs to the feature adapter.
///
/// The credential is read from [_credentialSupplier] on every connect — never
/// captured in the constructor — because a reconnect after a token refresh must
/// present the new token, and an account switch must not reuse the old one.
final class SocketClient implements Disposable, SocketEndpoint {
  new({
    required this._uri,
    required this._credentialSupplier,
    required this._logger,
    required this._clock,
    this._pingInterval = const Duration(seconds: 25),
    this._pongTimeout = const Duration(seconds: 10),
    this._maxAttempts = 5,
    this._initialBackoff = const Duration(seconds: 1),
    this._maxBackoff = const Duration(seconds: 30),
  });

  /// The socket endpoint as configured — `wss://host/ws`, without a token.
  final Uri _uri;

  /// The account's current access token, read on connect and reconnect only.
  final FutureOr<String?> Function() _credentialSupplier;

  /// The logger used to log socket events and errors.
  final MenoLogger _logger;

  /// The clock used to measure time for backoff and heartbeat intervals.
  final Clock _clock;

  /// How often a heartbeat is sent while open.
  final Duration _pingInterval;

  /// How long the answer to one heartbeat may take before the connection is
  /// considered dead.
  final Duration _pongTimeout;

  /// Reconnect attempts before the chain gives up and reports `closed`.
  final int _maxAttempts;

  final Duration _initialBackoff;
  final Duration _maxBackoff;

  /// Close codes that mean "do not try again".
  ///
  /// `1008` is policy violation — the code a server uses when it will not
  /// accept this connection at all. `4401`/`4403` are the transport's own
  /// spellings of "unauthorised"/"forbidden" for servers that close instead of
  /// refusing the handshake.
  static const _terminalCloseCodes = {1008, 4401, 4403};

  /// Handshake refusals that will not change on their own.
  ///
  /// The API answers an unusable token with `401` and an unverified address or
  /// a refused account with `403`, all *before* upgrading (routes/ws.rs), so
  /// these arrive as `WebSocketException.httpStatusCode` rather than as close
  /// codes. `429` is deliberately absent: the reconnect window expires, so the
  /// bounded retry chain is the right response to it.
  static const _terminalHandshakeStatuses = {401, 403};

  /// Where the connection stands, as news.
  final phase = ValueNotifier<ConnectionPhase>(.idle);

  /// Why it last closed, kept so a terminal reason can be shown rather than
  /// guessed at.
  final lastClose = ValueNotifier<SocketClose?>(null);

  /// Every handler registered with [on], keyed by event.
  ///
  /// Held here rather than on the socket because a socket does not survive a
  /// reconnect: the replacement is built from scratch and handed this map, so a
  /// subscription outlives the swap.
  final _handlers = <SocketEvent, List<void Function(dynamic)>>{};

  final _reconnectedContr = StreamController<void>.broadcast();

  WebSocket? _socket;
  StreamSubscription<dynamic>? _frames;
  Timer? _retry;
  Timer? _heartbeat;
  Timer? _pong;

  /// Whether [start] has been called and [stop] has not.
  bool _active = false;

  /// Whether a heartbeat is awaiting its answer.
  bool _awaitingPong = false;

  /// Failed attempts since the last successful connection.
  int _attempt = 0;

  /// Which connection attempt owns the current socket.
  ///
  /// Bumped by every [start] and every attempt, so a connect that completes
  /// after a [stop] — or after a newer attempt — closes what it opened instead
  /// of becoming the live socket.
  int _run = 0;

  /// Published when a socket that had dropped is open again.
  ///
  /// Not the same as [phase] reaching `open`: a first connect has nothing to
  /// re-announce, while a reconnect hands the client a socket the server has no
  /// membership for, which whoever was in a room has to say again.
  Stream<void> get onReconnected => _reconnectedContr.stream;

  /// Whether a frame can be sent right now.
  @override
  bool get isOpen => phase.value == .open;

  /// Connects, and keeps the connection up until [stop].
  ///
  /// Idempotent for the current iteration: a second call while a connection is
  /// being established, open or waiting to retry does nothing.
  @override
  Future<void> start() async {
    if (_active) return;
    _active = true;
    _attempt = 0;
    await _connect();
  }

  /// The current attempt: read a token, upgrade, and wire the socket up.
  Future<void> _connect() async {
    final run = ++_run;
    final reconnecting = _attempt > 0;
    phase.value = reconnecting ? .reconnecting : .connecting;

    final String? token;
    try {
      token = await _credentialSupplier();
    } on Object catch (error, stackTrace) {
      _logger.w(
        'SocketClient: the credential supplier failed',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
      _afterFailure(
        run,
        SocketClose(
          code: null,
          reason: 'credential supplier failed',
          at: _clock.nowUtc(),
        ),
      );
      return;
    }
    if (run != _run) return;

    if (token == null || token.isEmpty) {
      // Not retried: the supplier answers the same empty value until a new
      // account is live, and a loop would only spend the API's reconnect
      // budget to learn the same thing.
      _finish(
        run,
        SocketClose(code: null, reason: 'no access token', at: _clock.nowUtc()),
      );
      return;
    }

    try {
      final socket = await WebSocket.connect(_handshake(token).toString());
      if (run != _run || !_active) {
        await socket.close();
        return;
      }

      _socket = socket;
      _frames = socket.listen(
        _onFrame,
        onDone: () => _onDisconnected(socket),
        onError: (Object error, StackTrace stackTrace) {
          _logger.w(
            'SocketClient: the frame stream failed',
            tag: MenoLogTag.network.label,
            error: error,
            stackTrace: stackTrace,
          );
        },
      );
      _startHeartbeat();

      final reconnected = reconnecting;
      _attempt = 0;
      phase.value = .open;
      _logger.i('SocketClient: connected', tag: MenoLogTag.network.label);
      if (reconnected && !_reconnectedContr.isClosed) {
        _reconnectedContr.add(null);
      }
    } on WebSocketException catch (error) {
      if (run != _run) return;
      _afterFailure(
        run,
        SocketClose(
          code: error.httpStatusCode,
          reason: error.message,
          at: _clock.nowUtc(),
        ),
        terminal: _terminalHandshakeStatuses.contains(error.httpStatusCode),
      );
    } on Object catch (error) {
      if (run != _run) return;
      _afterFailure(
        run,
        SocketClose(code: null, reason: '$error', at: _clock.nowUtc()),
      );
    }
  }

  /// The endpoint with the token in the query.
  ///
  /// The token goes in `?token=` because that is what `WsQuery` in
  /// `apps/api/src/routes/ws.rs` reads — a browser cannot set an `Authorization`
  /// header on a handshake, so the API never looks for one.
  Uri _handshake(String token) =>
      _uri.replace(queryParameters: {..._uri.queryParameters, 'token': token});

  /// Dispatches one received frame to the handlers for its event.
  void _onFrame(dynamic raw) {
    if (raw is! String) {
      // The backend sends text only; a binary frame is not ours to interpret.
      _logger.w(
        'SocketClient: dropped a binary frame',
        tag: MenoLogTag.network.label,
      );
      return;
    }

    final frame = SocketFrame.tryDecode(raw);
    if (frame == null) {
      // One sanitised line, no payload: the text is attacker-shaped.
      _logger.w(
        'SocketClient: dropped a frame that is not an event envelope',
        tag: MenoLogTag.network.label,
      );
      return;
    }

    if (frame.event == .heartbeat) {
      _awaitingPong = false;
      _pong?.cancel();
      _pong = null;
    }

    final handlers = _handlers[frame.event];
    if (handlers == null) {
      _logger.d(
        'SocketClient: no handler for ${frame.event.value}',
        tag: MenoLogTag.network.label,
      );
      return;
    }

    // Copied: a handler may cancel its own subscription while being called.
    for (final handler in List.of(handlers)) {
      try {
        handler(frame.data);
      } on Object catch (error, stackTrace) {
        _logger.w(
          'SocketClient: a handler for ${frame.event.value} threw',
          tag: MenoLogTag.network.label,
          error: error,
          stackTrace: stackTrace,
        );
      }
    }
  }

  /// Sends one frame to the server.
  ///
  /// Throws [SocketNotConnectedException] unless the socket is open, because a
  /// frame handed to a socket that is not there is a frame that silently never
  /// left. There is no acknowledgement protocol to wait on: the backend has
  /// none, and inventing one would turn "the server never answered" into a
  /// timeout instead of the real failure.
  void send(SocketEvent event, [Object? data]) {
    final socket = _socket;
    if (socket == null || phase.value != .open) {
      throw SocketNotConnectedException(
        'Cannot send "${event.value}": the socket is ${phase.value.name}',
      );
    }
    socket.add(SocketFrame(event: event, data: data).encode());
  }

  /// Subscribes to a socket event.
  ///
  /// The handler is remembered, so a socket built later — by [start] or by a
  /// reconnect — gets it too, and the returned [SocketSubscription] cancels it.
  SocketSubscription on(SocketEvent event, void Function(dynamic) handler) {
    _handlers.putIfAbsent(event, () => []).add(handler);
    return _SocketSubscription(this, event, handler);
  }

  void _off(SocketEvent event, void Function(dynamic) handler) {
    final handlers = _handlers[event];
    if (handlers == null) return;
    handlers.remove(handler);
    if (handlers.isEmpty) _handlers.remove(event);
  }

  /// Stops the connection and every attempt to restore it.
  ///
  /// Idempotent, and safe in any phase: listeners are detached before the
  /// socket is closed, so the close it provokes cannot be read as a drop worth
  /// reconnecting from.
  @override
  Future<void> stop() async {
    _active = false;
    _run++;

    _retry?.cancel();
    _retry = null;
    _stopHeartbeat();

    final frames = _frames;
    final socket = _socket;
    _frames = null;
    _socket = null;

    await frames?.cancel();
    await socket?.close(WebSocketStatus.normalClosure, 'client stopped');

    phase.value = .closed;
  }

  /// Starts the ping/answer cycle for a freshly opened socket.
  void _startHeartbeat() {
    _stopHeartbeat();
    _awaitingPong = false;
    _heartbeat = Timer.periodic(_pingInterval, (_) => _ping());
  }

  void _stopHeartbeat() {
    _heartbeat?.cancel();
    _heartbeat = null;
    _pong?.cancel();
    _pong = null;
    _awaitingPong = false;
  }

  void _ping() {
    final socket = _socket;
    if (socket == null || phase.value != .open || _awaitingPong) return;

    _awaitingPong = true;
    socket.add(
      const SocketFrame(event: .heartbeat, data: <String, Object?>{}).encode(),
    );
    _logger.v('SocketClient: heartbeat sent', tag: MenoLogTag.network.label);

    _pong = Timer(_pongTimeout, () {
      if (!_awaitingPong) return;
      final socket = _socket;
      if (socket == null) return;
      _logger.w(
        'SocketClient: no heartbeat answer within ${_pongTimeout.inSeconds}s',
        tag: MenoLogTag.network.label,
      );
      _drop(
        socket,
        SocketClose(
          code: null,
          reason: 'heartbeat timeout',
          at: _clock.nowUtc(),
        ),
      );
    });
  }

  /// The socket ended on its own — a close frame from the server, or the
  /// transport dying.
  void _onDisconnected(WebSocket socket) {
    if (!identical(socket, _socket)) return;
    _socket = null;
    _stopHeartbeat();
    unawaited(_frames?.cancel());
    _frames = null;

    _afterFailure(
      _run,
      SocketClose(
        code: socket.closeCode,
        reason: socket.closeReason,
        at: _clock.nowUtc(),
      ),
    );
  }

  /// Drops a socket this client decided to abandon — a silent one — and lets
  /// the retry chain replace it.
  void _drop(WebSocket socket, SocketClose close) {
    if (!identical(socket, _socket)) return;
    _socket = null;
    _stopHeartbeat();
    unawaited(_frames?.cancel());
    _frames = null;
    unawaited(socket.close(WebSocketStatus.goingAway, close.reason));

    _afterFailure(_run, close);
  }

  /// Decides what a lost connection means: another attempt, or the end.
  void _afterFailure(int run, SocketClose close, {bool terminal = false}) {
    if (run != _run) return;
    lastClose.value = close;
    _logger.i(
      'SocketClient: closed (${close.code ?? 'no code'}): ${close.reason}',
      tag: MenoLogTag.network.label,
    );

    if (!_active || terminal || _terminalCloseCodes.contains(close.code)) {
      _finish(run, close);
      return;
    }

    _attempt++;
    if (_attempt > _maxAttempts) {
      _logger.w(
        'SocketClient: gave up after $_maxAttempts reconnect attempts',
        tag: MenoLogTag.network.label,
      );
      _finish(run, close);
      return;
    }

    phase.value = .reconnecting;
    final delay = _backoff(_attempt);
    _logger.i(
      'SocketClient: reconnecting in ${delay.inMilliseconds}ms '
      '(attempt $_attempt/$_maxAttempts)',
      tag: MenoLogTag.network.label,
    );

    _retry?.cancel();
    _retry = Timer(delay, () {
      if (!_active || run != _run) return;
      unawaited(_connect());
    });
  }

  /// Ends the chain: no further attempt will be made.
  void _finish(int run, SocketClose close) {
    if (run != _run) return;
    _retry?.cancel();
    _retry = null;
    lastClose.value = close;
    phase.value = .closed;
  }

  /// How long to wait before attempt [attempt], which starts at 1.
  ///
  /// Exponential and capped, and deliberately without jitter: the retry chain
  /// is bounded at [_maxAttempts], so the whole schedule already fits inside
  /// the API's reconnect window, and a deterministic delay is one that can be
  /// asserted instead of guessed.
  Duration _backoff(int attempt) {
    final millis =
        _initialBackoff.inMilliseconds * math.pow(2, attempt - 1).toDouble();
    return Duration(
      milliseconds: math
          .min(millis, _maxBackoff.inMilliseconds.toDouble())
          .round(),
    );
  }

  @override
  FutureOr<dynamic> onDispose() async {
    _logger.d('SocketClient: disposing', tag: MenoLogTag.network.label);
    await stop();
    _handlers.clear();
    await _reconnectedContr.close();
    phase.dispose();
    lastClose.dispose();
  }
}

/// The [SocketSubscription] [SocketClient.on] hands back.
///
/// Private because cancelling means removing a handler from this client's
/// registry, which is nobody else's business. Callers get the contract instead.
final class _SocketSubscription implements SocketSubscription {
  new(this._client, this.event, this._handler);

  final SocketClient _client;
  final void Function(dynamic) _handler;

  @override
  final SocketEvent event;

  @override
  void cancel() => _client._off(event, _handler);
}
