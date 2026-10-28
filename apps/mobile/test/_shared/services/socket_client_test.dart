import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/socket.dart';
import 'package:meno_mobile/_shared/services/socket_client.dart';

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

/// A real WebSocket server on loopback, standing in for `apps/api`'s `/ws`.
///
/// It accepts every upgrade, records the token it was handed and every frame it
/// receives, and — unless told otherwise — answers nothing at all, which is
/// what makes a heartbeat miss observable.
final class _LocalSocketServer {
  new(this._http);

  final HttpServer _http;

  /// The tokens the handshakes presented, in arrival order.
  final tokens = <String?>[];

  /// The sockets that were opened, in arrival order.
  final sockets = <WebSocket>[];

  /// Every text frame the server received, in arrival order.
  final received = <String>[];

  /// A close code the server applies to every socket as soon as it opens.
  int? closeOnOpen;

  int get connectionCount => sockets.length;

  static Future<_LocalSocketServer> start() async {
    final http = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final server = _LocalSocketServer(http);

    http.listen((request) async {
      server.tokens.add(request.uri.queryParameters['token']);
      final socket = await WebSocketTransformer.upgrade(request);
      server.sockets.add(socket);

      final code = server.closeOnOpen;
      if (code != null) {
        await socket.close(code, 'refused');
        return;
      }

      socket.listen((data) {
        if (data is String) server.received.add(data);
      }, onError: (Object _) {});
    });

    return server;
  }

  Uri get uri => Uri.parse('ws://${_http.address.host}:${_http.port}/ws');

  Future<void> close() async {
    for (final socket in sockets) {
      await socket.close();
    }
    await _http.close(force: true);
  }
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

SocketClient _client(
  Uri uri, {
  FutureOr<String?> Function()? credential,
  // Long by default so a test that is not about the heartbeat never races it.
  Duration pingInterval = const Duration(seconds: 10),
  Duration pongTimeout = const Duration(seconds: 1),
  int maxAttempts = 3,
  Duration initialBackoff = const Duration(milliseconds: 10),
}) => SocketClient(
  uri: uri,
  credentialSupplier: credential ?? () => 'token-1',
  logger: const _SilentLogger(),
  clock: const SystemClock(),
  pingInterval: pingInterval,
  pongTimeout: pongTimeout,
  maxAttempts: maxAttempts,
  initialBackoff: initialBackoff,
  maxBackoff: const Duration(milliseconds: 40),
);

void main() {
  group('SocketFrame', () {
    test('encodes and decodes an event envelope round trip', () {
      const frame = SocketFrame(
        event: .participantJoined,
        data: {'broadcastId': 'abc', 'count': 2},
      );

      final decoded = SocketFrame.tryDecode(frame.encode());

      expect(decoded, isNotNull);
      expect(decoded!.event, SocketEvent.participantJoined);
      expect(decoded.data, {'broadcastId': 'abc', 'count': 2});
    });

    test('drops a frame whose event this build does not know', () {
      expect(
        SocketFrame.tryDecode('{"event":"someFutureEvent","data":{}}'),
        isNull,
      );
    });

    test('drops a frame that is not an envelope', () {
      expect(SocketFrame.tryDecode('42["newBroadcast",{}]'), isNull);
      expect(SocketFrame.tryDecode('not json'), isNull);
      expect(SocketFrame.tryDecode('{"event":42}'), isNull);
    });
  });

  group('SocketClient', () {
    late _LocalSocketServer server;
    SocketClient? client;

    setUp(() async {
      server = await _LocalSocketServer.start();
    });

    tearDown(() async {
      await client?.onDispose();
      client = null;
      await server.close();
    });

    test(
      'connects with the supplied token and receives a known event',
      () async {
        final received = Completer<dynamic>();
        client = _client(server.uri)
          ..on(SocketEvent.newBroadcast, (data) {
            if (!received.isCompleted) received.complete(data);
          });

        await client!.start();
        await _waitFor(
          () => server.connectionCount == 1,
          reason: 'the upgrade',
        );

        await _waitFor(
          () => client!.phase.value == ConnectionPhase.open,
          reason: 'the open phase',
        );
        expect(server.tokens.single, 'token-1');

        server.sockets.single.add(
          const SocketFrame(
            event: .newBroadcast,
            data: {'title': 'on air'},
          ).encode(),
        );

        expect(await received.future, {'title': 'on air'});
      },
    );

    test('a missed heartbeat reconnects, reading the token again', () async {
      var reads = 0;
      client = _client(
        server.uri,
        credential: () => 'token-${++reads}',
        pingInterval: const Duration(milliseconds: 40),
        pongTimeout: const Duration(milliseconds: 50),
      );

      await client!.start();
      await _waitFor(() => server.connectionCount == 1, reason: 'the upgrade');
      await _waitFor(
        () => client!.phase.value == ConnectionPhase.open,
        reason: 'the open phase',
      );

      // The server never answers the heartbeat, so the client must notice the
      // silence and open a fresh socket rather than waiting forever.
      await _waitFor(
        () => server.connectionCount >= 2,
        reason: 'a reconnect after the heartbeat miss',
      );

      await _waitFor(
        () => client!.phase.value == ConnectionPhase.open,
        reason: 'the reconnected open phase',
      );
      expect(server.tokens, ['token-1', 'token-2']);
    });

    test('a terminal close code stops the retry chain', () async {
      server.closeOnOpen = 1008;
      client = _client(server.uri);

      await client!.start();
      await _waitFor(
        () => client!.phase.value == ConnectionPhase.closed,
        reason: 'the terminal close',
      );

      // Give a retry the time it would have taken to appear.
      await Future<void>.delayed(const Duration(milliseconds: 150));

      expect(server.connectionCount, 1);
      expect(client!.lastClose.value?.code, 1008);
      expect(client!.phase.value, ConnectionPhase.closed);
    });

    test('send throws a typed error before the socket is open', () {
      client = _client(server.uri);

      expect(
        () => client!.send(SocketEvent.sendChatMessage, {'text': 'hi'}),
        throwsA(isA<SocketNotConnectedException>()),
      );
    });

    test('stop closes the socket and makes it non-retrying', () async {
      client = _client(server.uri);
      await client!.start();
      await _waitFor(() => server.connectionCount == 1, reason: 'the upgrade');

      await client!.stop();

      expect(client!.phase.value, ConnectionPhase.closed);
      await Future<void>.delayed(const Duration(milliseconds: 150));
      expect(server.connectionCount, 1);
    });
  });
}
