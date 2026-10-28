import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/room_transport.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/session/active_broadcast_session.dart';

import '../../_support/fake_clock.dart';
import '../../_support/stub_api.dart';

const _viewer = 'c0ffee00-1111-4222-8333-444455556666';

BroadcastSession _session({
  String creatorId = _viewer,
  bool canPublish = true,
}) => BroadcastSession(
  userId: Id.fromString(_viewer),
  broadcast: Broadcast(
    id: Id.fromString('3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e'),
    title: SingleLineString('Morning show'),
    state: .live,
    status: .active,
    creator: Participant(
      id: Id.fromString(creatorId),
      fullName: SingleLineString('Ada'),
    ),
  ),
  token: 'room-token',
  canPublish: canPublish,
);

final class _FakeRoom implements RoomTransport {
  final _states = StreamController<RoomConnectionState>.broadcast();

  int disposals = 0;
  int connects = 0;
  bool failConnect = false;
  bool connected = false;
  bool mic = false;

  @override
  Stream<RoomConnectionState> get connectionState => _states.stream;

  @override
  bool get isConnected => connected;

  @override
  bool get isMicrophoneEnabled => mic;

  @override
  Future<void> connect(String token, {required bool canPublish}) async {
    connects++;
    if (failConnect) throw const LiveKitConnectionFailed('refused');
    connected = true;
    _states.add(.connected);
  }

  @override
  Future<void> disconnect() async => connected = false;

  @override
  Future<void> setMicrophoneEnabled(bool enabled) async => mic = enabled;

  @override
  FutureOr<dynamic> onDispose() async {
    disposals++;
    connected = false;
    await _states.close();
  }
}

final class _FakeRoster implements LiveRoster {
  int attaches = 0;
  int detaches = 0;

  @override
  Future<void> attach() async => attaches++;

  @override
  Future<void> detach() async => detaches++;
}

void main() {
  late FakeClock clock;
  late _FakeRoom room;
  late _FakeRoster roster;

  setUp(() {
    clock = FakeClock(DateTime.utc(2026, 10, 28, 9));
    room = _FakeRoom();
    roster = _FakeRoster();
  });

  ActiveBroadcastSession newSession({
    String creatorId = _viewer,
    LiveServerExit? exitOnServer,
    Duration? initialElapsed,
  }) => ActiveBroadcastSession(
    session: _session(creatorId: creatorId),
    room: room,
    roster: roster,
    clock: clock,
    logger: MockMenoLogger(),
    exitOnServer: exitOnServer,
    initialElapsed: initialElapsed,
  );

  test('closing before start throws nothing and still tears down', () async {
    var exitCalls = 0;
    final session = newSession(
      exitOnServer: () async {
        exitCalls++;
        return const LiveServerOutcome(confirmed: true);
      },
    );

    // The old manager built its timer in a `late final` only a page
    // initialised, so this path used to throw (audit H03).
    final outcome = await session.close(LiveCloseReason.userExit);

    expect(outcome.confirmed, isTrue);
    expect(exitCalls, 0, reason: 'a session that never started has no exit');
    expect(session.isClosed, isTrue);
    expect(room.disposals, 1);
    expect(roster.detaches, 1);
  });

  test('a failed server exit still closes everything exactly once', () async {
    final session = newSession(
      exitOnServer: () async => throw StateError('the API is unreachable'),
    );
    await session.start();

    final outcome = await session.close(LiveCloseReason.userExit);

    expect(outcome.confirmed, isFalse);
    expect(outcome.error, isNotNull);
    expect(session.isClosed, isTrue);
    expect(room.disposals, 1);
    expect(roster.detaches, 1);
    expect(session.details.canPublish, isTrue);
  });

  test('closing twice awaits the same close', () async {
    final session = newSession();
    await session.start();

    final first = session.close(LiveCloseReason.userExit);
    final second = session.close(LiveCloseReason.replaced);

    expect(identical(first, second), isTrue);
    await Future.wait([first, second]);

    expect(room.disposals, 1);
    expect(roster.detaches, 1);
    expect(session.isClosed, isTrue);
  });

  test('the ending summary is carried out of the close', () async {
    final ended = EndedBroadcast(
      broadcastId: Id.fromString('3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e'),
      title: SingleLineString('Morning show'),
      creatorId: Id.fromString(_viewer),
      duration: const Duration(minutes: 12),
    );
    final session = newSession(
      exitOnServer: () async =>
          LiveServerOutcome(confirmed: true, summary: ended),
    );
    await session.start();

    final outcome = await session.close(LiveCloseReason.userExit);

    // Retained by the owner, never read from the closed session.
    expect(outcome.summary, ended);
    expect(outcome.summary!.duration, const Duration(minutes: 12));
  });
}
