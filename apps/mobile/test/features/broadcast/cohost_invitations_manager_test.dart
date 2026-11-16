import 'dart:async';
import 'dart:convert';

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

import '../../_support/stub_api.dart';

const broadcastId = '4b5b0d0c-1f5e-4a0f-9a43-0f0f3a2b1c11';
const otherBroadcastId = '0f0f3a2b-1c11-4b5b-8d0c-1f5e4a0f9a43';
const invitationId = '9a6f2f7e-3c2b-4f5a-8f31-5f5b7d2c4e22';
const inviteeId = '1c2d3e4f-5a6b-4c7d-8e9f-0a1b2c3d4e33';

/// The API's envelope around a list, which [envelope] cannot express.
ResponseBody listEnvelope(List<Map<String, dynamic>> items) {
  return ResponseBody.fromString(
    jsonEncode({
      'code': 'OK',
      'message': '',
      'status': true,
      'data': items,
    }),
    200,
    headers: {
      Headers.contentTypeHeader: [Headers.jsonContentType],
    },
  );
}

/// The pending-list item the API answers with.
Map<String, dynamic> invitationJson() => {
  'id': invitationId,
  'broadcastId': broadcastId,
  'broadcastTitle': 'Sunday live',
  'inviterName': 'Ada',
  'status': 'pending',
  'createdAt': '2026-11-15T10:00:00Z',
};

CohostInvitationsManager managerWith(
  StubApi api,
  Stream<CohostEvent> events,
) => CohostInvitationsManager(
  http: BroadcastHttpService(stubClient(api)),
  events: events,
  logger: MockMenoLogger(),
);

void main() {
  late StreamController<CohostEvent> events;
  late StubApi api;
  late CohostInvitationsManager manager;

  setUp(() {
    events = StreamController<CohostEvent>();
    api = StubApi([
      () async => envelope(
        data: {
          'user': {'id': inviteeId, 'fullName': 'Ada'},
          'livekitToken': 'token',
        },
      ),
      () async => envelope(),
    ]);
    manager = CohostInvitationsManager(
      http: BroadcastHttpService(stubClient(api)),
      events: events.stream,
      logger: MockMenoLogger(),
    )..init();
  });

  tearDown(() async {
    await manager.onDispose();
    await events.close();
  });

  test(
    'an invitation frame becomes a pending invitation with its names',
    () async {
      events.add(
        CohostInvitedEvent(
          broadcastId: Id.fromString(broadcastId),
          invitationId: Id.fromString(invitationId),
          inviterName: 'Ada',
          broadcastTitle: 'Sunday live',
        ),
      );
      await pumpEventQueue();

      final invitation = manager.pending.value.single;
      expect(invitation.id.getOrCrash, invitationId);
      expect(invitation.broadcastId.getOrCrash, broadcastId);
      expect(invitation.prompt, 'Ada invited you to co-host "Sunday live"');
    },
  );

  test('accepting answers the invitation and forgets it', () async {
    events.add(
      CohostInvitedEvent(
        broadcastId: Id.fromString(broadcastId),
        invitationId: Id.fromString(invitationId),
        inviterName: 'Ada',
        broadcastTitle: 'Sunday live',
      ),
    );
    await pumpEventQueue();

    final session = await manager.accept.runAsync(manager.pending.value.single);

    expect(session?.livekitToken, 'token');
    expect(session?.user.fullName.getOrCrash, 'Ada');
    expect(
      api.requests.single.path,
      '/broadcasts/$broadcastId/co-hosts/$invitationId/accept',
    );
    expect(manager.pending.value, isEmpty);
  });

  test('declining forgets the invitation without a session', () async {
    events.add(
      CohostInvitedEvent(
        broadcastId: Id.fromString(broadcastId),
        invitationId: Id.fromString(invitationId),
        inviterName: 'Ada',
        broadcastTitle: 'Sunday live',
      ),
    );
    await pumpEventQueue();

    await manager.decline.runAsync(manager.pending.value.single);

    expect(
      api.requests.last.path,
      '/broadcasts/$broadcastId/co-hosts/$invitationId/decline',
    );
    expect(manager.pending.value, isEmpty);
  });

  test(
    'the handshake moving on drops the invitation for that broadcast',
    () async {
      events
        ..add(
          CohostInvitedEvent(
            broadcastId: Id.fromString(broadcastId),
            invitationId: Id.fromString(invitationId),
            inviterName: 'Ada',
            broadcastTitle: 'Sunday live',
          ),
        )
        ..add(
          CohostInvitedEvent(
            broadcastId: Id.fromString(otherBroadcastId),
            invitationId: Id.fromString('7d7d7d7d-1111-4222-8333-944494449444'),
            inviterName: 'Ada',
            broadcastTitle: 'Monday live',
          ),
        );
      await pumpEventQueue();
      expect(manager.pending.value, hasLength(2));

      events.add(
        CohostAcceptedEvent(
          broadcastId: Id.fromString(broadcastId),
          userId: Id.fromString(inviteeId),
          userName: 'Ada',
        ),
      );
      await pumpEventQueue();

      expect(manager.pending.value, hasLength(1));
      expect(
        manager.pending.value.single.broadcastId.getOrCrash,
        otherBroadcastId,
      );
    },
  );

  test('loading seeds the list the account missed while offline', () async {
    final loadApi = StubApi([() async => listEnvelope([invitationJson()])]);
    final localEvents = StreamController<CohostEvent>();
    addTearDown(localEvents.close);
    final loader = managerWith(loadApi, localEvents.stream)..init();
    addTearDown(loader.onDispose);

    await loader.loadPending.runAsync();

    expect(loadApi.requests.single.path, '/broadcasts/co-host-invitations');
    final invitation = loader.pending.value.single;
    expect(invitation.id.getOrCrash, invitationId);
    expect(invitation.broadcastId.getOrCrash, broadcastId);
    expect(invitation.prompt, 'Ada invited you to co-host "Sunday live"');
  });

  test('a list read does not resurrect a declined invitation', () async {
    final loadApi = StubApi([
      () async => envelope(),
      () async => listEnvelope([invitationJson()]),
    ]);
    final localEvents = StreamController<CohostEvent>();
    addTearDown(localEvents.close);
    final loader = managerWith(loadApi, localEvents.stream)..init();
    addTearDown(loader.onDispose);

    localEvents.add(
      CohostInvitedEvent(
        broadcastId: Id.fromString(broadcastId),
        invitationId: Id.fromString(invitationId),
        inviterName: 'Ada',
        broadcastTitle: 'Sunday live',
      ),
    );
    await pumpEventQueue();
    await loader.decline.runAsync(loader.pending.value.single);

    await loader.loadPending.runAsync();

    expect(loader.pending.value, isEmpty);
  });

  test('a run of frames for one invitation leaves one prompt', () async {
    for (var i = 0; i < 3; i++) {
      events.add(
        CohostInvitedEvent(
          broadcastId: Id.fromString(broadcastId),
          invitationId: Id.fromString(invitationId),
          inviterName: 'Ada',
          broadcastTitle: 'Sunday live',
        ),
      );
    }
    await pumpEventQueue();

    expect(manager.pending.value, hasLength(1));
  });
}
