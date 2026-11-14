import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/_shared/services/room_transport.dart';
import 'package:meno_mobile/_shared/services/secure_storage.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/auth_http_service.dart';
import 'package:meno_mobile/features/auth/services/session_service.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

const _jim = '018f2b1e-0000-7000-8000-000000000000';
const _pam = '018f2b1e-0000-7000-8000-000000000001';
const _broadcast = '018f2b1e-0000-7000-8000-000000000002';

class _MockPlatform extends Mock implements FlutterSecureStorage;

BroadcastSession _details() => BroadcastSession(
  userId: Id.fromString(_jim),
  broadcast: Broadcast(
    id: Id.fromString(_broadcast),
    title: SingleLineString('Morning show'),
    state: .live,
    status: .active,
    participantRole: .host,
    creator: Participant(
      id: Id.fromString(_jim),
      fullName: SingleLineString('Jim'),
    ),
  ),
  token: 'room-token',
  canPublish: true,
);

final class _FakeRoom implements RoomTransport {
  final _states = StreamController<RoomConnectionState>.broadcast();

  @override
  Stream<RoomConnectionState> get connectionState => _states.stream;

  @override
  RoomDisconnectReason? get lastDisconnectReason => null;

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

  @override
  FutureOr<dynamic> onDispose() => _states.close();
}

final class _FakeRoster implements LiveRoster {
  @override
  Future<void> attach() async {}

  @override
  Future<void> detach() async {}
}

/// A gateway whose next entry can be held open by the test.
final class _HoldingGateway implements LiveGateway {
  int enterCalls = 0;
  Completer<BroadcastSession>? holdNextEnter;

  @override
  Future<BroadcastSession> enter(
    LiveEnterRequest request,
    AccountContext context,
  ) {
    enterCalls++;
    final held = holdNextEnter;
    if (held != null) {
      holdNextEnter = null;
      return held.future;
    }
    return Future.value(_details());
  }

  @override
  Future<EndedBroadcast> end(Id broadcastId) async => EndedBroadcast(
    broadcastId: broadcastId,
    title: SingleLineString('Morning show'),
  );

  @override
  Future<void> leave(Id broadcastId) async {}
}

CredentialRecord _account(String id) => CredentialRecord(
  accountId: Id.fromString(id),
  schemaVersion: CredentialRecord.schemaVersionNumber,
  epoch: 'epoch-$id',
  revision: 1,
  accessToken: 'access',
  refreshToken: 'refresh',
  accessExpiresAt: DateTime.utc(2030),
  receivedAt: DateTime.utc(2026),
  isVerified: true,
  renewalState: RenewalState.idle,
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late LocalStorage storage;
  late LiveSessionStore liveStore;
  late StubApi api;

  setUp(() async {
    Command.globalExceptionHandler = (_, _) {};
    SharedPreferences.setMockInitialValues({});
    await di.reset();
    di.pushNewScope(scopeName: Scope.root);

    storage = LocalStorage(await SharedPreferences.getInstance());
    liveStore = LiveSessionStore(storage, userId: Id.fromString(_jim));
    api = StubApi();
  });

  tearDown(() async {
    Command.globalExceptionHandler = null;
    await di.reset();
  });

  ActiveBroadcastManager newLiveManager(_HoldingGateway gateway) {
    final manager = ActiveBroadcastManager(
      context: AccountContext(accountId: Id.fromString(_jim), generation: 1),
      gateway: gateway,
      store: liveStore,
      logger: MockMenoLogger(),
      createSession: ({required session, required exitOnServer}) =>
          ActiveBroadcastSession(
            session: session,
            room: _FakeRoom(),
            roster: _FakeRoster(),
            clock: const SystemClock(),
            logger: MockMenoLogger(),
            exitOnServer: exitOnServer,
          ),
    );
    addTearDown(manager.onDispose);
    return manager;
  }

  final request = LiveEnterRequest(
    broadcastId: Id.fromString(_broadcast),
    mode: LiveEnterMode.listen,
  );

  group('the live fence', () {
    test(
      'an exit during an open closes locally and publishes nothing',
      () async {
        final held = Completer<BroadcastSession>();
        final gateway = _HoldingGateway()..holdNextEnter = held;
        final live = newLiveManager(gateway);

        final entering = live.enter(request);
        await pumpUntil(() => gateway.enterCalls == 1);

        final exiting = live.closeForAccountExit();
        held.complete(_details());

        final result = await entering;
        await exiting;

        expect(result.outcome, EnterOutcome.rejected);
        expect(live.hasSession, isFalse);
        expect(live.snapshot.value.phase, LivePhase.idle);
      },
    );

    test('no entry is admitted once the exit has begun', () async {
      final gateway = _HoldingGateway();
      final live = newLiveManager(gateway);
      await live.closeForAccountExit();

      final result = await live.enter(request);

      expect(result.outcome, EnterOutcome.rejected);
      expect(gateway.enterCalls, 0);
      expect(live.hasSession, isFalse);
    });

    test('the exit waits for an entry that is still in flight', () async {
      final held = Completer<BroadcastSession>();
      final gateway = _HoldingGateway()..holdNextEnter = held;
      final live = newLiveManager(gateway);

      final entering = live.enter(request);
      await pumpUntil(() => gateway.enterCalls == 1);

      var closed = false;
      final exiting = live.closeForAccountExit().then((_) => closed = true);
      await pumpEventQueue();
      expect(
        closed,
        isFalse,
        reason: 'the transition must not finish while an open is in flight',
      );

      held.complete(_details());
      await exiting.timeout(const Duration(seconds: 2));
      expect(closed, isTrue);

      // The entry was released by the generation bump, not by the caller.
      await entering.timeout(const Duration(seconds: 2));
    });

    test('a failed drain leaves the old account scoped', () async {
      final platform = _MockPlatform();
      final disk = <String, String>{};
      when(() => platform.read(key: any(named: 'key'))).thenAnswer(
        (invocation) async => disk[invocation.namedArguments[#key]! as String],
      );
      when(
        () => platform.write(
          key: any(named: 'key'),
          value: any(named: 'value'),
        ),
      ).thenAnswer((invocation) async {
        disk[invocation.namedArguments[#key]! as String] =
            invocation.namedArguments[#value]! as String;
      });
      when(() => platform.delete(key: any(named: 'key')))
          .thenAnswer((invocation) async {
            disk.remove(invocation.namedArguments[#key]);
          });

      final store = SessionService(SecureStorage(platform));
      addTearDown(store.onDispose);
      await store.initialize();
      final auth = await AuthManager(
        http: AuthHttpService(stubClient(api)),
        store: store,
        clock: const SystemClock(),
        logger: MockMenoLogger(),
      ).init();
      addTearDown(auth.onDispose);

      final transition = await AccountTransitionOwner(
        activeRecord: auth.activeRecord,
        logger: MockMenoLogger(),
        beginLiveExit: () async => throw const RequestTimeoutException(),
      ).init();
      addTearDown(transition.onDispose);

      await store.adoptCandidate(_account(_jim));
      await store.activate(Id.fromString(_jim));
      await pumpUntil(() => di.isRegistered<CredentialRecord>());

      await store.adoptCandidate(_account(_pam));
      await store.activate(Id.fromString(_pam));
      await pumpUntil(
        () => auth.activeRecord.value?.accountId == Id.fromString(_pam),
      );
      await transition.ready;

      // The new account is not scoped: the transition is blocked until the
      // live tier can be closed, and the old account keeps what it owns.
      expect(di.currentScopeName, Scope.authenticated);
      expect(di<CredentialRecord>().accountId, Id.fromString(_jim));
      expect(di<CredentialRecord>().accountId, isNot(Id.fromString(_pam)));
    });
  });
}
