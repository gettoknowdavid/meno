import 'package:dio/dio.dart' show ResponseBody;
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

const _creator = 'c0ffee00-1111-4222-8333-444455556666';

/// One row of a `GET /broadcasts` page.
///
/// `status=inactive` carries drafts and scheduled broadcasts as well as ended
/// ones, which is why the feed still has to filter: this fixture set is the
/// only way to prove a page full of the wrong kind does not end the list.
Map<String, dynamic> _item(String id, {required String state}) => {
  'id': id,
  'title': id,
  'state': state,
  'status': state == 'live' ? 'active' : 'inactive',
  'createdAt': '2026-01-01T00:00:00Z',
  'totalParticipants': 0,
  'creatorId': _creator,
  'creatorName': 'Ada',
};

Future<ResponseBody> Function() _page(
  List<Map<String, dynamic>> items, {
  String? nextCursor,
}) =>
    () async => envelope(
      data: {
        'data': items,
        'next_cursor': nextCursor,
        'has_next_page': nextCursor != null,
      },
    );

String _id(int n) => '018f2b1e-0000-7000-8000-${n.toString().padLeft(12, '0')}';

/// A feed whose pages never match, so the footer's own behaviour can be read.
final class _NothingMatchesSource extends PagedFeedDataSource<String> {
  new({this.hasMore = true});

  /// Whether the endpoint claims another page after every one it answers.
  final bool hasMore;

  /// How many pages were read.
  int reads = 0;

  @override
  bool itemsAreEqual(String a, String b) => a == b;

  @override
  Future<FeedPage<String>> fetchPage(String? cursor) async {
    reads += 1;
    return FeedPage<String>(
      items: const [],
      nextCursor: hasMore ? 'page-$reads' : null,
    );
  }
}

Widget _harness(PagedFeedDataSource<String> source) => MaterialApp(
  theme: MTheme.light,
  home: Scaffold(
    body: FeedWidget<String>(
      feedSource: source,
      itemBuilder: (context, item) => Text(item),
    ),
  ),
);

void main() {
  setUp(() => Command.globalExceptionHandler = (_, _) {});

  group('a page the filter rejects (Step 61)', () {
    late BroadcastFeedDataSource feed;

    Future<void> start(StubApi api) async {
      feed = BroadcastFeedDataSource(
        http: BroadcastHttpService(stubClient(api)),
        query: const BroadcastQuery.recentlyLive(),
      );
      await feed.refresh();
    }

    tearDown(() => feed.onDispose());

    test(
      'does not end the list: the older ended row is still reachable',
      () async {
        final api = StubApi([
          _page([_item(_id(1), state: 'live')], nextCursor: 'c1'),
          _page([_item(_id(2), state: 'draft')], nextCursor: 'c2'),
          _page([_item(_id(3), state: 'ended')]),
        ]);

        await start(api);

        expect(api.requests, hasLength(3));
        expect(feed.items.single.id.getOrCrash, _id(3));
        expect(feed.hasMoreFiltered, isFalse);
      },
    );

    test('stops after the page budget and says there is more', () async {
      // Six pages, none of which survives the filter.
      final api = StubApi([
        for (var page = 0; page < 6; page++)
          _page([_item(_id(10 + page), state: 'live')], nextCursor: 'c$page'),
      ]);

      await start(api);

      // The first page plus the bounded walk: `maxFilteredPages` in all.
      expect(api.requests, hasLength(1 + PagedFeedDataSource.maxFilteredPages));
      expect(feed.items, isEmpty);
      expect(feed.hasMoreFiltered, isTrue);
    });

    test(
      'a cursor that never moves stops the walk instead of spinning',
      () async {
        final api = StubApi([
          _page([_item(_id(1), state: 'live')], nextCursor: 'same'),
          _page([_item(_id(2), state: 'live')], nextCursor: 'same'),
        ]);

        await start(api);

        expect(
          api.requests,
          hasLength(2),
          reason: 'one page, then the fresh answer read once more',
        );
        expect(feed.hasMoreFiltered, isTrue);
      },
    );
  });

  group('an empty filtered list (Step 61)', () {
    testWidgets('offers "Load more" instead of the empty state', (
      tester,
    ) async {
      final source = _NothingMatchesSource();
      addTearDown(source.onDispose);

      await tester.pumpWidget(_harness(source));
      await pumpUntilFound(tester, find.text('Load more'));

      expect(find.text('Load more'), findsOneWidget);
      expect(find.byType(MEmptyState), findsNothing);

      // Asking is a user action, and it reads again. The read itself starts on
      // a zero-delay timer, so it takes pumps rather than a wait.
      final before = source.reads;
      await tester.tap(find.text('Load more'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 20));
      await tester.pump();

      expect(source.reads, greaterThan(before));

      await tester.pump(const Duration(milliseconds: 20));
      await unmountTree(tester);
    });

    testWidgets('a list with nothing left to read says it is empty', (
      tester,
    ) async {
      final source = _NothingMatchesSource(hasMore: false);
      addTearDown(source.onDispose);

      await tester.pumpWidget(_harness(source));
      await pumpUntilFound(tester, find.byType(MEmptyState));

      // Nothing further to read: the honest state is "empty", not "more".
      expect(find.byType(MEmptyState), findsOneWidget);
      expect(find.text('Load more'), findsNothing);

      await tester.pump(const Duration(milliseconds: 20));
      await unmountTree(tester);
    });
  });
}
