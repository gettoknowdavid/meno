import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';

import '../../_support/pump_until_found.dart';

/// A feed whose pages the test scripts, one queue per cursor.
///
/// [gate] holds the next read open so the races can be staged: a refresh that
/// starts while a page is in flight, or a query change that lands mid-read.
final class _TestSource extends PagedFeedDataSource<String> {
  new();

  /// Pages to answer with, per requested cursor; `null` is the first page.
  final pages = <String?, List<FeedPage<String>>>{};

  /// Every cursor the source was asked for, in order.
  final fetched = <String?>[];

  /// While set, the next read waits for it before answering.
  Completer<void>? gate;

  /// Thrown by the next read instead of a page.
  Exception? failNext;

  @override
  bool itemsAreEqual(String a, String b) => a == b;

  @override
  Future<FeedPage<String>> fetchPage(String? cursor) async {
    fetched.add(cursor);

    final pending = gate;
    if (pending != null) await pending.future;

    final error = failNext;
    if (error != null) {
      failNext = null;
      throw error;
    }

    final queue = pages[cursor];
    if (queue == null || queue.isEmpty) return FeedPage.empty<String>();
    return queue.removeAt(0);
  }

  int firstPageReads() => fetched.where((cursor) => cursor == null).length;
}

FeedPage<String> _page(List<String> items, String? nextCursor) =>
    FeedPage(items: items, nextCursor: nextCursor);

void main() {
  setUp(() => Command.globalExceptionHandler = (_, _) {});

  group('a query change', () {
    test('discards the page that was in flight (Step 60a)', () async {
      final source = _TestSource()
        ..pages[null] = [
          _page(['a'], 'c1'),
        ]
        ..pages['c1'] = [
          _page(['stale'], null),
        ];
      addTearDown(source.onDispose);

      await source.refresh();
      expect(source.items, ['a']);
      expect(source.hasNextPage, isTrue);

      // A page starts under the old query and is held open.
      source.gate = Completer<void>();
      unawaited(source.requestNextPageCommand.runAsync());
      await pumpUntil(() => source.fetched.length == 2);

      // The filters change while that read is in flight, and the feed asks for
      // its new first page — which, with the page still open, waits.
      source.invalidateQuery();
      source.pages[null] = [
        _page(['fresh'], null),
      ];
      unawaited(source.refresh());
      source.gate!.complete();
      source.gate = null;

      await pumpUntil(
        () => source.items.contains('fresh'),
        step: const Duration(milliseconds: 5),
      );

      // The held page resolved under a generation that no longer exists: no old
      // rows, and the cursor belongs to the new query's first page.
      expect(source.items, ['fresh']);
      expect(source.hasNextPage, isFalse);
    });

    test(
      'drops the cursor, so the new query starts at its first page',
      () async {
        final source = _TestSource()
          ..pages[null] = [
            _page(['a'], 'c1'),
          ];
        addTearDown(source.onDispose);

        await source.refresh();
        expect(source.hasNextPage, isTrue);

        source.invalidateQuery();

        expect(source.cursor, isNull);
        expect(source.hasNextPage, isFalse);
        expect(source.items, isEmpty);
      },
    );
  });

  group('a refresh', () {
    test('waits for the page in flight and runs once (Step 60b)', () async {
      final source = _TestSource()
        ..pages[null] = [
          _page(['a'], 'c1'),
          _page(['a', 'b'], null),
        ]
        ..pages['c1'] = [
          _page(['b'], null),
        ];
      addTearDown(source.onDispose);

      await source.refresh();

      source.gate = Completer<void>();
      unawaited(source.requestNextPageCommand.runAsync());
      await pumpUntil(() => source.fetched.length == 2);

      // Both pull-to-refreshes arrive while the page is still loading.
      await source.refresh();
      await source.refresh();
      expect(
        source.firstPageReads(),
        1,
        reason: 'the in-flight read owns the slot',
      );

      source.gate!.complete();
      source.gate = null;

      await pumpUntil(() => source.firstPageReads() == 2);
      await pumpUntil(() => source.items.length == 2);

      expect(
        source.firstPageReads(),
        2,
        reason: 'one trailing refresh, not two',
      );
      expect(source.items, ['a', 'b']);
    });

    test('collapses a burst of invalidations into one (Step 60c)', () async {
      final source = _TestSource()
        ..pages[null] = [
          _page(['a'], null),
          _page(['b'], null),
        ];
      addTearDown(source.onDispose);

      source.gate = Completer<void>();
      unawaited(source.refresh());
      await pumpUntil(() => source.fetched.length == 1);

      // Three frames arrive during the one read.
      await source.refresh();
      await source.refresh();
      await source.refresh();

      source.gate!.complete();
      source.gate = null;

      await pumpUntil(() => source.items.contains('b'));
      await Future<void>.delayed(const Duration(milliseconds: 20));

      expect(source.firstPageReads(), 2, reason: 'one read, one trailing read');
      expect(source.items, ['b']);
    });

    test('replaces the items instead of appending to them', () async {
      final source = _TestSource()
        ..pages[null] = [
          _page(['a', 'b'], null),
          _page(['c'], null),
        ];
      addTearDown(source.onDispose);

      await source.refresh();
      await source.refresh();

      expect(source.items, ['c']);
    });
  });

  group('a next page', () {
    test('is appended without duplicates (Step 60d)', () async {
      final source = _TestSource()
        ..pages[null] = [
          _page(['a', 'b'], 'c1'),
        ]
        ..pages['c1'] = [
          _page(['b', 'c'], null),
        ];
      addTearDown(source.onDispose);

      await source.refresh();
      await source.requestNextPageCommand.runAsync();

      expect(source.items, ['a', 'b', 'c']);
    });

    test(
      'records a failure on its own surface rather than the refresh one',
      () async {
        final source = _TestSource()
          ..pages[null] = [
            _page(['a'], 'c1'),
          ];
        addTearDown(source.onDispose);

        await source.refresh();
        source.failNext = const NetworkException();
        await source.requestNextPageCommand.runAsync();

        expect(source.pageError.value, isNotNull);
        expect(source.pageError.value!.isRetryable, isTrue);
        expect(source.commandErrors.value?.error, isNull);

        // And the failure gate stays shut: a list that keeps scrolling must not
        // retry it on its own.
        source.fetched.clear();
        source.getItemAtIndex(source.items.length - 1);
        await Future<void>.delayed(Duration.zero);
        expect(source.fetched, isEmpty);
      },
    );

    test('a refusal is not offered as retryable', () async {
      final source = _TestSource()
        ..pages[null] = [
          _page(['a'], 'c1'),
        ];
      addTearDown(source.onDispose);

      await source.refresh();
      source.failNext = MenoException.fromResponse(
        statusCode: 403,
        code: 'FORBIDDEN',
        message: 'not yours',
      );
      await source.requestNextPageCommand.runAsync();

      expect(source.pageError.value!.isRetryable, isFalse);
    });

    test('retry clears the failure and reads the page', () async {
      final source = _TestSource()
        ..pages[null] = [
          _page(['a'], 'c1'),
        ]
        ..pages['c1'] = [
          _page(['b'], null),
        ];
      addTearDown(source.onDispose);

      await source.refresh();
      source.failNext = const NetworkException();
      await source.requestNextPageCommand.runAsync();
      expect(source.pageError.value, isNotNull);

      await source.retryPage();

      expect(source.pageError.value, isNull);
      expect(source.items, ['a', 'b']);
    });
  });

  group('the cursor contract (Step 64)', () {
    test('a cursor is what says another page exists', () async {
      final source = _TestSource()
        ..pages[null] = [
          _page(['a'], 'c1'),
        ]
        ..pages['c1'] = [
          _page(['b'], null),
        ];
      addTearDown(source.onDispose);

      await source.refresh();
      expect(source.hasNextPage, isTrue);

      await source.requestNextPageCommand.runAsync();
      expect(
        source.hasNextPage,
        isFalse,
        reason: 'no cursor came back, so there is no next page',
      );
    });

    test('reading three from the end asks for the next page', () async {
      final source = _TestSource()
        ..pages[null] = [
          _page([for (var i = 0; i < 10; i++) 'live-$i'], 'c1'),
        ]
        ..pages['c1'] = [
          _page(['more'], null),
        ];
      addTearDown(source.onDispose);

      await source.refresh();

      // Ten items, so index 7 is the third from the end — the prefetch trigger.
      source.getItemAtIndex(3);
      await Future<void>.delayed(Duration.zero);
      expect(source.fetched, [null], reason: 'reading early does not page');

      source.getItemAtIndex(7);
      await pumpUntil(() => source.fetched.length == 2);
      expect(source.fetched.last, 'c1');
    });
  });
}
