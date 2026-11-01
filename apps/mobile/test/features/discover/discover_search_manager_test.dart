//
// ignore_for_file: lines_longer_than_80_chars

import 'dart:async';

import 'package:dio/dio.dart' show ResponseBody;
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/features/discover/discover.dart';
import 'package:meno_mobile/features/profile/profile.dart';
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

Map<String, dynamic> _account(int n, {String name = 'Ada'}) => {
  'id': _id(n),
  'full_name': name,
  'bio': null,
  'avatar_url': null,
  'is_following': false,
  'followers': 0,
  'following': 0,
  'broadcasts': 0,
  'created_at': null,
};

/// One page of `GET /users`, as the API frames it.
Future<ResponseBody> _page(
  List<Map<String, dynamic>> accounts, {
  String? cursor,
}) async => envelope(
  data: {
    'data': accounts,
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
    api = StubApi();
    storage = LocalStorage(await SharedPreferences.getInstance());
    manager = DiscoverSearchManager(
      http: ProfileHttpService(stubClient(api)),
      local: DiscoverLocalService(Id.fromString(_viewer), storage),
    );
  });

  tearDown(() async {
    await manager.onDispose();
  });

  group('DiscoverSearchManager', () {
    test('a term too short to ask for is not sent, and clears the list', () async {
      api.answers.add(() => _page([_account(1)]));

      await manager.search.runAsync('ada');
      expect(manager.feed.itemCount.value, 1);

      await manager.search.runAsync('ad');

      // The API refuses a term under three characters, so nothing was sent and
      // the previous term's rows are gone rather than left standing.
      expect(api.requests, hasLength(1));
      expect(manager.feed.itemCount.value, 0);
      expect(manager.hasMore.value, isFalse);
      expect(manager.recentSearches.value, ['ada']);
    });

    test('searching asks the profile endpoint and records the term', () async {
      api.answers.add(() => _page([_account(1), _account(2)], cursor: 'c2'));

      await manager.search.runAsync('  ada  ');

      final request = api.requests.single;
      expect(request.path, '/users');
      expect(request.queryParameters['q'], 'ada');
      expect(manager.query.value, 'ada');
      expect(manager.feed.items.map((profile) => profile.id.getOrCrash), [
        _id(1),
        _id(2),
      ]);
      expect(manager.hasMore.value, isTrue);

      // Only a search that found somebody is remembered, and the trimmed term
      // is what is remembered.
      expect(manager.recentSearches.value, ['ada']);
      expect(
        storage.getStringList(
          StorageKeys.forUser(StorageKeys.recentSearches, _viewer),
        ),
        ['ada'],
      );
    });

    test('a search that found nobody is not remembered', () async {
      api.answers.add(() => _page(const []));

      await manager.search.runAsync('zzz');

      expect(manager.feed.itemCount.value, 0);
      expect(manager.recentSearches.value, isEmpty);
    });

    test('fetchMore reads the cursor page and appends it', () async {
      api.answers.addAll([
        () => _page([_account(1), _account(2)], cursor: 'c2'),
        () => _page([_account(3)]),
      ]);

      await manager.search.runAsync('ada');
      await manager.fetchMore.runAsync();

      expect(api.requests, hasLength(2));
      expect(api.requests.last.queryParameters['cursor'], 'c2');
      expect(manager.feed.itemCount.value, 3);
      expect(manager.hasMore.value, isFalse);
    });

    test('a page that repeats an account does not add it twice', () async {
      api.answers.addAll([
        () => _page([_account(1), _account(2)], cursor: 'c2'),
        () => _page([_account(2), _account(3)]),
      ]);

      await manager.search.runAsync('ada');
      await manager.fetchMore.runAsync();

      expect(manager.feed.itemCount.value, 3);
    });

    test('a failed page is reported once and waits for the user', () async {
      api.answers.addAll([
        () => _page([_account(1)], cursor: 'c2'),
        () => Future.value(envelope(status: 500)),
        () => _page([_account(2)]),
      ]);

      await manager.search.runAsync('ada');
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
          () => _page([_account(1)]),
          () => _page([_account(2)]),
        ]);

        await manager.search.runAsync('ada');
        await manager.search.runAsync('grace');
        expect(manager.recentSearches.value, ['grace', 'ada']);

        await manager.removeRecentSearch.runAsync('grace');
        expect(manager.recentSearches.value, ['ada']);

        await manager.clearRecentSearches.runAsync();
        expect(manager.recentSearches.value, isEmpty);
      },
    );

    test('the history survives a manager built again for the same account', () async {
      api.answers.add(() => _page([_account(1)]));
      await manager.search.runAsync('ada');

      // A second manager over the same account and device reads what the first
      // one stored — that is what "recent" means across a restart.
      final again = DiscoverSearchManager(
        http: ProfileHttpService(stubClient(StubApi())),
        local: DiscoverLocalService(Id.fromString(_viewer), storage),
      );
      addTearDown(again.onDispose);

      expect(again.recentSearches.value, ['ada']);
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

        final searching = manager.search.runAsync('ada');
        await manager.onDispose();
        gates.first.release(await _page([_account(1)]));
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
