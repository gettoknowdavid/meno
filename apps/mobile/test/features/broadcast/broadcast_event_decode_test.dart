import 'dart:convert';
import 'dart:io';

import 'package:dio/dio.dart' show ResponseBody;
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/manager/broadcast_feed_data_source.dart';
import 'package:meno_mobile/_shared/services/socket_client.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';

/// The frames the backend emits, shaped exactly as `apps/api` serialises them.
///
/// `homeInvalidated` carries a real RFC3339 string here. Serde writes an
/// `OffsetDateTime` as an array unless its RFC3339 adapter is on, which is the
/// payload the next test uses to prove a bad timestamp is rejected rather than
/// guessed at.
final _fixtures = <String, String>{
  'newBroadcast': _frame('newBroadcast', _broadcastJson(_broadcastId)),
  'endedBroadcast': _frame('endedBroadcast', {
    'broadcastId': _broadcastId,
    'reason': 'host_disconnected',
  }),
  'homeInvalidated': _frame('homeInvalidated', {
    'timestamp': '2026-10-28T09:15:00Z',
  }),
  'participantJoined': _frame('participantJoined', {
    'broadcastId': _broadcastId,
    'count': 2,
  }),
  'broadcastError': _frame('broadcastError', {
    'code': 'TOKEN_EXPIRED',
    'message': 'expired',
    'recoverable': true,
  }),
};

/// One `{event, data}` frame, as the API writes it.
String _frame(String event, Map<String, dynamic> data) =>
    jsonEncode({'event': event, 'data': data});

/// A broadcast as the API lists it, with only the fields it always sends.
Map<String, dynamic> _broadcastJson(String id) => {
  'id': id,
  'title': 'Morning show',
  'state': 'live',
  'status': 'active',
  'participantRole': 'host',
  'isJoined': true,
  'isBookmarked': false,
  'liveParticipantsCount': 1,
  'totalParticipants': 0,
  'recordingEnabled': false,
  'cohosts': <Map<String, dynamic>>[],
  'createdAt': '2026-01-01T00:00:00Z',
  'creatorId': 'c0ffee00-1111-4222-8333-444455556666',
  'creator': {'id': 'c0ffee00-1111-4222-8333-444455556666', 'fullName': 'Ada'},
};

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

/// An empty page, as `GET /broadcasts` answers with one.
Future<ResponseBody> Function() _emptyPage() =>
    () async =>
        envelope(data: {'data': <Map<String, dynamic>>[], 'next_cursor': null});

/// One page carrying [items].
Future<ResponseBody> Function() _page(List<Map<String, dynamic>> items) =>
    () async => envelope(data: {'data': items, 'next_cursor': null});

void main() {
  // An unclaimed command failure has to have somewhere to go.
  setUp(() => Command.globalExceptionHandler = (_, _) {});

  group('a socket event payload', () {
    late _LocalSocketServer server;
    late SocketClient client;
    late BroadcastSocketService socket;
    late List<BroadcastEvent> events;

    setUp(() async {
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

      events = [];
      // A manager would listen to one stream; this test listens to all of them
      // so a frame that decodes into the wrong variant is visible.
      socket.onNewBroadcast.listen(
        (event) => events.add(NewBroadcastEvent(broadcast: event)),
      );
      socket.onEndedBroadcast.listen(events.add);
      socket.onHomeInvalidated.listen(
        (event) => events.add(HomeInvalidatedEvent(invalidation: event)),
      );
      socket.onParticipantJoined.listen(
        (event) => events.add(ParticipantJoinedEvent(participant: event)),
      );
      socket.onBroadcastError.listen(
        (event) => events.add(BroadcastFailedEvent(error: event)),
      );

      socket.install();
      await client.start();
      await pumpUntil(() => client.isOpen);
    });

    tearDown(() async {
      await socket.onDispose();
      await client.onDispose();
      await server.close();
    });

    test('every fixture the backend emits decodes', () async {
      for (final frame in _fixtures.values) {
        await server.send(frame);
      }

      await pumpUntil(() => events.length == _fixtures.length);

      expect(events, hasLength(_fixtures.length));
      expect(
        events.whereType<NewBroadcastEvent>().single.broadcast.title.getOrNull,
        'Morning show',
      );
      expect(
        events.whereType<EndedSignal>().single.reason,
        EndReason.hostDisconnected,
      );
      expect(
        events.whereType<HomeInvalidatedEvent>().single.invalidation.timestamp,
        DateTime.utc(2026, 10, 28, 9, 15),
      );
      expect(
        events.whereType<ParticipantJoinedEvent>().single.participant.count,
        2,
      );
      expect(
        events.whereType<BroadcastFailedEvent>().single.error.code,
        BroadcastErrorCode.tokenExpired,
      );
    });

    test('a timestamp that is not RFC3339 fails instead of defaulting', () {
      // Serde's own `OffsetDateTime` output, and two strings that
      // `DateTime.parse` would happily accept as something else.
      expect(
        () => parseRfc3339([2026, 301, 0, 0, 0, 0]),
        throwsFormatException,
      );
      expect(() => parseRfc3339('2026-10-28'), throwsFormatException);
      expect(() => parseRfc3339('2026-10-28T09:15:00'), throwsFormatException);
      expect(() => parseRfc3339(null), throwsFormatException);

      expect(
        parseRfc3339('2026-10-28T09:15:00Z'),
        DateTime.utc(2026, 10, 28, 9, 15),
      );
      expect(
        parseRfc3339('2026-10-28T11:15:00+02:00'),
        DateTime.utc(2026, 10, 28, 9, 15),
      );
    });

    test('a non-RFC3339 invalidation is dropped, not guessed at', () async {
      await server.send(
        '{"event":"homeInvalidated","data":'
        '{"timestamp":[2026,301,0,0,0,0,0]}}',
      );
      await Future<void>.delayed(const Duration(milliseconds: 100));

      expect(events.whereType<HomeInvalidatedEvent>(), isEmpty);

      // The connection survives the bad frame: the next one still decodes.
      await server.send(_fixtures['homeInvalidated']!);
      await pumpUntil(
        () => events.whereType<HomeInvalidatedEvent>().isNotEmpty,
      );
      expect(events.whereType<HomeInvalidatedEvent>(), hasLength(1));
    });

    test(
      'an unknown event and an ended frame with no id emit nothing',
      () async {
        await server.send('{"event":"someFutureEvent","data":{}}');
        await server.send(
          '{"event":"endedBroadcast","data":{"broadcastId":"not-a-uuid",'
          '"reason":"normal"}}',
        );
        await Future<void>.delayed(const Duration(milliseconds: 100));

        expect(events, isEmpty);
      },
    );
  });

  group('an ended signal', () {
    late _LocalSocketServer server;
    late SocketClient client;
    late BroadcastSocketService socket;
    late StubApi api;
    late BroadcastFeedDataSource feed;

    setUp(() async {
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

      api = StubApi([
        _emptyPage(),
        _page([
          {
            'id': _broadcastId,
            'title': 'Morning show',
            'state': 'ended',
            'status': 'inactive',
            'createdAt': '2026-01-01T00:00:00Z',
            'endTime': '2026-10-28T09:15:00Z',
            'endReason': 'host_disconnected',
            'totalParticipants': 4,
            'creatorId': 'c0ffee00-1111-4222-8333-444455556666',
            'creatorName': 'Ada',
          },
        ]),
      ]);
      feed = BroadcastFeedDataSource(
        http: BroadcastHttpService(stubClient(api)),
        query: const BroadcastQuery.recentlyLive(),
        socket: socket,
      );
    });

    tearDown(() async {
      await feed.onDispose();
      await socket.onDispose();
      await client.onDispose();
      await server.close();
    });

    test('refetches the real summary instead of fabricating a card', () async {
      await feed.updateDataCommand.runAsync();
      expect(feed.items, isEmpty);
      expect(api.requests, hasLength(1));

      await server.send(_fixtures['endedBroadcast']!);

      // The refetch is the only thing that can add the row: the signal carries
      // no title, host or duration to build one from.
      await pumpUntil(
        () => feed.items.isNotEmpty,
        step: const Duration(milliseconds: 10),
      );

      expect(api.requests, hasLength(2));
      expect(feed.items.single.title.getOrNull, 'Morning show');
      expect(feed.items.single.totalParticipants, 4);
    });
  });
}
