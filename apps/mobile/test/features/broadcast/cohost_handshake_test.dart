import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

import '../../_support/stub_api.dart';

/// A broadcast, an invitation and an account, as ids the wire accepts.
const broadcastId = '4b5b0d0c-1f5e-4a0f-9a43-0f0f3a2b1c11';
const invitationId = '9a6f2f7e-3c2b-4f5a-8f31-5f5b7d2c4e22';
const inviteeId = '1c2d3e4f-5a6b-4c7d-8e9f-0a1b2c3d4e33';

void main() {
  group('CohostInvitationDto', () {
    test('reads the invitation the invite endpoint answers', () {
      final dto = CohostInvitationDto.fromJson({
        'id': invitationId,
        'broadcastId': broadcastId,
        'invitee': {'id': inviteeId, 'fullName': 'Ada', 'avatarId': null},
        'status': 'pending',
        'createdAt': '2026-01-02T03:04:05Z',
      });

      final invitation = dto.toDomain;

      expect(invitation.id.getOrCrash, invitationId);
      expect(invitation.broadcastId.getOrCrash, broadcastId);
      expect(invitation.status, InvitationStatus.pending);
      expect(invitation.createdAt, DateTime.utc(2026, 1, 2, 3, 4, 5));
    });

    test('reads a status this client does not know as still pending', () {
      final invitation = CohostInvitationDto.fromJson({
        'id': invitationId,
        'broadcastId': broadcastId,
        'status': 'something-new',
      }).toDomain;

      expect(invitation.status, InvitationStatus.pending);
    });
  });

  group('CohostSessionDto', () {
    test('reads the accepted invitation and its token', () {
      final session = CohostSessionDto.fromJson({
        'user': {'id': inviteeId, 'fullName': 'Ada'},
        'livekitToken': 'token',
      }).toDomain;

      expect(session.user.id.getOrCrash, inviteeId);
      expect(session.user.fullName.getOrCrash, 'Ada');
      expect(session.livekitToken, 'token');
    });

    test('refuses a session with no user', () {
      expect(
        () => CohostSessionDto.fromJson({'livekitToken': 'token'}),
        throwsA(isA<FormatException>()),
      );
    });
  });

  group('BroadcastHttpService co-host handshake', () {
    test('invites on the co-hosts path and reads the invitation', () async {
      final api = StubApi([
        () async => envelope(
          data: {
            'id': invitationId,
            'broadcastId': broadcastId,
            'status': 'pending',
          },
        ),
      ]);
      final http = BroadcastHttpService(stubClient(api));

      final invitation = await http.inviteCohost(
        Id.fromString(broadcastId),
        Id.fromString(inviteeId),
      );

      expect(invitation.id.getOrCrash, invitationId);
      final request = api.requests.single;
      expect(request.path, '/broadcasts/$broadcastId/co-hosts');
      expect(request.data, {'cohost': inviteeId});
    });

    test('accepts with the invitation id in the path', () async {
      final api = StubApi([
        () async => envelope(
          data: {
            'user': {'id': inviteeId, 'fullName': 'Ada'},
          },
        ),
      ]);
      final http = BroadcastHttpService(stubClient(api));

      final session = await http.acceptCohostInvitation(
        Id.fromString(broadcastId),
        Id.fromString(invitationId),
      );

      expect(session.user.fullName.getOrCrash, 'Ada');
      expect(
        api.requests.single.path,
        '/broadcasts/$broadcastId/co-hosts/$invitationId/accept',
      );
    });

    test('declines with the invitation id in the path', () async {
      final api = StubApi();
      final http = BroadcastHttpService(stubClient(api));

      await http.declineCohostInvitation(
        Id.fromString(broadcastId),
        Id.fromString(invitationId),
      );

      expect(
        api.requests.single.path,
        '/broadcasts/$broadcastId/co-hosts/$invitationId/decline',
      );
    });
  });
}
