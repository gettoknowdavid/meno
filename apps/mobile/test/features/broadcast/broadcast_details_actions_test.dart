import 'dart:async';

import 'package:dio/dio.dart' show ResponseBody;
// The platform-channel import returns with the Copy Link test below
// (docs/launch-readiness.md §2.2).
// import 'package:flutter/services.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/broadcast/pages/broadcast_details_page.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/objectbox.g.dart' hide Id;
import 'package:meno_ui/meno_ui.dart';

import '../../_support/stub_api.dart';

const _viewer = '018f2b1e-0000-7000-8000-000000000001';
const _creator = '018f2b1e-0000-7000-8000-000000000002';
const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';

Broadcast _broadcast({String creatorId = _creator}) => Broadcast(
  id: Id.fromString(_broadcastId),
  title: SingleLineString('Morning show'),
  creator: Participant(
    id: Id.fromString(creatorId),
    fullName: SingleLineString('Ada Lovelace'),
  ),
);

void main() {
  setUp(() => Command.globalExceptionHandler = (_, _) {});

  late Store store;
  late Database database;
  late BroadcastLocalService local;
  late StubApi api;

  setUp(() async {
    await di.reset();
    di.pushNewScope(scopeName: Scope.root);
    di.registerSingleton<MenoLogger>(MockMenoLogger());

    store = Store(
      getObjectBoxModel(),
      directory:
          'memory:meno-details-actions-'
          '${DateTime.now().microsecondsSinceEpoch}',
    );
    database = Database.fromStore(store);
    local = BroadcastLocalService(
      Id.fromString(_viewer),
      database,
      logger: di<MenoLogger>(),
    );
    di.registerSingleton<BroadcastLocalService>(local);

    // No `FavouritesManager` here: the details sheet no longer offers "Listen
    // Later" (recording is a later build), and the list it kept is covered by
    // `favourites_manager_test`. Registering one would also fetch the bookmarks
    // page on construction, which is a request these assertions do not expect.
    api = StubApi();
    di.registerSingleton<BroadcastHttpService>(
      BroadcastHttpService(stubClient(api)),
    );
    di.registerSingleton<ProfileHttpService>(
      ProfileHttpService(stubClient(api)),
    );
    di.registerSingleton<UserProfileRepository>(UserProfileRepository());
  });

  tearDown(() async {
    Command.globalExceptionHandler = null;
    await database.onDispose();
    await di.reset();
  });

  /// Pumps the details view over a router, because the sheet navigates.
  Future<BroadcastProxy> pumpDetails(
    WidgetTester tester, {
    required Broadcast broadcast,
  }) async {
    final proxy = BroadcastProxy(broadcast, Id.fromString(_viewer));
    final router = GoRouter(
      initialLocation: '/broadcasts/${broadcast.id.getOrCrash}',
      routes: [
        GoRoute(
          path: '/broadcasts/:broadcastId',
          builder: (_, _) => BroadcastDetailsView(proxy: proxy),
        ),
        GoRoute(
          path: '/users/:userId',
          builder: (_, state) =>
              Scaffold(body: Text('profile ${state.pathParameters['userId']}')),
        ),
      ],
    );

    await tester.pumpWidget(
      MaterialApp.router(theme: MTheme.dark, routerConfig: router),
    );
    await tester.pumpAndSettle();
    return proxy;
  }

  Future<void> openSheet(WidgetTester tester) async {
    await tester.tap(find.byIcon(MIcons.dots_horizontal));
    await tester.pumpAndSettle();
  }

  test('the details proxy toggles a local favourite off again', () async {
    final proxy = BroadcastProxy(_broadcast(), Id.fromString(_viewer));

    await proxy.toggleIsFavourite.runAsync();
    expect(proxy.isFavourited, isTrue);
    expect(local.isFavourited(Id.fromString(_broadcastId)), isTrue);

    await proxy.toggleIsFavourite.runAsync();
    expect(proxy.isFavourited, isFalse);
    expect(local.isFavourited(Id.fromString(_broadcastId)), isFalse);
  });

  // The saving is the account's, not the phone's: the star has to write the
  // bookmark to the API, and the device copy is what keeps the next launch
  // honest until the bookmarks page answers.
  test('the star writes the bookmark to the API and the device', () async {
    final proxy = BroadcastProxy(_broadcast(), Id.fromString(_viewer));
    final id = Id.fromString(_broadcastId);

    await proxy.toggleIsFavourite.runAsync();

    expect(local.isFavourited(id), isTrue);
    await proxy.toggleIsFavourite.runAsync();
    expect(local.isFavourited(id), isFalse);

    final writes = api.requests
        .where((request) => request.path == '/broadcasts/$_broadcastId/bookmark')
        .toList();
    expect(writes, hasLength(2), reason: 'one request per toggle');
    expect(writes[0].method, 'POST');
    expect(writes[1].method, 'DELETE');
  });

  test('the star flips before the API answers and flips back when it refuses',
      () async {
    Command.globalExceptionHandler = (_, _) {};
    final id = Id.fromString(_broadcastId);

    // The write stays unanswered until the test answers it, so what the star
    // shows in the meantime is what the person sees while the request is out.
    final answer = Completer<ResponseBody>();
    api.answers.add(() => answer.future);

    final proxy = BroadcastProxy(_broadcast(), Id.fromString(_viewer));
    expect(proxy.isFavourited, isFalse);

    final pending = proxy.toggleIsFavourite.runAsync();
    await pumpEventQueue();

    expect(
      proxy.isFavourited,
      isTrue,
      reason: 'optimistic: the star fills without waiting for the write',
    );
    expect(
      local.isFavourited(id),
      isFalse,
      reason: 'the device is written only once the API has agreed',
    );

    answer.complete(envelope(status: 500));
    try {
      await pending;
    } on Object {
      // The refusal is reported through the command; the flag is the subject.
    }

    expect(proxy.isFavourited, isFalse, reason: 'the failed write is undone');
    expect(local.isFavourited(id), isFalse);
  });

  testWidgets('the sheet opens the creator profile instead of a placeholder', (
    tester,
  ) async {
    await pumpDetails(tester, broadcast: _broadcast());
    await openSheet(tester);

    expect(find.text('Go to Profile'), findsOne);
    // Copy Link is commented out until a share link resolves
    // (docs/launch-readiness.md §2.2).
    // expect(find.text('Copy Link'), findsOne);

    await tester.tap(find.text('Go to Profile'));
    await tester.pumpAndSettle();

    expect(find.text('profile $_creator'), findsOne);
  });

  testWidgets('Unsubscribe is absent on your own broadcast, and calls DELETE '
      'for somebody else', (tester) async {
    await pumpDetails(tester, broadcast: _broadcast(creatorId: _viewer));
    await openSheet(tester);
    expect(find.text('Unsubscribe'), findsNothing);

    await tester.tap(find.text('Go to Profile'));
    await tester.pumpAndSettle();

    await pumpDetails(tester, broadcast: _broadcast());
    await openSheet(tester);
    expect(find.text('Unsubscribe'), findsOne);

    await tester.tap(find.text('Unsubscribe'));
    await tester.pumpAndSettle();

    expect(api.requests, hasLength(1));
    expect(api.requests.single.method, 'DELETE');
    expect(api.requests.single.path, '/subscribers/$_creator');
  });

  testWidgets('the sheet omits Listen Later while recording is off', (
    tester,
  ) async {
    await pumpDetails(tester, broadcast: _broadcast());
    await openSheet(tester);

    expect(find.text('Listen Later'), findsNothing);
    expect(find.text('Remove from Listen Later'), findsNothing);
    // expect(find.text('Copy Link'), findsOne);
  });

  // Copy Link is commented out until a share link resolves
  // (docs/launch-readiness.md §2.2). Restore with the sheet row.
  //
  // testWidgets('Copy Link puts the broadcast deep link on the clipboard', (
  //   tester,
  // ) async {
  //   String? copied;
  //   final messenger =
  //       TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  //   messenger.setMockMethodCallHandler(SystemChannels.platform,
  //       (call) async {
  //     if (call.method == 'Clipboard.setData') {
  //       copied = (call.arguments as Map)['text'] as String?;
  //     }
  //     return null;
  //   });
  //   addTearDown(
  //     () => messenger.setMockMethodCallHandler(SystemChannels.platform,
  //       null),
  //   );
  //
  //   await pumpDetails(tester, broadcast: _broadcast());
  //   await openSheet(tester);
  //
  //   await tester.tap(find.text('Copy Link'));
  //   await tester.pumpAndSettle();
  //
  //   expect(copied, 'https://meno.app/broadcasts/$_broadcastId');
  // });
}
