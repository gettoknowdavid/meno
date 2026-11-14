import 'dart:convert';
import 'dart:io';

import 'package:dio/dio.dart' show ResponseBody;
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

/// The creator a fixture broadcast belongs to.
const _creator = 'c0ffee00-1111-4222-8333-444455556666';

/// Another account, whose broadcasts must never appear in [_creator]'s feeds.
const _otherCreator = 'c0ffee00-2222-4222-8333-444455556666';

const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';

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

  Future<void> send(String frame) async {
    await pumpUntil(() => sockets.isNotEmpty);
    sockets.last.add(frame);
  }

  Future<void> close() async {
    for (final socket in sockets) {
      await socket.close();
    }
    await _http.close(force: true);
  }
}

/// A broadcast as the API lists it.
///
/// [withCreator] false is the frame that carries no creator at all — the shape
/// a creator-filtered feed cannot place without asking.
Map<String, dynamic> _broadcastJson({
  String id = _broadcastId,
  String creator = _creator,
  bool withCreator = true,
}) => {
  'id': id,
  'title': 'Morning show',
  'state': 'live',
  'status': 'active',
  // `participantRole` is not optional: a frame without it does not decode, and
  // an undecodable frame is indistinguishable from one that never arrived.
  'participantRole': 'viewer',
  'isJoined': true,
  'isBookmarked': false,
  'liveParticipantsCount': 1,
  'totalParticipants': 0,
  'recordingEnabled': false,
  'cohosts': <Map<String, dynamic>>[],
  'createdAt': '2026-01-01T00:00:00Z',
  if (withCreator) ...{
    'creatorId': creator,
    'creator': {'id': creator, 'fullName': 'Ada'},
  },
};

String _frame(String event, Map<String, dynamic> data) =>
    jsonEncode({'event': event, 'data': data});

/// An empty page, with a cursor so the feed knows there is more.
Future<ResponseBody> Function() _emptyPage({String? nextCursor}) =>
    () async => envelope(
      data: {
        'data': <Map<String, dynamic>>[],
        'next_cursor': nextCursor,
        'has_next_page': nextCursor != null,
      },
    );

void main() {
  setUp(() => Command.globalExceptionHandler = (_, _) {});

  late _LocalSocketServer server;
  late SocketClient client;
  late BroadcastSocketService socket;
  late StubApi api;
  late BroadcastFeedDataSource feed;

  /// Whether the feed's own disposal already happened; a command may only be
  /// disposed once, and one test disposes the feed itself.
  var disposed = false;

  Future<void> startFeed(BroadcastQuery query) async {
    api = StubApi([for (var i = 0; i < 20; i++) _emptyPage()]);
    feed = BroadcastFeedDataSource(
      http: BroadcastHttpService(stubClient(api)),
      query: query,
      socket: socket,
    );
    await feed.refresh();
  }

  setUp(() async {
    disposed = false;
    server = await _LocalSocketServer.start();
    client = SocketClient(
      uri: server.uri,
      credentialSupplier: () => 'token-1',
      logger: const _SilentLogger(),
      clock: const SystemClock(),
      pingInterval: const Duration(seconds: 10),
      pongTimeout: const Duration(seconds: 1),
    );
    socket = BroadcastSocketService(
      client: client,
      logger: const _SilentLogger(),
    );
    socket.install();
    await client.start();
    await pumpUntil(() => client.isOpen);
  });

  tearDown(() async {
    if (!disposed) await feed.onDispose();
    await socket.onDispose();
    await client.onDispose();
    await server.close();
  });

  /// Whether a frame produced exactly one refetch.
  Future<void> expectOneRefetch(int before) async {
    await pumpUntil(
      () => api.requests.length == before + 1,
      step: const Duration(milliseconds: 10),
    );
    // Give a second subscription the chance to ask again.
    await Future<void>.delayed(const Duration(milliseconds: 40));
    expect(api.requests, hasLength(before + 1));
  }

  group('rebinding (Step 63)', () {
    test('switching sections twice leaves one live subscription set', () async {
      await startFeed(const BroadcastQuery.nowLive());
      final initial = api.requests.length;

      feed.updateQuery(const BroadcastQuery.all());
      feed.updateQuery(const BroadcastQuery.nowLive());
      await pumpUntil(() => api.requests.length >= initial + 2);

      // One frame, one refetch: two subscription sets would each ask.
      final before = api.requests.length;
      await server.send(
        _frame('homeInvalidated', {'timestamp': '2026-10-28T09:15:00Z'}),
      );

      await expectOneRefetch(before);
    });

    test('a frame that arrives while the old set is cancelled lands', () async {
      await startFeed(const BroadcastQuery.nowLive());

      // The rebind swaps its handles in synchronously and only then awaits the
      // cancels. Waiting for the query's own refresh first means the frame is
      // read after that swap: one subscription set caught it, not two.
      feed.updateQuery(const BroadcastQuery.nowLive());
      await pumpUntil(() => api.requests.length >= 2);

      final before = api.requests.length;
      await server.send(
        _frame('homeInvalidated', {'timestamp': '2026-10-28T09:15:00Z'}),
      );

      await expectOneRefetch(before);
    });

    test('disposing stops the delivery', () async {
      await startFeed(const BroadcastQuery.nowLive());
      await feed.onDispose();
      disposed = true;

      final before = api.requests.length;
      await server.send(_frame('newBroadcast', _broadcastJson()));
      await Future<void>.delayed(const Duration(milliseconds: 60));

      expect(api.requests, hasLength(before));
    });
  });

  group('a predicate on the query (Step 63)', () {
    test("a creator's feed ignores another creator's broadcast", () async {
      await startFeed(const BroadcastQuery.byCreator(_creator));

      await server.send(
        _frame(
          'newBroadcast',
          _broadcastJson(
            id: '3f2b1c9e-8a4d-4f6b-9c1e-000000000001',
            creator: _otherCreator,
          ),
        ),
      );
      await Future<void>.delayed(const Duration(milliseconds: 60));

      expect(feed.items, isEmpty);
      expect(
        api.requests,
        hasLength(1),
        reason: 'nothing to refetch: the frame is simply not ours',
      );
    });

    test("a creator's feed takes that creator's broadcast", () async {
      await startFeed(const BroadcastQuery.byCreator(_creator));

      await server.send(_frame('newBroadcast', _broadcastJson()));

      // A frame travels the loopback socket, so this waits in real time.
      await pumpUntil(
        () => feed.items.isNotEmpty,
        step: const Duration(milliseconds: 10),
      );
      expect(feed.items.single.id.getOrNull, _broadcastId);
    });

    test('a frame with no creator refetches instead of guessing', () async {
      await startFeed(const BroadcastQuery.byCreator(_creator));
      final before = api.requests.length;

      await server.send(
        _frame('newBroadcast', _broadcastJson(withCreator: false)),
      );

      await expectOneRefetch(before);
      expect(feed.items, isEmpty);
    });

    test('a live section drops the broadcast that ends', () async {
      await startFeed(const BroadcastQuery.nowLive());
      await server.send(_frame('newBroadcast', _broadcastJson()));
      await pumpUntil(() => feed.items.isNotEmpty);

      await server.send(
        _frame('endedBroadcast', {
          'broadcastId': _broadcastId,
          'reason': 'host_disconnected',
        }),
      );

      await pumpUntil(() => feed.items.isEmpty);
      expect(feed.items, isEmpty);
    });
  });
}
