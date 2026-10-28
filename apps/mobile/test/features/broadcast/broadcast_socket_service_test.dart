import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/socket_client.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// A logger that writes nothing, so a test's only output is its assertions.
final class _SilentLogger implements MenoLogger {
  const new();

  @override
  void v(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void d(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void i(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void w(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void e(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void fatal(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void event(String name, Map<String, Object> context) {}
}

/// One loopback WebSocket server standing in for the API's `/ws`.
final class _LocalSocketServer {
  new(this._http);

  final HttpServer _http;
  final sockets = <WebSocket>[];

  static Future<_LocalSocketServer> start() async {
    final http = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final server = _LocalSocketServer(http);

    http.listen((request) async {
      final socket = await WebSocketTransformer.upgrade(request);
      server.sockets.add(socket);
      socket.listen((_) {}, onError: (Object _) {});
    });

    return server;
  }

  Uri get uri => Uri.parse('ws://${_http.address.host}:${_http.port}/ws');

  /// Sends one text frame to the most recently opened socket.
  ///
  /// Awaits the server's own side of the upgrade: the client reports `open`
  /// as soon as its handshake completes, which can be a microtask before this
  /// list has the socket.
  Future<void> send(String frame) async {
    await _waitFor(() => sockets.isNotEmpty, reason: 'the server socket');
    sockets.last.add(frame);
  }

  Future<void> close() async {
    for (final socket in sockets) {
      await socket.close();
    }
    await _http.close(force: true);
  }
}

const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';

/// The `hostDisconnected` frame the API sends.
String _hostDisconnected() =>
    '{"event":"hostDisconnected","data":{"broadcastId":"$_broadcastId"}}';

/// Waits for [condition], or fails the test after a bounded wait.
Future<void> _waitFor(
  bool Function() condition, {
  String reason = 'condition',
  Duration timeout = const Duration(seconds: 3),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (!condition()) {
    if (DateTime.now().isAfter(deadline)) {
      fail('Timed out waiting for $reason.');
    }
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
}

void main() {
  late _LocalSocketServer server;
  late SocketClient client;
  late BroadcastSocketService service;

  setUp(() async {
    server = await _LocalSocketServer.start();
    client = SocketClient(
      uri: server.uri,
      credentialSupplier: () => 'token-1',
      logger: const _SilentLogger(),
      clock: const SystemClock(),
      // Long enough that the heartbeat never races these tests.
      pingInterval: const Duration(seconds: 10),
      pongTimeout: const Duration(seconds: 1),
    );
    service = BroadcastSocketService(
      client: client,
      logger: const _SilentLogger(),
    );
  });

  tearDown(() async {
    await service.onDispose();
    await client.onDispose();
    await server.close();
  });

  test('install decodes a frame into the typed event', () async {
    final hosts = <HostConnectionEvent>[];
    service.onHostConnectionChanged.listen(hosts.add);

    service.install();
    await client.start();
    await _waitFor(() => client.isOpen, reason: 'the socket to open');

    await server.send(_hostDisconnected());
    await _waitFor(() => hosts.isNotEmpty, reason: 'the host event');

    expect(hosts.single.broadcastId.getOrNull, _broadcastId);
    expect(hosts.single.connected, isFalse);
  });

  test(
    'install is idempotent: a second call does not double-deliver',
    () async {
      final hosts = <HostConnectionEvent>[];
      service.onHostConnectionChanged.listen(hosts.add);

      service.install();
      service.install();
      await client.start();
      await _waitFor(() => client.isOpen, reason: 'the socket to open');

      await server.send(_hostDisconnected());
      await _waitFor(() => hosts.isNotEmpty, reason: 'the host event');
      await Future<void>.delayed(const Duration(milliseconds: 100));

      expect(hosts, hasLength(1));
    },
  );

  test('detach cancels every handle, and is idempotent', () async {
    final hosts = <HostConnectionEvent>[];
    service.onHostConnectionChanged.listen(hosts.add);

    service.install();
    await client.start();
    await _waitFor(() => client.isOpen, reason: 'the socket to open');

    service
      ..detach()
      ..detach();

    await server.send(_hostDisconnected());
    await Future<void>.delayed(const Duration(milliseconds: 150));

    expect(hosts, isEmpty);

    // Installing again after a detach attaches a fresh set rather than
    // delivering to handles the first install created.
    service.install();
    await server.send(_hostDisconnected());
    await _waitFor(() => hosts.isNotEmpty, reason: 'the host event');
    expect(hosts, hasLength(1));
  });

  test('a malformed frame emits nothing and does not throw', () async {
    final hosts = <HostConnectionEvent>[];
    service.onHostConnectionChanged.listen(hosts.add);

    service.install();
    await client.start();
    await _waitFor(() => client.isOpen, reason: 'the socket to open');

    await server.send('42["hostDisconnected",{}]');
    await server.send('{"event":"hostDisconnected","data":');
    await server.send('{"event":"someFutureEvent","data":{}}');
    await server.send('{"event":"hostDisconnected","data":{"broadcastId":42}}');
    await Future<void>.delayed(const Duration(milliseconds: 150));

    expect(hosts, isEmpty);

    // The connection is still healthy: a well-formed frame still arrives.
    await server.send(_hostDisconnected());
    await _waitFor(() => hosts.isNotEmpty, reason: 'the host event');
  });

  test('subscribing without install receives nothing', () async {
    final hosts = <HostConnectionEvent>[];
    service.onHostConnectionChanged.listen(hosts.add);

    await client.start();
    await _waitFor(() => client.isOpen, reason: 'the socket to open');

    await server.send(_hostDisconnected());
    await Future<void>.delayed(const Duration(milliseconds: 150));

    expect(hosts, isEmpty);
  });
}
