import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/_shared/services/room_transport.dart';
import 'package:meno_mobile/features/auth/model/auth_context.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/live_session_store.dart';
import 'package:meno_mobile/features/broadcast/session/active_broadcast_session.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/fake_clock.dart';
import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

const _viewer = 'c0ffee00-1111-4222-8333-444455556666';
const _other = 'c0ffee00-2222-4222-8333-444455556666';
const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';

BroadcastSession _details({
  String creatorId = _viewer,
  bool canPublish = true,
}) => BroadcastSession(
  userId: Id.fromString(_viewer),
  broadcast: Broadcast(
    id: Id.fromString(_broadcastId),
    title: SingleLineString('Morning show'),
    state: .live,
    status: .active,
    participantRole: creatorId == _viewer ? .host : .listener,
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
  bool failConnect = false;

  @override
  Stream<RoomConnectionState> get connectionState => _states.stream;

  @override
  bool get isConnected => false;

  @override
  bool get isMicrophoneEnabled => false;

  @override
  Future<void> connect(String token, {required bool canPublish}) async {
    if (failConnect) throw const LiveKitConnectionFailed('refused');
  }

  @override
  Future<void> disconnect() async {}

  @override
  Future<void> setMicrophoneEnabled(bool enabled) async {}

  @override
  FutureOr<dynamic> onDispose() async {
    disposals++;
    await _states.close();
  }
}

final class _FakeRoster implements LiveRoster {
  @override
  Future<void> attach() async {}

  @override
  Future<void> detach() async {}
}

/// A gateway a test drives: entries can be held, the end can fail.
final class _FakeGateway implements LiveGateway {
  int enterCalls = 0;
  int endCalls = 0;
  int leaveCalls = 0;
  Completer<BroadcastSession>? holdNextEnter;
  bool failEnd = false;
  bool failEnter = false;

  @override
  Future<BroadcastSession> enter(
    LiveEnterRequest request,
    AccountContext context,
  ) async {
    enterCalls++;
    if (failEnter) throw StateError('the API is unreachable');
    final held = holdNextEnter;
    if (held != null) {
      holdNextEnter = null;
      return await held.future;
    }
    return _details();
  }

  @override
  Future<EndedBroadcast> end(Id broadcastId) async {
    endCalls++;
    if (failEnd) throw StateError('the API is unreachable');
    return EndedBroadcast(
      broadcastId: broadcastId,
      title: SingleLineString('Morning show'),
      creatorId: Id.fromString(_viewer),
      duration: const Duration(minutes: 12),
      totalParticipants: 4,
    );
  }

  @override
  Future<void> leave(Id broadcastId) async {
    leaveCalls++;
  }
}

void main() {
  late FakeClock clock;
  late _FakeGateway gateway;
  late LiveSessionStore store;
  late List<_FakeRoom> rooms;
  late ActiveBroadcastManager manager;
  var failRooms = false;

  setUp(() async {
    Command.globalExceptionHandler = (_, _) {};
    SharedPreferences.setMockInitialValues({});
    clock = FakeClock(DateTime.utc(2026, 10, 28, 9));
    gateway = _FakeGateway();
    rooms = [];
    store = LiveSessionStore(
      LocalStorage(await SharedPreferences.getInstance()),
      userId: Id.fromString(_viewer),
    );

    manager = ActiveBroadcastManager(
      context: AccountContext(accountId: Id.fromString(_viewer), generation: 1),
      gateway: gateway,
      store: store,
      logger: MockMenoLogger(),
      createSession: ({required session, required exitOnServer}) {
        final room = _FakeRoom()..failConnect = failRooms;
        rooms.add(room);
        return ActiveBroadcastSession(
          session: session,
          room: room,
          roster: _FakeRoster(),
          clock: clock,
          logger: MockMenoLogger(),
          exitOnServer: exitOnServer,
        );
      },
    );
    addTearDown(manager.onDispose);
  });

  const request = LiveEnterRequest(
    broadcastId: Id.empty,
    mode: LiveEnterMode.listen,
  );

  test('a second entry replaces the first session exactly once', () async {
    final first = await manager.enter(request);
    final second = await manager.enter(request);

    expect(first.isEntered, isTrue);
    expect(second.isEntered, isTrue);
    expect(gateway.enterCalls, 2);
    expect(rooms, hasLength(2));
    expect(rooms.first.disposals, 1, reason: 'the replaced session closed');
    expect(rooms.last.disposals, 0, reason: 'the live one is still open');
    expect(manager.hasSession, isTrue);
    expect(manager.snapshot.value.phase, LivePhase.live);
  });

  test('an account exit during an entry publishes nothing live', () async {
    final held = Completer<BroadcastSession>();
    gateway.holdNextEnter = held;

    final entering = manager.enter(request);
    await pumpUntil(() => gateway.enterCalls == 1);

    final exiting = manager.closeForAccountExit();
    held.complete(_details());

    final result = await entering;
    await exiting;

    expect(result.outcome, EnterOutcome.rejected);
    expect(manager.hasSession, isFalse);
    expect(
      manager.snapshot.value.phase,
      isNot(LivePhase.live),
      reason: 'nothing may go live once the account is leaving',
    );
  });

  test('recovery does not replace a newer session', () async {
    await manager.enter(request);
    await pumpUntil(() => manager.snapshot.value.phase == LivePhase.live);

    await manager.recover();

    expect(gateway.enterCalls, 1, reason: 'recovery found a session already');
    expect(manager.snapshot.value.phase, LivePhase.live);
    expect(manager.hasSession, isTrue);
  });

  test('a failed REST end still ends locally, with a summary', () async {
    await manager.enter(request);
    gateway.failEnd = true;

    await manager.leaveOrEnd.runAsync();

    final snapshot = manager.snapshot.value;
    expect(snapshot.phase, LivePhase.ended);
    expect(snapshot.endedSummary, isNotNull);
    expect(snapshot.endedSummary!.id.getOrNull, _broadcastId);
    expect(manager.hasSession, isFalse);
  });

  test('recovery from the store completes without deadlocking', () async {
    await store.save(_details());

    await manager.recover().timeout(const Duration(seconds: 2));

    expect(gateway.enterCalls, 1);
    expect(manager.snapshot.value.phase, LivePhase.live);
  });

  test('a session whose start throws is closed, not leaked', () async {
    failRooms = true;
    addTearDown(() => failRooms = false);

    final result = await manager.enter(request);

    expect(result.outcome, EnterOutcome.failed);
    expect(result.failure?.retryable, isTrue);
    expect(manager.snapshot.value.phase, LivePhase.failed);
    expect(manager.hasSession, isFalse);
    expect(rooms.single.disposals, 1);
  });

  test('a cohost exits by leaving, never by ending', () async {
    gateway = _FakeGateway();
    // Rebuild the manager over the same gateway shape, this time with a
    // broadcast this account did not create.
    final cohostManager = ActiveBroadcastManager(
      context: AccountContext(accountId: Id.fromString(_viewer), generation: 1),
      gateway: _CohostGateway(gateway),
      store: store,
      logger: MockMenoLogger(),
      createSession: ({required session, required exitOnServer}) {
        rooms.add(_FakeRoom());
        return ActiveBroadcastSession(
          session: session,
          room: rooms.last,
          roster: _FakeRoster(),
          clock: clock,
          logger: MockMenoLogger(),
          exitOnServer: exitOnServer,
        );
      },
    );
    addTearDown(cohostManager.onDispose);

    await cohostManager.enter(request);
    await cohostManager.leaveOrEnd.runAsync();

    expect(gateway.endCalls, 0, reason: 'a cohost may not end the broadcast');
    expect(gateway.leaveCalls, 1);
    expect(cohostManager.snapshot.value.phase, LivePhase.ended);
  });
}

/// The same fake, answering entries as somebody else's broadcast.
final class _CohostGateway implements LiveGateway {
  const new(this._inner);

  final _FakeGateway _inner;

  @override
  Future<BroadcastSession> enter(
    LiveEnterRequest request,
    AccountContext context,
  ) async {
    _inner.enterCalls++;
    return _details(creatorId: _other);
  }

  @override
  Future<EndedBroadcast> end(Id broadcastId) => _inner.end(broadcastId);

  @override
  Future<void> leave(Id broadcastId) => _inner.leave(broadcastId);
}
