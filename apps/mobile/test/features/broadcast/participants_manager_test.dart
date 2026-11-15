import 'dart:async';
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

const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';
const _otherBroadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-000000000002';
const _hostId = '018f2b1e-0000-7000-8000-000000000001';
const _listenerId = '018f2b1e-0000-7000-8000-000000000002';
const _newcomerId = '018f2b1e-0000-7000-8000-000000000003';

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

Map<String, dynamic> _participant(String id, String name, String role) => {
  'id': id,
  'fullName': name,
  'role': role,
  'isJoined': true,
};

Future<ResponseBody> Function() _page(List<Map<String, dynamic>> items) =>
    () async => envelope(
      data: {'data': items, 'next_cursor': null, 'has_next_page': false},
    );

String _frame(String event, Map<String, dynamic> data) =>
    jsonEncode({'event': event, 'data': data});

void main() {
  setUp(() => Command.globalExceptionHandler = (_, _) {});

  late _LocalSocketServer server;
  late SocketClient client;
  late BroadcastSocketService socket;
  late ParticipantsManager manager;

  setUp(() async {
    server = await _LocalSocketServer.start();
    client = SocketClient(
      uri: server.uri,
      credentialSupplier: () => 'token-1',
      logger: MockMenoLogger(),
      clock: const SystemClock(),
      pingInterval: const Duration(seconds: 10),
      pongTimeout: const Duration(seconds: 1),
    );
    socket = BroadcastSocketService(client: client, logger: MockMenoLogger());
    socket.install();
    await client.start();
    await pumpUntil(() => client.isOpen);

    final api = StubApi([
      _page([
        _participant(_hostId, 'Ada', 'host'),
        _participant(_listenerId, 'Grace', 'participant'),
      ]),
    ]);
    manager = ParticipantsManager(
      http: BroadcastHttpService(stubClient(api)),
      socket: socket,
      broadcastId: Id.fromString(_broadcastId),
    );
    manager.initialize.run();
    await pumpUntil(() => manager.totalCount.value == 2);
    expect(manager.hasMore.value, isFalse);
  });

  tearDown(() async {
    await manager.onDispose();
    await socket.onDispose();
    await client.onDispose();
    await server.close();
  });

  test('the server count replaces the length of the loaded page', () async {
    await server.send(
      _frame('numberOfLiveParticipants', {
        'broadcastId': _broadcastId,
        'count': 9,
      }),
    );

    await pumpUntil(() => manager.totalCount.value == 9);
    expect(manager.liveCount.value, 9);
    expect(
      manager.allTimeCount.value,
      2,
      reason: 'a count is not a new account joining',
    );
    expect(
      manager.hasMore.value,
      isFalse,
      reason: 'there are no unloaded rows for Load more to reveal',
    );
  });

  test('another room count does not move this roster', () async {
    await server.send(
      _frame('numberOfLiveParticipants', {
        'broadcastId': _broadcastId,
        'count': 9,
      }),
    );
    await pumpUntil(() => manager.totalCount.value == 9);

    await server.send(
      _frame('numberOfLiveParticipants', {
        'broadcastId': _otherBroadcastId,
        'count': 99,
      }),
    );
    await server.send(
      _frame('participantJoined', {
        'broadcastId': _otherBroadcastId,
        'count': 100,
        'participant': _participant(_newcomerId, 'Ruth', 'participant'),
      }),
    );
    await Future<void>.delayed(const Duration(milliseconds: 50));

    expect(manager.totalCount.value, 9);
    expect(manager.allTimeCount.value, 2);
  });

  test('a count during the first read is not overwritten by it', () async {
    final held = Completer<ResponseBody>();
    final api = StubApi([() => held.future]);
    final late = ParticipantsManager(
      http: BroadcastHttpService(stubClient(api)),
      socket: socket,
      broadcastId: Id.fromString(_broadcastId),
    );
    addTearDown(late.onDispose);

    late.initialize.run();
    await pumpUntil(() => api.requests.isNotEmpty);

    await server.send(
      _frame('numberOfLiveParticipants', {
        'broadcastId': _broadcastId,
        'count': 9,
      }),
    );
    await pumpUntil(() => late.totalCount.value == 9);

    held.complete(
      await _page([
        _participant(_hostId, 'Ada', 'host'),
        _participant(_listenerId, 'Grace', 'participant'),
      ])(),
    );
    await pumpUntil(() => !late.isLoading.value);
    await Future<void>.delayed(const Duration(milliseconds: 40));

    expect(
      late.totalCount.value,
      9,
      reason: 'the page is older than the count it raced',
    );
  });

  test('a join frame continues from the server count', () async {
    await server.send(
      _frame('numberOfLiveParticipants', {
        'broadcastId': _broadcastId,
        'count': 9,
      }),
    );
    await pumpUntil(() => manager.totalCount.value == 9);

    await server.send(
      _frame('participantJoined', {
        'broadcastId': _broadcastId,
        'count': 10,
        'participant': _participant(_newcomerId, 'Ruth', 'participant'),
      }),
    );

    await pumpUntil(() => manager.totalCount.value == 10);
    expect(manager.allTimeCount.value, 3);
  });
}
