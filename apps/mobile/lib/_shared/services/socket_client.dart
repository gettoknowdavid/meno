import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/socket.dart';
import 'package:socket_io_client/socket_io_client.dart' as io;

/// Infrastructure layer Socket.IO client
///
/// This client manages the WebSocket connection lifecycle and provides
/// a type-safe interface for socket operations. It should be used within
/// RemoteDataSource implementations, NOT in the application layer.
///
class SocketClient implements Disposable {
  new({required this._url, required this._token, required this._logger});

  final String _url;
  final String _token;
  final MenoLogger _logger;

  io.Socket? _socket;

  final _connectionContr = StreamController<SocketConnectionState>.broadcast();

  final _reconnectedContr = StreamController<void>.broadcast();

  /// Every handler registered with [on], keyed by event name.
  ///
  /// The handlers are kept here and not only on the socket because a socket
  /// does not survive a [connect]: the replacement is built from scratch, and
  /// this map is what it is handed so a subscription outlives the swap.
  final Map<String, List<void Function(dynamic)>> _eventHandlers = {};

  /// The last state published on [connectionState].
  SocketConnectionState _state = .disconnected;

  /// Stream of connection state changes
  Stream<SocketConnectionState> get connectionState => _connectionContr.stream;

  /// Published when a socket that had dropped is connected again.
  ///
  /// Separate from [connectionState] because that stream cannot tell the two
  /// apart: a first connect and a reconnect both report `connected`. The
  /// difference matters to whoever was in a room — a reconnect hands the client
  /// a socket the server has no membership for, so what it was subscribed to
  /// has to be said again — while a client connecting for the first time has
  /// nothing to re-announce.
  Stream<void> get onReconnected => _reconnectedContr.stream;

  /// Current connection status
  bool get isConnected => _socket?.connected ?? false;

  /// Initialize and connect to the socket server
  ///
  /// Does nothing when already connected, and the socket that is created is
  /// handed every handler registered so far.
  void connect() {
    if (isConnected) {
      _logger.d('SocketClient: Already connected');
      return;
    }

    // Dispose old socket if exists
    _disposeSocket();

    _logger.i('SocketClient: Connecting to $_url');
    _updateState(.connecting);

    _socket = io.io(
      _url,
      io.OptionBuilder().setTransports(['websocket']).setQuery({
        'token': _token,
      }).build(),
    );

    _setupConnectionListeners();
    _reattachEventHandlers();
  }

  /// Setup connection lifecycle listeners
  void _setupConnectionListeners() {
    _socket?.onConnect((_) {
      _logger.i('SocketClient: Connected');
      _updateState(.connected);
    });

    _socket?.onDisconnect((_) {
      _logger.d('SocketClient: Disconnected');
      _updateState(.disconnected);
    });

    _socket?.onReconnect((_) {
      _logger.i('SocketClient: Reconnected');
      _updateState(.connected);
      if (!_reconnectedContr.isClosed) _reconnectedContr.add(null);
    });

    _socket?.onReconnectAttempt((attempt) {
      _logger.v('SocketClient: Reconnecting... (attempt: $attempt)');
      _updateState(.reconnecting);
    });

    _socket?.onConnectError((error) {
      _logger.w('SocketClient: Connection error: $error');
      _updateState(.error);
    });

    _socket?.onError((error) {
      _logger.e('SocketClient: Socket error: $error');
      _updateState(.error);
    });
  }

  /// Reattach all registered event handlers after reconnection
  void _reattachEventHandlers() {
    _eventHandlers.forEach((event, handlers) {
      for (final handler in handlers) {
        _socket?.on(event, handler);
      }
    });
  }

  /// Subscribe to a socket event
  ///
  /// The handler is remembered, so a socket built later — by [connect] — gets
  /// it too, and returns a [SocketSubscription] that cancels it.
  SocketSubscription on(
    SocketEvent event,
    void Function(dynamic data) handler,
  ) {
    if (_socket == null) {
      _logger.d('SocketClient: Holding the $event handler until connect()');
    }

    // Store handler for reconnection scenarios
    _eventHandlers.putIfAbsent(event.value, () => []).add(handler);

    // Attach to socket if available
    _socket?.on(event.value, handler);

    return _SocketSubscription(this, event, handler);
  }

  /// Unsubscribe from a socket event
  void _off(SocketEvent event, void Function(dynamic) handler) {
    _eventHandlers[event.value]?.remove(handler);
    if (_eventHandlers[event.value]?.isEmpty ?? false) {
      _eventHandlers.remove(event.value);
    }
    _socket?.off(event.value, handler);
  }

  /// Emit an event to the server
  ///
  /// Throws [SocketNotConnectedException] if socket is not connected
  void emit(SocketEvent event, Object? data) {
    final socket = _socket;
    if (socket == null || !socket.connected) {
      throw SocketNotConnectedException(
        'Cannot emit "${event.value}": Socket not connected',
      );
    }

    _logger.d('SocketClient: Emitting event: ${event.value}');
    socket.emit(event.value, data);
  }

  /// Emit an event and wait for acknowledgment
  ///
  /// Returns the acknowledgment data from the server
  /// Throws [SocketNotConnectedException] if socket is not connected
  /// Throws [SocketTimeoutException] if acknowledgment times out
  Future<Object?> emitWithAck(
    SocketEvent event,
    Object? data, {
    Duration timeout = const Duration(seconds: 20),
  }) async {
    final socket = _socket;
    if (socket == null || !socket.connected) {
      throw SocketNotConnectedException(
        'Cannot emit "${event.value}": Socket not connected',
      );
    }

    _logger.d('SocketClient: Emitting event with ack: ${event.value}');

    // The acknowledgement arrives on a callback, so it is bridged into a
    // future here; the wait is bounded by [timeout] below rather than by a
    // timer of our own, which keeps the two failure modes — never acked, and
    // acked late — from overlapping.
    final acknowledgment = Completer<Object?>();
    socket.emitWithAck(
      event.value,
      data,
      ack: (Object? response) {
        if (!acknowledgment.isCompleted) {
          _logger.d('SocketClient: Received ack for event ${event.value}');
          acknowledgment.complete(response);
        }
      },
    );

    return await acknowledgment.future.timeout(
      timeout,
      onTimeout: () {
        _logger.w(
          'SocketClient: Timeout waiting for ack on event ${event.value}',
        );
        throw SocketTimeoutException(
          'Timeout waiting for acknowledgment on event "${event.value}"',
        );
      },
    );
  }

  /// Disconnect from the socket server
  void disconnect() {
    _logger.i('SocketClient: Disconnecting...');
    _disposeSocket();
    _updateState(.disconnected);
  }

  void _disposeSocket() {
    _logger.d('Disposing socket internally');
    _socket?.clearListeners();
    _socket?.dispose();
    _socket = null;
  }

  /// Publishes [next] on [connectionState] when it is news.
  ///
  /// The socket reports its own disconnects, so a state that is set here and
  /// then confirmed by the socket itself would otherwise be published twice
  /// for one transition.
  void _updateState(SocketConnectionState next) {
    if (_state == next || _connectionContr.isClosed) return;
    _state = next;
    _connectionContr.add(next);
  }

  @override
  FutureOr<dynamic> onDispose() async {
    _logger.d('SocketClient: Disposing');
    _disposeSocket();
    _eventHandlers.clear();
    await Future.wait([_connectionContr.close(), _reconnectedContr.close()]);
  }
}

/// The [SocketSubscription] [SocketClient.on] hands back.
///
/// Private because cancelling means removing a handler from this client's two
/// registries — the map that survives a reconnect, and the live socket — which
/// is nobody else's business. Callers get the contract instead.
final class _SocketSubscription implements SocketSubscription {
  new(this._client, this.event, this._handler);

  final SocketClient _client;
  final void Function(dynamic) _handler;

  @override
  final SocketEvent event;

  @override
  void cancel() => _client._off(event, _handler);
}

/// Socket connection states
enum SocketConnectionState {
  disconnected,
  connecting,
  connected,
  reconnecting,
  error,
}
