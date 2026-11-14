import 'dart:io';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/router.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/features/broadcast/model/broadcast_query.dart';

/// A navigation by name, as written in the app's source.
final _namedNavigation = RegExp(
  r'(?:pushNamed|goNamed|pushReplacementNamed)\(\s*MRoutes\.(\w+)',
  multiLine: true,
);

/// A route declaring the name it can be navigated by.
final _declaredName = RegExp(r'\bname:\s*MRoutes\.(\w+)', multiLine: true);

/// The identifiers [pattern] captures in [file], ignoring everything else.
Set<String> _identifiersIn(File file, RegExp pattern) => {
  for (final match in pattern.allMatches(file.readAsStringSync()))
    match.group(1)!,
};

void main() {
  // The home page builds managers from the locator; nothing here touches it,
  // but a stray command failure should still have somewhere to go.
  setUp(() => Command.globalExceptionHandler = (_, _) {});
  tearDown(() => Command.globalExceptionHandler = null);

  group('a named navigation', () {
    test('names a route the router declares', () {
      // `M20`: home pushed `MRoutes.broadcasts` by name while the route
      // declared only a path, so "See all" failed at runtime — the kind of
      // break that compiles and ships. Reading the declarations out of the
      // route table means this test cannot drift from it.
      // The runtime authority: what the assembled router actually declares.
      expect(declaredRouteNames(), contains(MRoutes.broadcasts));

      // And every name the app navigates by is one the route table declares.
      // Compared as written — `MRoutes.x` against `MRoutes.x` — because a name
      // is a value, and the identifier is what both sides of the call site
      // share.
      final table = File('lib/_routing/router.dart');
      final declared = _identifiersIn(table, _declaredName);

      final requested = <String>{};
      final sources = Directory('lib')
          .listSync(recursive: true)
          .whereType<File>()
          .where((file) => file.path.endsWith('.dart'));
      for (final file in sources) {
        requested.addAll(_identifiersIn(file, _namedNavigation));
      }

      expect(declared, isNotEmpty, reason: 'no route declares a name');
      expect(requested, isNotEmpty, reason: 'no navigation uses one');
      expect(
        requested.difference(declared),
        isEmpty,
        reason: 'every pushNamed target has a name: on its route',
      );
    });

    testWidgets('opens the broadcasts list with the filter it carries', (
      tester,
    ) async {
      // The destination publishes where it landed, so the query parameters the
      // home sections send can be read back.
      final router = GoRouter(
        initialLocation: MRoutes.home,
        routes: [
          GoRoute(
            path: MRoutes.home,
            builder: (context, _) => Scaffold(
              body: TextButton(
                // What `_RecentlyLiveSection.onSeeAll` does.
                onPressed: () => context.pushNamed(
                  MRoutes.broadcasts,
                  queryParameters: const BroadcastQuery.recentlyLive()
                      .toRouterParams(),
                ),
                child: const Text('See all'),
              ),
            ),
          ),
          GoRoute(
            name: MRoutes.broadcasts,
            path: MRoutes.broadcasts,
            builder: (_, state) =>
                Scaffold(body: Text('landed:${state.uri.queryParameters}')),
          ),
        ],
      );

      await tester.pumpWidget(MaterialApp.router(routerConfig: router));
      await tester.pumpAndSettle();

      await tester.tap(find.text('See all'));
      await tester.pumpAndSettle();

      expect(router.state.uri.path, MRoutes.broadcasts);
      expect(
        router.state.uri.queryParameters,
        const BroadcastQuery.recentlyLive().toRouterParams(),
      );
      expect(
        router.state.uri.queryParameters,
        containsPair('status', 'inactive'),
      );
      expect(find.textContaining('landed:'), findsOneWidget);
    });
  });
}
