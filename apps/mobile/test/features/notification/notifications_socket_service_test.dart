import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/socket_client.dart';
import 'package:meno_mobile/features/notification/notification.dart';

import '../../_support/stub_api.dart';

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

/// The `notification` frame the API sends.
///
/// It carries the recipient, the rendered copy and a timestamp — and nothing
/// else: no id, no type, no deep link.
String _notificationFrame({String? title = 'Live Now', String? body}) {
  return jsonEncode({
    'event': 'notification',
    'data': {
      'userId': '11111111-1111-4111-8111-111111111111',
      'title': ?title,
      'body': ?body,
      'timestamp': DateTime.now().toIso8601String(),
    },
  });
}

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
  late NotificationsSocketService service;
  late List<NotificationSignal> signals;

  setUp(() async {
    server = await _LocalSocketServer.start();
    client = SocketClient(
      uri: server.uri,
      credentialSupplier: () => 'token-1',
      logger: MockMenoLogger(),
      clock: const SystemClock(),
      // Long enough that the heartbeat never races these tests.
      pingInterval: const Duration(seconds: 10),
      pongTimeout: const Duration(seconds: 1),
    );
    service = NotificationsSocketService(
      client: client,
      logger: MockMenoLogger(),
    );
    signals = [];
    service.onNotification.listen(signals.add);
  });

  tearDown(() async {
    await service.onDispose();
    await client.onDispose();
    await server.close();
  });

  test('a notification frame becomes a typed signal', () async {
    service.install();
    await client.start();
    await _waitFor(() => client.isOpen, reason: 'the socket to open');

    await server.send(_notificationFrame(body: 'Ada is live: Morning Notes'));
    await _waitFor(() => signals.isNotEmpty, reason: 'the signal');

    expect(signals.single.title, 'Live Now');
    expect(signals.single.body, 'Ada is live: Morning Notes');
  });

  test(
    'install is idempotent: a second call does not double-deliver',
    () async {
      service.install();
      service.install();
      await client.start();
      await _waitFor(() => client.isOpen, reason: 'the socket to open');

      await server.send(_notificationFrame());
      await _waitFor(() => signals.isNotEmpty, reason: 'the signal');
      await Future<void>.delayed(const Duration(milliseconds: 100));

      expect(signals, hasLength(1));
    },
  );

  test('a frame with no copy is dropped', () async {
    service.install();
    await client.start();
    await _waitFor(() => client.isOpen, reason: 'the socket to open');

    await server.send(_notificationFrame(title: null));
    await Future<void>.delayed(const Duration(milliseconds: 200));

    expect(signals, isEmpty);
  });

  test('a frame that is not an object is dropped', () async {
    service.install();
    await client.start();
    await _waitFor(() => client.isOpen, reason: 'the socket to open');

    await server.send(
      jsonEncode({
        'event': 'notification',
        'data': ['not', 'a', 'map'],
      }),
    );
    await Future<void>.delayed(const Duration(milliseconds: 200));

    expect(signals, isEmpty);
  });

  test('detach stops delivery, and is idempotent', () async {
    service.install();
    await client.start();
    await _waitFor(() => client.isOpen, reason: 'the socket to open');

    service
      ..detach()
      ..detach();

    await server.send(_notificationFrame());
    await Future<void>.delayed(const Duration(milliseconds: 200));

    expect(signals, isEmpty);
  });
}
