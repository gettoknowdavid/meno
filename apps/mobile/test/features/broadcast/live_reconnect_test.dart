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
const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';

BroadcastSession _details() => BroadcastSession(
  userId: Id.fromString(_viewer),
  broadcast: Broadcast(
    id: Id.fromString(_broadcastId),
    title: SingleLineString('Morning show'),
    state: .live,
    status: .active,
    participantRole: .host,
    creator: Participant(
      id: Id.fromString(_viewer),
      fullName: SingleLineString('Ada'),
    ),
  ),
  token: 'room-token',
  canPublish: true,
);

/// A room a test drives: it emits the connection states and the reason itself.
final class _DrivenRoom implements RoomTransport {
  final _states = StreamController<RoomConnectionState>.broadcast();

  @override
  RoomDisconnectReason? lastDisconnectReason;

  int disposals = 0;

  @override
  Stream<RoomConnectionState> get connectionState => _states.stream;

  @override
  bool get isConnected => false;

  @override
  bool get isMicrophoneEnabled => false;

  @override
  Future<void> connect(String token, {required bool canPublish}) async {}

  @override
  Future<void> disconnect() async {}

  @override
  Future<void> setMicrophoneEnabled(bool enabled) async {}

  /// Reports a drop, and why.
  void drop(RoomDisconnectReason reason) {
    lastDisconnectReason = reason;
    _states.add(RoomConnectionState.disconnected);
  }

  /// Emits a state, unless the room was already torn down (a terminal end
  /// disposes it, and the test that checks "no resurrection" does that).
  void emit(RoomConnectionState state) {
    if (!_states.isClosed) _states.add(state);
  }

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

/// A gateway that counts entries and can hold an end open.
final class _Gateway implements LiveGateway {
  int enterCalls = 0;
  Completer<EndedBroadcast>? holdEnd;

  @override
  Future<BroadcastSession> enter(
    LiveEnterRequest request,
    AccountContext context,
  ) async {
    enterCalls++;
    return _details();
  }

  @override
  Future<EndedBroadcast> end(Id broadcastId) async {
    final held = holdEnd;
    if (held != null) {
      holdEnd = null;
      return await held.future;
    }
    return EndedBroadcast(
      broadcastId: broadcastId,
      title: SingleLineString('Morning show'),
      creatorId: Id.fromString(_viewer),
      duration: const Duration(minutes: 3),
      totalParticipants: 2,
    );
  }

  @override
  Future<void> leave(Id broadcastId) async {}
}

void main() {
  late _Gateway gateway;
  late LiveSessionStore store;
  late List<_DrivenRoom> rooms;
  late ActiveBroadcastManager manager;

  ActiveBroadcastManager build({
    Duration reconnectBudget = const Duration(seconds: 10),
    Stream<EndedSignal>? ended,
  }) {
    final built = ActiveBroadcastManager(
      context: AccountContext(accountId: Id.fromString(_viewer), generation: 1),
      gateway: gateway,
      store: store,
      logger: MockMenoLogger(),
      endedBroadcasts: ended,
      reconnectBudget: reconnectBudget,
      createSession: ({required session, required exitOnServer}) {
        final room = _DrivenRoom();
        rooms.add(room);
        return ActiveBroadcastSession(
          session: session,
          room: room,
          roster: _FakeRoster(),
          clock: FakeClock(DateTime.utc(2026, 10, 28, 9)),
          logger: MockMenoLogger(),
          exitOnServer: exitOnServer,
        );
      },
    );
    addTearDown(built.onDispose);
    return built;
  }

  setUp(() async {
    Command.globalExceptionHandler = (_, _) {};
    SharedPreferences.setMockInitialValues({});
    gateway = _Gateway();
    rooms = [];
    store = LiveSessionStore(
      LocalStorage(await SharedPreferences.getInstance()),
      userId: Id.fromString(_viewer),
    );
    manager = build();
  });

  const request = LiveEnterRequest(
    broadcastId: Id.empty,
    mode: LiveEnterMode.listen,
  );

  test('a kick ends the session once and never reconnects', () async {
    await manager.enter(request);
    expect(manager.snapshot.value.phase, LivePhase.live);

    rooms.single.drop(RoomDisconnectReason.partyRemoved);
    await pumpUntil(() => manager.snapshot.value.phase == LivePhase.ended);

    final snapshot = manager.snapshot.value;
    expect(snapshot.phase, LivePhase.ended);
    expect(snapshot.endReason, EndReason.adminForced);
    expect(snapshot.endedSummary?.id.getOrNull, _broadcastId);
    expect(manager.hasSession, isFalse);
    expect(rooms.single.disposals, 1);

    // A late "connected" from the room cannot bring an ended session back.
    rooms.single.emit(RoomConnectionState.connected);
    await pumpEventQueue();
    expect(manager.snapshot.value.phase, LivePhase.ended);
    expect(gateway.enterCalls, 1, reason: 'a kick is not retried');
  });

  test('a network drop reconnects into the same session', () async {
    await manager.enter(request);

    rooms.single.emit(RoomConnectionState.reconnecting);
    await pumpUntil(
      () => manager.snapshot.value.phase == LivePhase.reconnecting,
    );
    expect(manager.snapshot.value.phase, LivePhase.reconnecting);
    expect(manager.snapshot.value.broadcast?.id.getOrNull, _broadcastId);

    rooms.single.emit(RoomConnectionState.connected);
    await pumpUntil(() => manager.snapshot.value.phase == LivePhase.live);

    expect(manager.snapshot.value.phase, LivePhase.live);
    expect(rooms, hasLength(1), reason: 'the SDK reconnect keeps the session');
    expect(gateway.enterCalls, 1);
  });

  test(
    'a retry after a long outage asks for fresh entry credentials',
    () async {
      await manager.onDispose();
      manager = build(reconnectBudget: const Duration(milliseconds: 20));
      await manager.enter(request);

      rooms.single.drop(RoomDisconnectReason.signalClose);
      await pumpUntil(
        () => manager.snapshot.value.phase == LivePhase.reconnecting,
      );

      // The budget expires: the old media token is worthless, so the manager
      // closes the session and asks the API to enter again — a new token.
      await pumpUntil(
        () => gateway.enterCalls == 2,
        step: const Duration(milliseconds: 10),
        attempts: 80,
      );
      await pumpUntil(() => manager.snapshot.value.phase == LivePhase.live);

      expect(
        gateway.enterCalls,
        2,
        reason: 'fresh credentials, not the old token',
      );
      expect(rooms, hasLength(2));
      expect(rooms.first.disposals, 1);
      expect(manager.snapshot.value.phase, LivePhase.live);
    },
  );

  test('an ending never returns to live', () async {
    await manager.enter(request);

    final heldEnd = Completer<EndedBroadcast>();
    gateway.holdEnd = heldEnd;
    final ending = manager.leaveOrEnd.runAsync();
    await pumpUntil(() => manager.snapshot.value.phase == LivePhase.ending);
    expect(manager.snapshot.value.phase, LivePhase.ending);

    rooms.single.emit(RoomConnectionState.connected);
    await pumpEventQueue();
    expect(manager.snapshot.value.phase, LivePhase.ending);

    heldEnd.complete(
      EndedBroadcast(
        broadcastId: Id.fromString(_broadcastId),
        title: SingleLineString('Morning show'),
        creatorId: Id.fromString(_viewer),
        duration: const Duration(minutes: 3),
        totalParticipants: 2,
      ),
    );
    await ending;

    expect(manager.snapshot.value.phase, LivePhase.ended);
  });

  test('the socket ending signal ends the room, once', () async {
    await manager.onDispose();
    final endings = StreamController<EndedSignal>.broadcast();
    addTearDown(endings.close);
    manager = build(ended: endings.stream);
    await manager.enter(request);

    endings.add(
      EndedSignal(
        broadcastId: Id.fromString(_broadcastId),
        reason: EndReason.adminForced,
      ),
    );
    await pumpUntil(() => manager.snapshot.value.phase == LivePhase.ended);

    expect(manager.snapshot.value.phase, LivePhase.ended);
    expect(manager.snapshot.value.endReason, EndReason.adminForced);
    expect(rooms.single.disposals, 1);
    expect(manager.hasSession, isFalse);
  });
}
