import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/live_background_service.dart';
import 'package:meno_mobile/_shared/services/room_transport.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/session/active_broadcast_session.dart';

import '../../_support/fake_clock.dart';
import '../../_support/stub_api.dart';

const _accountId = 'c0ffee00-3333-4222-8333-444455556666';
const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';

/// A session for an account whose role is [canPublish]'s business.
BroadcastSession _session({bool canPublish = true}) => BroadcastSession(
  userId: Id.fromString(_accountId),
  broadcast: Broadcast(
    id: Id.fromString(_broadcastId),
    title: SingleLineString('Morning show'),
    state: .live,
    status: .active,
    participantRole: canPublish ? .host : .listener,
    creator: Participant(
      id: Id.fromString(_accountId),
      fullName: SingleLineString('Ada'),
    ),
  ),
  token: 'room-token',
  canPublish: canPublish,
);

/// Remembers whether it was started, and as which side of the room.
final class _RecordingBackground implements LiveBackgroundService {
  final List<bool> starts = [];
  int stops = 0;

  @override
  Future<void> start({required bool isPublishing}) async {
    starts.add(isPublishing);
  }

  @override
  Future<void> stop() async {
    stops++;
  }
}

final class _Roster implements LiveRoster {
  @override
  Future<void> attach() async {}

  @override
  Future<void> detach() async {}
}

/// A room that always refuses, for the case where no service should start.
final class _RefusingRoom implements RoomTransport {
  @override
  Stream<RoomConnectionState> get connectionState =>
      const Stream<RoomConnectionState>.empty();

  @override
  RoomDisconnectReason? get lastDisconnectReason => null;

  @override
  bool get isConnected => false;

  @override
  bool get isMicrophoneEnabled => false;

  @override
  Future<void> connect(String token, {required bool canPublish}) async {
    throw const LiveKitConnectionFailed('refused');
  }

  @override
  Future<void> disconnect() async {}

  @override
  Future<void> setMicrophoneEnabled(bool enabled) async {}

  @override
  FutureOr<dynamic> onDispose() async {}
}

ActiveBroadcastSession _build(
  LiveBackgroundService background, {
  RoomTransport? room,
  bool canPublish = true,
}) => ActiveBroadcastSession(
  session: _session(canPublish: canPublish),
  room: room ?? SilentRoomTransport(),
  roster: _Roster(),
  background: background,
  clock: FakeClock(DateTime.utc(2026, 10, 28, 9)),
  logger: MockMenoLogger(),
);

void main() {
  test('a live room starts background execution after connecting', () async {
    final background = _RecordingBackground();
    final session = _build(background);

    await session.start();

    expect(background.starts, [true], reason: 'the host publishes');
    expect(background.stops, 0);
  });

  test('a listener starts it too, as a listener', () async {
    final background = _RecordingBackground();
    final session = _build(background, canPublish: false);

    await session.start();

    expect(
      background.starts,
      [false],
      reason: 'a locked screen must not silence a listener',
    );
  });

  test('closing the room stops it, once', () async {
    final background = _RecordingBackground();
    final session = _build(background);

    await session.start();
    await session.close(LiveCloseReason.userExit);
    await session.close(LiveCloseReason.userExit);

    expect(background.stops, 1, reason: 'close is idempotent');
  });

  test('a session closed before it started never starts one', () async {
    final background = _RecordingBackground();
    final session = _build(background);

    await session.close(LiveCloseReason.failed);

    expect(background.starts, isEmpty);
  });

  test('a room that refuses the connection starts nothing', () async {
    final background = _RecordingBackground();
    final session = _build(background, room: _RefusingRoom());

    await expectLater(session.start(), throwsA(isA<LiveKitFailure>()));

    // What the manager does with a start that threw: close it, which releases
    // everything the attempt had built.
    await session.close(LiveCloseReason.failed);

    expect(background.starts, isEmpty);
    expect(background.stops, 1, reason: 'teardown still releases it');
  });
}
