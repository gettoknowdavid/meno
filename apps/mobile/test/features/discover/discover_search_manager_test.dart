//
// ignore_for_file: lines_longer_than_80_chars

import 'dart:async';

import 'package:dio/dio.dart' show ResponseBody;
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/discover/discover.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/stub_api.dart';

const _viewer = 'a84f11da-43e5-4aef-9f29-e5e400883b09';
const _id1 = '344f0166-230b-4cb3-99ec-818b82caa3fc';
const _id2 = '561bc5aa-568a-452b-bafa-706855167495';
const _id3 = 'cf83f3be-6f7f-4ec6-869c-50d507394f94';
String _id(int n) => switch (n) {
  1 => _id1,
  2 => _id2,
  3 => _id3,
  _ => throw ArgumentError.value(n),
};

/// One row of `GET /broadcasts`, as the API frames it.
///
/// `active` with no `end_time` is on air; anything else has to carry an
/// `end_time` to count as aired, which is the distinction the API's keyword
/// filter uses to keep drafts out of a search.
Map<String, dynamic> _broadcast(
  int n, {
  String title = 'The Purpose',
  bool live = true,
}) => {
  'id': _id(n),
  'title': title,
  'status': live ? 'active' : 'inactive',
  'state': live ? 'live' : 'ended',
  'totalParticipants': 0,
  'creatorId': _viewer,
  'creatorName': 'David Michael',
  'description': null,
  'timeZone': 'Africa/Lagos',
  'imageUrl': null,
  'imageId': null,
  'createdAt': '2026-10-28T09:15:00Z',
  'startTime': '2026-10-28T09:15:00Z',
  'endTime': live ? null : '2026-10-28T10:15:00Z',
};

/// One page of `GET /broadcasts`, as the API frames it.
Future<ResponseBody> _page(
  List<Map<String, dynamic>> broadcasts, {
  String? cursor,
}) async => envelope(
  data: {
    'data': broadcasts,
    'next_cursor': cursor,
    'has_next_page': cursor != null,
  },
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late StubApi api;
  late LocalStorage storage;
  late DiscoverSearchManager manager;

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    // An unclaimed failure has to have somewhere to go; the app installs a
    // logger in main(), and these tests only need the slot filled.
    Command.globalExceptionHandler = (_, _) {};
    api = StubApi();
    storage = LocalStorage(await SharedPreferences.getInstance());
    manager = DiscoverSearchManager(
      http: BroadcastHttpService(stubClient(api)),
      local: DiscoverLocalService(Id.fromString(_viewer), storage),
    );
  });

  tearDown(() async {
    await manager.onDispose();
  });

  group('DiscoverSearchManager', () {
    test('a term too short to ask for is not sent, and clears the list', () async {
      api.answers.add(() => _page([_broadcast(1)]));

      await manager.search.runAsync('the');
      expect(manager.feed.itemCount.value, 1);

      await manager.search.runAsync('th');

      // Too short to be worth a request, so nothing was sent and the previous
      // term's rows are gone rather than left standing.
      expect(api.requests, hasLength(1));
      expect(manager.feed.itemCount.value, 0);
      expect(manager.hasMore.value, isFalse);
      expect(manager.recentSearches.value, ['the']);
    });

    test('searching asks the broadcast endpoint by keyword and records it', () async {
      api.answers.add(
        () => _page(
          [
            _broadcast(1),
            _broadcast(2, title: 'The Purpose, again', live: false),
          ],
          cursor: 'c2',
        ),
      );

      await manager.search.runAsync('  the purpose  ');

      final request = api.requests.single;
      // The bug this records: the search used to read accounts, so a title
      // plainly in the list was never found. The keyword now goes where the
      // broadcasts are.
      expect(request.path, '/broadcasts');
      expect(request.queryParameters['q'], 'the purpose');
      expect(manager.query.value, 'the purpose');
      expect(manager.feed.items.map((broadcast) => broadcast.id.getOrCrash), [
        _id(1),
        _id(2),
      ]);
      expect(manager.hasMore.value, isTrue);

      // Only a search that found something is remembered, and the trimmed term
      // is what is remembered.
      expect(manager.recentSearches.value, ['the purpose']);
      expect(
        storage.getStringList(
          StorageKeys.forUser(StorageKeys.recentSearches, _viewer),
        ),
        ['the purpose'],
      );
    });

    test('a search that found nothing is not remembered', () async {
      api.answers.add(() => _page(const []));

      await manager.search.runAsync('zzz');

      expect(manager.feed.itemCount.value, 0);
      expect(manager.recentSearches.value, isEmpty);
    });

    test('fetchMore reads the cursor page and appends it', () async {
      api.answers.addAll([
        () => _page([_broadcast(1), _broadcast(2)], cursor: 'c2'),
        () => _page([_broadcast(3)]),
      ]);

      await manager.search.runAsync('purpose');

      await manager.fetchMore.runAsync();

      expect(api.requests, hasLength(2));
      expect(api.requests.last.queryParameters['cursor'], 'c2');
      expect(manager.feed.itemCount.value, 3);
      expect(manager.hasMore.value, isFalse);
    });

    test('a page that repeats a broadcast does not add it twice', () async {
      api.answers.addAll([
        () => _page([_broadcast(1), _broadcast(2)], cursor: 'c2'),
        () => _page([_broadcast(2), _broadcast(3)]),
      ]);

      await manager.search.runAsync('purpose');

      await manager.fetchMore.runAsync();

      expect(manager.feed.itemCount.value, 3);
    });

    test('a failed page is reported once and waits for the user', () async {
      api.answers.addAll([
        () => _page([_broadcast(1)], cursor: 'c2'),
        () => Future.value(envelope(status: 500)),
        () => _page([_broadcast(2)]),
      ]);

      await manager.search.runAsync('purpose');

      await manager.fetchMore.runAsync();

      expect(manager.feed.pageError.value, isNotNull);
      // A failure must not turn into a request loop on the next rebuild.
      await manager.fetchMore.runAsync();
      expect(api.requests, hasLength(2));

      await manager.retryPage();

      expect(manager.feed.pageError.value, isNull);
      expect(api.requests, hasLength(3));
      expect(manager.feed.itemCount.value, 2);
    });

    test(
      'removing and clearing the history updates what the screen reads',
      () async {
        api.answers.addAll([
          () => _page([_broadcast(1)]),
          () => _page([_broadcast(2)]),
        ]);

        await manager.search.runAsync('purpose');
        await manager.search.runAsync('grace');

        expect(manager.recentSearches.value, ['grace', 'purpose']);

        await manager.removeRecentSearch.runAsync('grace');
        expect(manager.recentSearches.value, ['purpose']);

        await manager.clearRecentSearches.runAsync();
        expect(manager.recentSearches.value, isEmpty);
      },
    );

    test('the history survives a manager built again for the same account', () async {
      api.answers.add(() => _page([_broadcast(1)]));
      await manager.search.runAsync('purpose');

      // A second manager over the same account and device reads what the first
      // one stored — that is what "recent" means across a restart.
      final again = DiscoverSearchManager(
        http: BroadcastHttpService(stubClient(StubApi())),
        local: DiscoverLocalService(Id.fromString(_viewer), storage),
      );
      addTearDown(again.onDispose);

      expect(again.recentSearches.value, ['purpose']);
      expect(again.feed.itemCount.value, 0);
    });

    test(
      'a disposed manager ignores the answer it was still waiting for',
      () async {
        final gates = <_Gate>[_Gate()];
        api.answers.add(() async {
          final body = await gates.first.opened;
          return body;
        });

        final searching = manager.search.runAsync('purpose');
        await manager.onDispose();
        gates.first.release(await _page([_broadcast(1)]));
        await searching;
      },
    );
  });
}

/// An answer a test opens when it feels like it.
final class _Gate {
  final _completer = Completer<ResponseBody>();

  Future<ResponseBody> get opened => _completer.future;

  void release(ResponseBody body) => _completer.complete(body);
}
