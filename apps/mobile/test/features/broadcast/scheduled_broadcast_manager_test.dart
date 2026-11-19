import 'package:dio/dio.dart' show ResponseBody;
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

import '../../_support/stub_api.dart';

const _account = '018f2b1e-0000-7000-8000-000000000040';
const _soonest = '018f2b1e-0000-7000-8000-0000000000a1';
const _later = '018f2b1e-0000-7000-8000-0000000000a2';

/// A scheduled row as `GET /broadcasts` lists it.
Map<String, dynamic> _item(String id, {required String startTime}) => {
  'id': id,
  'title': 'Scheduled $id',
  'state': 'scheduled',
  'status': 'inactive',
  'createdAt': '2026-11-19T09:00:00Z',
  'startTime': startTime,
  'totalParticipants': 0,
  'creatorId': _account,
  'creatorName': 'Ada',
};

Future<ResponseBody> Function() _page(List<Map<String, dynamic>> items) =>
    () async => envelope(
      data: {'data': items, 'next_cursor': null, 'has_next_page': false},
    );

final Future<ResponseBody> Function() _empty = _page(const []);

/// The record `PATCH /broadcasts/{id}` answers with: one broadcast, not a page.
Future<ResponseBody> Function() _updated() => () async => envelope(
  data: {
    'id': _soonest,
    'title': 'Scheduled $_soonest',
    'status': 'inactive',
    'state': 'scheduled',
    'participantRole': 'host',
    'isJoined': false,
    'isBookmarked': false,
    'liveParticipantsCount': 0,
    'totalParticipants': 0,
    'recordingEnabled': false,
    'cohosts': <Map<String, dynamic>>[],
    'createdAt': '2026-11-19T09:00:00Z',
    'startTime': '2026-11-21T13:37:00Z',
    'endTime': null,
  },
);

void main() {
  setUp(() => Command.globalExceptionHandler = (_, _) {});
  tearDown(() => Command.globalExceptionHandler = null);

  ScheduledBroadcastManager managerFor(StubApi api) {
    final manager = ScheduledBroadcastManager(
      http: BroadcastHttpService(stubClient(api)),
      accountId: Id.fromString(_account),
      logger: MockMenoLogger(),
    );
    addTearDown(manager.onDispose);
    return manager;
  }

  group('reading the open schedule', () {
    test('asks for this account and keeps the earliest one', () async {
      final api = StubApi([
        _page([
          _item(_later, startTime: '2026-11-20T18:00:00Z'),
          _item(_soonest, startTime: '2026-11-20T10:00:00Z'),
        ]),
      ]);
      final manager = managerFor(api);

      await manager.refresh.runAsync();

      expect(api.requests.single.path, '/broadcasts');
      expect(api.requests.single.queryParameters['creatorId'], _account);
      expect(manager.next.value?.id.getOrCrash, _soonest);
    });

    test('a start more than five minutes out can be hidden', () async {
      final api = StubApi([
        _page([_item(_soonest, startTime: '2026-11-20T10:00:00Z')]),
      ]);
      final manager = managerFor(api);
      await manager.refresh.runAsync();

      expect(manager.isImminent.value, isFalse);
      expect(manager.canHide, isTrue);
      expect(manager.isVisible, isTrue);

      manager.hide();

      expect(manager.hidden.value, isTrue);
      expect(manager.isVisible, isFalse, reason: 'hidden until the reminder');
    });

    test('a start inside the window cannot be hidden', () async {
      final start = DateTime.now().toUtc().add(const Duration(minutes: 3));
      final api = StubApi([
        _page([_item(_soonest, startTime: start.toIso8601String())]),
      ]);
      final manager = managerFor(api);

      await manager.refresh.runAsync();

      expect(manager.isImminent.value, isTrue);
      expect(manager.canHide, isFalse);
      expect(
        manager.isVisible,
        isTrue,
        reason: 'the prompt is not dismissible',
      );
    });

    test('a start well past its time is overdue, not merely due', () async {
      final start = DateTime.now().toUtc().subtract(
        const Duration(minutes: 20),
      );
      final api = StubApi([
        _page([_item(_soonest, startTime: start.toIso8601String())]),
      ]);
      final manager = managerFor(api);

      await manager.refresh.runAsync();

      // Nothing starts it: the host is the one who goes live. Past the grace
      // window the banner says the moment was missed and offers the sheet.
      expect(manager.isDue.value, isTrue);
      expect(manager.isOverdue.value, isTrue);
    });

    test('a start that is a few minutes late is not overdue yet', () async {
      final start = DateTime.now().toUtc().subtract(
        const Duration(minutes: 2),
      );
      final api = StubApi([
        _page([_item(_soonest, startTime: start.toIso8601String())]),
      ]);
      final manager = managerFor(api);

      await manager.refresh.runAsync();

      expect(manager.isDue.value, isTrue);
      expect(
        manager.isOverdue.value,
        isFalse,
        reason: 'the grace window is still running',
      );
    });

    test('a new schedule gets its banner back after one was hidden', () async {
      final api = StubApi([
        _page([_item(_soonest, startTime: '2026-11-20T10:00:00Z')]),
        _page([_item(_later, startTime: '2026-11-21T10:00:00Z')]),
      ]);
      final manager = managerFor(api);

      await manager.refresh.runAsync();
      manager.hide();
      expect(manager.hidden.value, isTrue);

      await manager.refresh.runAsync();

      expect(manager.next.value?.id.getOrCrash, _later);
      expect(manager.hidden.value, isFalse);
    });
  });

  group('resolving the open schedule', () {
    test('cancel deletes it and re-reads', () async {
      final api = StubApi([
        _page([_item(_soonest, startTime: '2026-11-20T10:00:00Z')]),
        _empty,
        _empty,
      ]);
      final manager = managerFor(api);
      await manager.refresh.runAsync();

      await manager.cancel.runAsync(Id.fromString(_soonest));

      final remove = api.requests[1];
      expect(remove.method, 'DELETE');
      expect(remove.path, '/broadcasts/$_soonest');
      expect(manager.next.value, isNull);
    });

    test('reschedule patches the instant it was given, in UTC', () async {
      final api = StubApi([
        _page([_item(_soonest, startTime: '2026-11-20T10:00:00Z')]),
        _updated(),
        _empty,
      ]);
      final manager = managerFor(api);
      await manager.refresh.runAsync();

      // A wall clock the user picked: the API stores the instant it names.
      final chosen = DateTime(2026, 11, 21, 14, 37);
      await manager.reschedule.runAsync((
        id: Id.fromString(_soonest),
        start: chosen,
        zone: 'Africa/Lagos',
      ));

      final patch = api.requests[1];
      expect(patch.method, 'PATCH');
      expect(patch.path, '/broadcasts/$_soonest');
      final body = patch.data as Map<String, dynamic>;
      expect(body['start_time'], chosen.toUtc().toIso8601String());
      expect(body['time_zone'], 'Africa/Lagos');
    });
  });
}
