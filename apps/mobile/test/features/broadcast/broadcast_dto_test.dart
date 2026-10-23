import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/features/broadcast/model/dtos.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';

/// A `GET /broadcasts/{id}` response, exactly as
/// `apps/api/src/modules/broadcast/dto.rs` serialises one: every key camelCase
/// (§6.2), `endReason` null while it is still on air.
///
/// This is the Tier 1.1 gate — the client must parse the field names the
/// backend actually sends, not the snake_case ones it used to send.
Map<String, dynamic> _liveFixture() => {
  'id': 'b-1',
  'title': 'Ada\u2019s show',
  'description': 'Every Thursday',
  'timeZone': 'Africa/Lagos',
  'imageUrl': 'https://cdn.test/cover.jpg',
  'imageId': 'img-123',
  'createdAt': '2026-10-01T10:00:00Z',
  'startTime': null,
  'endTime': null,
  'publishedAt': '2026-10-01T10:05:00Z',
  'durationSeconds': null,
  'status': 'active',
  'state': 'live',
  'participantRole': 'cohost',
  'isJoined': true,
  'isBookmarked': true,
  'liveParticipantsCount': 18,
  'totalParticipants': 42,
  'recordingEnabled': true,
  'recordingUrl': null,
  'endReason': null,
  'timeRemainingSeconds': 1480,
  'creatorId': 'u-9',
  'creator': {'id': 'u-9', 'fullName': 'Ada Lovelace', 'avatarId': 'a-1'},
  'cohosts': [
    {'id': 'u-2', 'fullName': 'Grace Hopper'},
  ],
};

void main() {
  group('BroadcastDto parses the camelCase response the backend sends', () {
    test('every multi-word field survives a real fixture', () {
      final broadcast = BroadcastDto.fromJson(_liveFixture()).toDomain;

      // Values that are dropped the moment a key is misnamed: a snake_case read
      // of a camelCase field silently yields the default, which is exactly the
      // defect this test exists to catch.
      expect(broadcast.timeZone, 'Africa/Lagos');
      expect(broadcast.image?.toUrlOrFilePathOrNull, 'https://cdn.test/cover.jpg');
      expect(broadcast.imageId, 'img-123');
      expect(broadcast.publishedAt, isNotNull);
      expect(broadcast.participantRole, ParticipantRole.cohost);
      expect(broadcast.isJoined, isTrue);
      expect(broadcast.isBookmarked, isTrue);
      expect(broadcast.liveParticipantsCount, 18);
      expect(broadcast.totalParticipants, 42);
      expect(broadcast.recordingEnabled, isTrue);
      expect(broadcast.timeRemainingSeconds, 1480);
      expect(broadcast.creator?.fullName.getOrCrash, 'Ada Lovelace');
      expect(broadcast.creator?.id.getOrCrash, 'u-9');
      expect(broadcast.cohosts.single.fullName.getOrCrash, 'Grace Hopper');
    });

    test('a live broadcast has no end reason, and null reads as none', () {
      final broadcast = BroadcastDto.fromJson(_liveFixture()).toDomain;

      // The wire carries `endReason: null` for a live broadcast; the domain maps
      // that to `EndReason.none` rather than inventing a reason.
      expect(broadcast.endReason, EndReason.none);
      expect(broadcast.state, BroadcastState.live);
      expect(broadcast.isLive, isTrue);
      expect(broadcast.hasEnded, isFalse);
    });

    test('an ended broadcast carries its reason', () {
      final broadcast = BroadcastDto.fromJson({
        ..._liveFixture(),
        'state': 'ended',
        'status': 'inactive',
        'endReason': 'host_disconnected',
        'endTime': '2026-10-01T10:30:00Z',
      }).toDomain;

      expect(broadcast.endReason, EndReason.hostDisconnected);
      expect(broadcast.state, BroadcastState.ended);
      expect(broadcast.hasEnded, isTrue);
    });
  });

  group('BroadcastSessionDto', () {
    test('parses the { broadcast, token } envelope', () {
      final session = BroadcastSessionDto.fromJson({
        'broadcast': _liveFixture(),
        'token': 'room-token',
      });

      expect(session.token, 'room-token');
      expect(session.broadcast.id, 'b-1');
      expect(session.toDomain('u-9').token, 'room-token');
    });
  });

  group('UserSummaryDto', () {
    test('parses camelCase names', () {
      final user = UserSummaryDto.fromJson({
        'id': 'u-1',
        'fullName': 'Ada Lovelace',
        'avatarId': null,
        'avatarUrl': 'https://cdn.test/ada.jpg',
      });

      expect(user.fullName, 'Ada Lovelace');
      expect(user.avatarUrl, 'https://cdn.test/ada.jpg');
    });
  });

  group('EndBroadcastDto', () {
    test('parses the endedBroadcast camelCase payload', () {
      final ended = EndBroadcastDto.fromJson({
        'broadcastId': 'b-1',
        'broadcastTitle': 'Ada\u2019s show',
        'broadcastImageUrl': null,
        'creatorId': 'u-9',
        'creatorName': 'Ada Lovelace',
        'endedReason': 'normal',
        'endedAt': '2026-10-01T10:30:00Z',
        'durationSecs': 1500,
        'totalParticipants': 42,
        'recordingEnabled': true,
      }).toDomain;

      expect(ended.broadcastId.getOrCrash, 'b-1');
      expect(ended.creatorName.getOrCrash, 'Ada Lovelace');
      expect(ended.reason, EndReason.normal);
      expect(ended.duration, const Duration(seconds: 1500));
      expect(ended.totalParticipants, 42);
    });
  });
}
