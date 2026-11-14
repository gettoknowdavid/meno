import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_ui/meno_ui.dart';

import '../../_support/pump_until_found.dart';

/// A feed whose pages the test decides.
///
/// [gate] holds the first page open so the initial loading state can be looked
/// at; [failure] makes that page fail; [pageFailure] makes the *next* page
/// fail; [pageCount] says how many pages the endpoint has, and every read is
/// counted so "exactly once" can be asserted.
final class _TestFeedSource extends PagedFeedDataSource<String> {
  new({this.pageSize = 2, this.pageCount = 1, super.fetchMoreEnabled = true});

  final int pageSize;
  final int pageCount;

  int updateCalls = 0;
  int pageRequests = 0;
  Exception? failure;
  Exception? pageFailure;
  Completer<void>? gate;

  @override
  bool itemsAreEqual(String a, String b) => a == b;

  @override
  Future<FeedPage<String>> fetchPage(String? cursor) async {
    final pending = gate;
    if (pending != null) await pending.future;

    if (cursor == null) {
      updateCalls++;

      final thrown = failure;
      if (thrown != null) throw thrown;

      return FeedPage(
        items: [for (var i = 0; i < pageSize; i++) 'live-$i'],
        nextCursor: pageCount > 1 ? 'p1' : null,
      );
    }

    pageRequests++;

    final thrown = pageFailure;
    if (thrown != null) {
      pageFailure = null;
      throw thrown;
    }

    // `p<n>` is the page after the nth: a cursor from the server, opaque to the
    // feed, which is the whole point of the contract.
    final page = int.parse(cursor.substring(1));
    return FeedPage(
      items: [for (var i = 0; i < pageSize; i++) 'more-$page-$i'],
      nextCursor: page < pageCount - 1 ? 'p${page + 1}' : null,
    );
  }
}

/// The skeleton the harness draws in place of an item, so the initial loading
/// state can be told apart from the loaded one by key.
const _skeletonKey = Key('feed-skeleton');

/// Flushes the zero-duration notifications command_it schedules for its
/// listeners, then takes the tree down so those listeners go with it.
///
/// Without the pump the binding reports a pending timer for the last command a
/// test ran, which is the feed asking for its next page.
Future<void> _finish(WidgetTester tester) async {
  await tester.pump(const Duration(milliseconds: 20));
  await unmountTree(tester);
}

/// Fails the next page read and lets it reach the footer.
///
/// The command is not awaited: it starts with a zero-delay timer of its own,
/// which only advances while the test pumps, so awaiting it here would wait on
/// a clock that is not moving.
Future<void> _failNextPage(WidgetTester tester, _TestFeedSource source) async {
  source.pageFailure = const NetworkException();
  unawaited(source.requestNextPageCommand.runAsync());
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 20));
  await tester.pump();
}

Widget _harness(
  PagedFeedDataSource<String> source, {
  FeedLayout layout = FeedLayout.verticalList,
}) {
  return MaterialApp(
    // The kit's widgets read their colours from these extensions, so the tests
    // have to mount the real theme rather than Material's defaults.
    theme: MTheme.light,
    home: Scaffold(
      body: FeedWidget<String>(
        feedSource: source,
        layout: layout,
        skeletonItem: const SizedBox(key: _skeletonKey),
        itemBuilder: (context, item) => Text(item),
      ),
    ),
  );
}

void main() {
  // command_it asserts that a failure no widget claimed has somewhere to go.
  // The app installs a logger in main(); the tests only need the slot filled.
  setUp(() => Command.globalExceptionHandler = (_, _) {});

  testWidgets('draws skeletons while the first page is in flight', (
    tester,
  ) async {
    final source = _TestFeedSource()..gate = Completer<void>();
    addTearDown(source.onDispose);

    await tester.pumpWidget(_harness(source));
    await pumpUntilFound(tester, find.byKey(_skeletonKey));

    // How many skeletons a sliver builds is the viewport's business; what the
    // initial load has to do is draw the skeleton instead of items or an empty
    // state.
    expect(find.byKey(_skeletonKey), findsAtLeastNWidgets(2));
    expect(find.text('live-0'), findsNothing);

    source.gate!.complete();
    await pumpUntilFound(tester, find.text('live-0'));

    expect(find.byKey(_skeletonKey), findsNothing);
    expect(find.text('live-1'), findsOneWidget);

    await _finish(tester);
  });

  testWidgets('asks for the first page once, not on every rebuild', (
    tester,
  ) async {
    final source = _TestFeedSource();
    addTearDown(source.onDispose);

    await tester.pumpWidget(_harness(source));
    await pumpUntilFound(tester, find.text('live-0'));

    // A rebuild of the same widget must not re-request page one.
    await tester.pumpWidget(_harness(source));
    await tester.pump();

    expect(source.updateCalls, 1);
    expect(find.text('live-0'), findsOneWidget);

    await _finish(tester);
  });

  testWidgets('an empty first page draws the empty state', (tester) async {
    final source = _TestFeedSource(pageSize: 0);
    addTearDown(source.onDispose);

    await tester.pumpWidget(_harness(source));
    await pumpUntilFound(tester, find.byType(MEmptyState));

    expect(find.byType(MEmptyState), findsOneWidget);
    expect(find.byKey(_skeletonKey), findsNothing);

    await _finish(tester);
  });

  testWidgets('a failed first page draws the error state, and retry reloads', (
    tester,
  ) async {
    final source = _TestFeedSource()..failure = const NetworkException();
    addTearDown(source.onDispose);

    await tester.pumpWidget(_harness(source));
    await pumpUntilFound(tester, find.byType(MErrorState));

    expect(
      tester.widget<MErrorState>(find.byType(MErrorState)).message,
      errorMessage(const NetworkException()),
    );

    // The retry is the same request, so clearing the failure is all it takes.
    source.failure = null;
    await tester.tap(find.text('Retry'));
    await pumpUntilFound(tester, find.text('live-0'));

    expect(source.updateCalls, 2);
    expect(find.byType(MErrorState), findsNothing);

    await unmountTree(tester);
  });

  testWidgets('scrolling to the end of a page asks for the next one', (
    tester,
  ) async {
    final source = _TestFeedSource(pageCount: 2);
    addTearDown(source.onDispose);

    await tester.pumpWidget(_harness(source));
    await pumpUntilFound(tester, find.text('more-1-0'));

    // Reading an item near the end is what triggers paging.
    expect(source.pageRequests, greaterThan(0));
    expect(find.text('more-1-1'), findsOneWidget);

    await _finish(tester);
  });

  testWidgets('the footer ends the list when the last page has loaded', (
    tester,
  ) async {
    final source = _TestFeedSource();
    addTearDown(source.onDispose);

    await tester.pumpWidget(_harness(source));
    await pumpUntilFound(tester, find.text('live-0'));

    expect(find.text('You have reached the end'), findsOneWidget);

    await _finish(tester);
  });

  testWidgets('a horizontal feed draws its items in a row', (tester) async {
    final source = _TestFeedSource();
    addTearDown(source.onDispose);

    await tester.pumpWidget(
      _harness(source, layout: FeedLayout.horizontalList),
    );
    await pumpUntilFound(tester, find.text('live-0'));

    expect(find.byType(ListView), findsOneWidget);
    expect(find.text('live-1'), findsOneWidget);

    await _finish(tester);
  });

  group('a failed page (Step 62)', () {
    testWidgets('draws a footer with its own retry', (tester) async {
      final source = _TestFeedSource(pageCount: 2, fetchMoreEnabled: false);
      addTearDown(source.onDispose);

      await tester.pumpWidget(_harness(source));
      await pumpUntilFound(tester, find.text('live-0'));

      await _failNextPage(tester, source);

      // The list is still on screen — a page failure is not a refresh failure.
      expect(find.text('live-0'), findsOneWidget);
      expect(find.byType(MErrorState), findsNothing);
      expect(find.text(errorMessage(const NetworkException())), findsOneWidget);
      expect(find.text('Retry'), findsOneWidget);

      await _finish(tester);
    });

    testWidgets('does not re-trigger on its own', (tester) async {
      final source = _TestFeedSource(pageCount: 2, fetchMoreEnabled: false);
      addTearDown(source.onDispose);

      await tester.pumpWidget(_harness(source));
      await pumpUntilFound(tester, find.text('live-0'));

      await _failNextPage(tester, source);

      final attempts = source.pageRequests;

      // Scrolling the list, and rebuilding it, must not ask again: only the
      // retry button re-enables paging.
      await tester.drag(find.text('live-0'), const Offset(0, -400));
      await tester.pump();
      await tester.pumpWidget(_harness(source));
      await tester.pump();

      expect(source.pageRequests, attempts);

      await _finish(tester);
    });

    testWidgets('retry clears the footer and reads the page', (tester) async {
      final source = _TestFeedSource(pageCount: 2);
      addTearDown(source.onDispose);

      await tester.pumpWidget(_harness(source));
      await pumpUntilFound(tester, find.text('live-0'));

      await _failNextPage(tester, source);
      expect(find.text('Retry'), findsOneWidget);

      await tester.tap(find.text('Retry'));
      await pumpUntilFound(tester, find.text('more-1-0'));

      expect(find.text('Retry'), findsNothing);
      expect(find.text(errorMessage(const NetworkException())), findsNothing);

      await _finish(tester);
    });

    testWidgets('a refresh failure stays on its own surface', (tester) async {
      final source = _TestFeedSource(pageCount: 2, fetchMoreEnabled: false);
      addTearDown(source.onDispose);

      await tester.pumpWidget(_harness(source));
      await pumpUntilFound(tester, find.text('live-0'));

      // A page fails, then a refresh fails too. The two are different facts —
      // "this page could not be read" and "the list could not be re-read" — so
      // each keeps its own channel rather than overwriting the other.
      await _failNextPage(tester, source);

      // The failure is claimed here because the widget would otherwise see it
      // as an unhandled one; what the test reads is the command's own channel.
      source.failure = TimeoutException('slow');
      unawaited(source.refresh().catchError((Object _) {}));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 20));
      await tester.pump();

      expect(source.commandErrors.value, isNotNull);
      expect(
        source.pageError.value?.message,
        errorMessage(const NetworkException()),
        reason: 'the page failure is not replaced by the refresh one',
      );
      // The list itself is untouched: a failed refresh does not throw away what
      // the user is reading.
      expect(find.text('live-0'), findsOneWidget);
      expect(find.byType(MErrorState), findsNothing);

      await unmountTree(tester);
    });
  });
}
