import 'package:dio/dio.dart' show ResponseBody;
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

/// An empty page, as `GET /broadcasts` answers with one.
Future<ResponseBody> Function() _emptyPage({String? nextCursor}) =>
    () async => envelope(
      data: {
        'data': <Map<String, dynamic>>[],
        'next_cursor': nextCursor,
        'has_next_page': nextCursor != null,
      },
    );

/// A broadcast as `GET /broadcasts` would list it.
///
/// camelCase keys, matching what `BroadcastResponse` puts on the wire.
Map<String, dynamic> _item(String id, {required String state}) => {
  'id': id,
  'title': id,
  'state': state,
  'status': state == 'live' ? 'active' : 'inactive',
  'createdAt': '2026-01-01T00:00:00Z',
  'totalParticipants': 0,
  'creatorId': 'creator-1',
  'creatorName': 'Ada',
};

/// One page carrying [items].
Future<ResponseBody> Function() _page(List<Map<String, dynamic>> items) =>
    () async => envelope(
      data: {'data': items, 'next_cursor': null, 'has_next_page': false},
    );

void main() {
  // An unclaimed failure has to have somewhere to go; the app installs a logger
  // in main(), and these tests only need the slot filled.
  setUp(() => Command.globalExceptionHandler = (_, _) {});

  group('a section', () {
    test('sends the filters the endpoint can act on, and nothing else', () {
      expect(const BroadcastQuery.all().toQueryParameters(), {
        'order': 'desc',
        'limit': 20,
      });

      expect(const BroadcastQuery.nowLive().toQueryParameters(), {
        'status': 'active',
        'order': 'desc',
        'limit': 20,
      });

      // `inactive`, because that is what ending a broadcast writes: the API
      // sets `status = inactive` and `end_time`, and never writes `ended`.
      expect(const BroadcastQuery.recentlyLive().toQueryParameters(), {
        'status': 'inactive',
        'order': 'desc',
        'limit': 20,
      });
    });

    test('never puts its own type on the wire', () {
      for (final type in BroadcastsType.values) {
        expect(
          BroadcastQuery.section(type).toQueryParameters(),
          isNot(contains('type')),
        );
      }
    });

    test('maps a type to the filters it means', () {
      expect(
        BroadcastQuery.section(BroadcastsType.nowLive).status,
        BroadcastStatus.active,
      );
      expect(
        BroadcastQuery.section(BroadcastsType.forYou).type,
        BroadcastsType.forYou,
      );
      expect(
        BroadcastQuery.section(BroadcastsType.subscriptions).type,
        BroadcastsType.subscriptions,
      );
      expect(
        BroadcastQuery.section(BroadcastsType.recentlyLive).status,
        BroadcastStatus.inactive,
      );
    });

    test('names a creator the way the endpoint reads it', () {
      // `creatorId`, not `creator_id`: the query struct is `camelCase`, and an
      // unknown key is ignored rather than rejected, so a misnamed filter is a
      // filter that silently does nothing.
      expect(const BroadcastQuery.byCreator('creator-1').toQueryParameters(), {
        'creatorId': 'creator-1',
        'order': 'desc',
        'limit': 20,
      });
    });
  });

  group('the cursor', () {
    test('is left off the first page and carried on the next', () {
      final first = const BroadcastQuery.nowLive().withCursor('opaque');

      expect(
        const BroadcastQuery.nowLive().toQueryParameters(),
        isNot(contains('cursor')),
      );
      expect(const BroadcastQuery.nowLive().isFirstPage, isTrue);

      expect(
        const BroadcastQuery.nowLive().withCursor('opaque').cursor,
        'opaque',
      );

      // Everything else travels with it: a page is the same list, later.
      expect(first.type, BroadcastsType.nowLive);
      expect(first.status, BroadcastStatus.active);
      expect(first.toQueryParameters()['cursor'], 'opaque');
    });

    test('is cleared by withCursor, and cannot be by copyWith', () {
      final page = const BroadcastQuery.nowLive().withCursor('opaque');

      expect(page.withCursor(null).cursor, isNull);

      // copyWith cannot tell "not passed" from "passed as null", so a copy that
      // says nothing about the cursor leaves it where it was — which is why
      // asking for the first page is a method of its own.
      expect(page.copyWith().cursor, 'opaque');
    });
  });

  group('the router', () {
    test('round-trips a section', () {
      const query = BroadcastQuery.recentlyLive();

      expect(BroadcastQuery.fromRouter(query.toRouterParams()), query);
    });

    test('opens a first page even when the query had one open', () {
      final params = const BroadcastQuery.nowLive()
          .withCursor('opaque')
          .toRouterParams();

      expect(params, isNot(contains('cursor')));
      expect(BroadcastQuery.fromRouter(params).isFirstPage, isTrue);
    });

    test('leaves out what it cannot read', () {
      final query = BroadcastQuery.fromRouter({
        'type': 'nonsense',
        'status': 'nonsense',
        'order': 'sideways',
      });

      expect(query.type, isNull);
      expect(query.status, isNull);
      expect(query.order, BroadcastOrder.newest);
      expect(query.hasFilters, isFalse);
    });

    test('carries a creator, so a profile link opens that profile', () {
      final query = BroadcastQuery.fromRouter({'creatorId': 'creator-1'});

      expect(query.creatorId, 'creator-1');
      expect(query.hasFilters, isTrue);
    });
  });

  group('hasFilters', () {
    test('counts what narrows the rows, not what shapes them', () {
      expect(const BroadcastQuery.all().hasFilters, isFalse);
      expect(
        const BroadcastQuery.all().withCursor('opaque').hasFilters,
        isFalse,
      );
      expect(const BroadcastQuery.nowLive().hasFilters, isTrue);
      expect(const BroadcastQuery.byCreator('creator-1').hasFilters, isTrue);
    });
  });

  group('the feed', () {
    test('asks for the section it was built with', () async {
      final api = StubApi([_emptyPage()]);
      final source = BroadcastFeedDataSource(
        http: BroadcastHttpService(stubClient(api)),
        query: const BroadcastQuery.nowLive(),
      );
      addTearDown(source.onDispose);

      await source.updateDataCommand.runAsync();

      expect(api.requests.single.path, '/broadcasts');
      expect(api.requests.single.queryParameters, {
        'status': 'active',
        'order': 'desc',
        'limit': 20,
      });
    });

    test('hands the cursor back when it asks for the next page', () async {
      final api = StubApi([
        _emptyPage(nextCursor: 'opaque-cursor'),
        _emptyPage(),
      ]);
      final source = BroadcastFeedDataSource(
        http: BroadcastHttpService(stubClient(api)),
        query: const BroadcastQuery.recentlyLive(),
      );
      addTearDown(source.onDispose);

      await source.updateDataCommand.runAsync();
      await source.requestNextPageCommand.runAsync();

      expect(api.requests.first.queryParameters, isNot(contains('cursor')));
      expect(api.requests.last.queryParameters['cursor'], 'opaque-cursor');
    });

    test('keeps only what has ended in a recently-live list', () async {
      final api = StubApi([
        _page([
          _item('live-1', state: 'live'),
          _item('ended-1', state: 'ended'),
          _item('draft-1', state: 'draft'),
        ]),
      ]);
      final source = BroadcastFeedDataSource(
        http: BroadcastHttpService(stubClient(api)),
        query: const BroadcastQuery.recentlyLive(),
      );
      addTearDown(source.onDispose);

      await source.updateDataCommand.runAsync();

      // `status=inactive` is the closest the wire gets to "has ended" and it
      // carries drafts and live rows that are still shutting down with it.
      expect(source.items.map((item) => item.id), ['ended-1']);
    });

    test('keeps everything the endpoint sent in a live list', () async {
      final api = StubApi([
        _page([_item('live-1', state: 'live'), _item('live-2', state: 'live')]),
      ]);
      final source = BroadcastFeedDataSource(
        http: BroadcastHttpService(stubClient(api)),
        query: const BroadcastQuery.nowLive(),
      );
      addTearDown(source.onDispose);

      await source.updateDataCommand.runAsync();

      expect(source.items.map((item) => item.id), ['live-1', 'live-2']);
    });

    test('switching section re-reads from the first page', () async {
      final api = StubApi([_emptyPage(), _emptyPage()]);
      final source = BroadcastFeedDataSource(
        http: BroadcastHttpService(stubClient(api)),
        query: const BroadcastQuery.nowLive(),
      );
      addTearDown(source.onDispose);

      await source.updateDataCommand.runAsync();
      source.updateQuery(const BroadcastQuery.recentlyLive());
      await pumpUntil(() => api.requests.length == 2);

      expect(api.requests.length, 2);
      expect(api.requests.last.queryParameters, {
        'status': 'inactive',
        'order': 'desc',
        'limit': 20,
      });
    });
  });
}
