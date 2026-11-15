import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/_shared/services/media_service.dart';
import 'package:meno_mobile/features/auth/model/auth_context.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/objectbox.g.dart' hide Id;
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/stub_api.dart';

const _account = '018f2b1e-0000-7000-8000-000000000040';
const _broadcastId = '018f2b1e-0000-7000-8000-0000000000aa';
const _ada = '018f2b1e-0000-7000-8000-0000000000a1';
const _grace = '018f2b1e-0000-7000-8000-0000000000a2';

/// A created broadcast, with only the fields the API always sends.
Map<String, dynamic> _broadcastJson() => {
  'id': _broadcastId,
  'title': 'Sunday Service',
  'status': 'inactive',
  'state': 'draft',
  'participantRole': 'host',
  'isJoined': true,
  'isBookmarked': false,
  'liveParticipantsCount': 1,
  'totalParticipants': 1,
  'recordingEnabled': false,
  'cohosts': <Map<String, dynamic>>[],
};

/// `CohostInvitationResponse` for [invitee].
Map<String, dynamic> _invitationJson(String invitee) => {
  'id': '018f2b1e-0000-7000-8000-0000000000${invitee.substring(34)}',
  'broadcastId': _broadcastId,
  'status': 'pending',
  'invitee': {'id': invitee, 'fullName': 'Invited account'},
};

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late Store store;
  late StubApi api;
  late BroadcastEditorManager manager;

  setUp(() async {
    Command.globalExceptionHandler = (_, _) {};
    SharedPreferences.setMockInitialValues({});
    store = Store(
      getObjectBoxModel(),
      directory:
          '${Store.inMemoryPrefix}'
          'editor-cohosts-${DateTime.now().microsecondsSinceEpoch}',
    );

    final live = ActiveBroadcastManager(
      context: AccountContext(
        accountId: Id.fromString(_account),
        generation: 1,
      ),
      gateway: _NoGateway(),
      store: LiveSessionStore(
        LocalStorage(await SharedPreferences.getInstance()),
        userId: Id.fromString(_account),
      ),
      logger: MockMenoLogger(),
      createSession: ({required session, required exitOnServer}) =>
          throw StateError('no room in this test'),
    );

    api = StubApi();
    manager = BroadcastEditorManager(
      http: BroadcastHttpService(stubClient(api)),
      local: BroadcastLocalService(
        Id.fromString(_account),
        Database.fromStore(store),
        logger: MockMenoLogger(),
      ),
      media: MediaService(),
      active: live,
      userId: Id.fromString(_account),
      logger: MockMenoLogger(),
    );

    addTearDown(() async {
      await manager.onDispose();
      await live.onDispose();
    });
  });

  tearDown(() async {
    Command.globalExceptionHandler = null;
    store.close();
  });

  BroadcastDraft draftWith(List<String> cohosts) => BroadcastDraft(
    id: Id.fromString('018f2b1e-0000-7000-8000-0000000000bb'),
    ownerId: Id.fromString(_account),
    title: 'Sunday Service',
    description: 'A morning together',
    cohosts: [for (final id in cohosts) Id.fromString(id)],
    idempotencyKey: '018f2b1e-0000-7000-8000-0000000000c0',
  );

  test(
    'creation invites each picked cohost instead of sending the list',
    () async {
      api.answers.addAll([
        () async => envelope(data: _broadcastJson()),
        () async => envelope(data: _invitationJson(_ada)),
        () async => envelope(data: _invitationJson(_grace)),
      ]);

      await manager.createBroadcast.runAsync(draftWith([_ada, _grace]));

      expect(api.requests, hasLength(3));

      final create = api.requests.first;
      expect(create.path, '/broadcasts');
      final body = create.data as Map<String, dynamic>;
      expect(
        body.containsKey('cohosts'),
        isFalse,
        reason: 'cohosts must go through the invitation handshake',
      );

      expect(
        api.requests[1].path,
        '/broadcasts/$_broadcastId/co-hosts',
        reason: 'the invitation needs the row to exist first',
      );
      expect(api.requests[1].data, {'cohost': _ada});
      expect(api.requests[2].data, {'cohost': _grace});

      expect(manager.createdBroadcastId?.getOrCrash, _broadcastId);
      expect(manager.step.value, BroadcastCreationStep.created);
    },
  );

  test('a retry invites only the cohosts not yet invited', () async {
    api.answers.addAll([
      () async => envelope(data: _broadcastJson()),
      () async => envelope(data: _invitationJson(_ada)),
      () async => envelope(status: 500),
      () async => envelope(data: _invitationJson(_grace)),
    ]);

    final draft = draftWith([_ada, _grace]);

    await expectLater(
      manager.createBroadcast.runAsync(draft),
      throwsA(isA<MenoException>()),
    );

    // The row exists and the first invite went out; the second failed. The
    // state is `.created`, so the next press resumes instead of creating again.
    expect(api.requests, hasLength(3));
    expect(manager.createdBroadcastId?.getOrCrash, _broadcastId);
    expect(manager.step.value, BroadcastCreationStep.created);

    await manager.createBroadcast.runAsync(draft);

    expect(api.requests, hasLength(4), reason: 'no second broadcast');
    expect(
      api.requests.where((request) => request.path == '/broadcasts'),
      hasLength(1),
    );
    expect(api.requests.last.path, '/broadcasts/$_broadcastId/co-hosts');
    expect(api.requests.last.data, {'cohost': _grace});
    expect(
      api.requests.where(
        (request) =>
            request.data is Map && (request.data as Map)['cohost'] == _ada,
      ),
      hasLength(1),
      reason: 'the cohost already invited is not asked twice',
    );
  });
}

final class _NoGateway implements LiveGateway {
  @override
  Future<BroadcastSession> enter(
    LiveEnterRequest request,
    AccountContext context,
  ) => throw StateError('no room in this test');

  @override
  Future<EndedBroadcast> end(Id broadcastId) =>
      throw StateError('no room in this test');

  @override
  Future<void> leave(Id broadcastId) async {}
}
