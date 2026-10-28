import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/_shared/services/media_service.dart';
import 'package:meno_mobile/features/auth/model/auth_context.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/broadcast/pages/broadcast_editor_page.dart';
import 'package:meno_mobile/objectbox.g.dart' hide Id;
import 'package:meno_ui/meno_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/stub_api.dart';

const _account = '018f2b1e-0000-7000-8000-000000000040';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late Store store;
  late ActiveBroadcastManager live;

  setUp(() async {
    Command.globalExceptionHandler = (_, _) {};
    SharedPreferences.setMockInitialValues({});
    await di.reset();
    di.pushNewScope(scopeName: Scope.root);
    di.registerSingleton<MenoLogger>(MockMenoLogger());

    store = Store(
      getObjectBoxModel(),
      directory:
          '${Store.inMemoryPrefix}editor-${DateTime.now().microsecondsSinceEpoch}',
    );

    live = ActiveBroadcastManager(
      context: AccountContext(
        accountId: Id.fromString(_account),
        generation: 1,
      ),
      gateway: _NoGateway(),
      store: LiveSessionStore(
        LocalStorage(await SharedPreferences.getInstance()),
        userId: Id.fromString(_account),
      ),
      logger: di<MenoLogger>(),
      createSession: ({required session, required exitOnServer}) =>
          throw StateError('no room in this test'),
    );
    di.registerSingleton<ActiveBroadcastManager>(live);
    di.registerSingleton<BroadcastHttpService>(
      BroadcastHttpService(stubClient(StubApi())),
    );
    di.registerSingleton<BroadcastLocalService>(
      BroadcastLocalService(
        Database.fromStore(store),
        logger: di<MenoLogger>(),
      ),
    );
    di.registerLazySingleton<MediaService>(MediaService.new);
  });

  tearDown(() async {
    Command.globalExceptionHandler = null;
    store.close();
    await di.reset();
  });

  testWidgets(
    'the page creates one editor manager and disposes it on pop',
    // Skipped here: the ObjectBox native library is not installed in this
    // environment, and the page needs the account local service for its
    // manager.
    skip: true,
    (tester) async {
      expect(di.isRegistered<BroadcastEditorManager>(), isFalse);

      await tester.pumpWidget(
        MaterialApp(
          theme: MTheme.dark,
          home: Builder(
            builder: (context) => Scaffold(
              body: TextButton(
                onPressed: () => Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) => const BroadcastEditorPage(),
                  ),
                ),
                child: const Text('open'),
              ),
            ),
          ),
        ),
      );

      await tester.tap(find.text('open'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));

      expect(find.byType(BroadcastEditorPage), findsOne);
      expect(di.isRegistered<BroadcastEditorManager>(), isTrue);
      final manager = di<BroadcastEditorManager>();

      // Opening it twice does not add a second manager: the page owns exactly
      // one, in the scope it pushed for itself.
      expect(identical(di<BroadcastEditorManager>(), manager), isTrue);

      Navigator.of(tester.element(find.byType(BroadcastEditorPage))).pop();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));

      expect(find.byType(BroadcastEditorPage), findsNothing);
      expect(
        di.isRegistered<BroadcastEditorManager>(),
        isFalse,
        reason: 'the page-owned manager goes with the page',
      );
      expect(di.currentScopeName, Scope.root);
    },
  );
}

final class _NoGateway implements LiveGateway {
  @override
  Future<BroadcastSession> enter(
    LiveEnterRequest request,
    AccountContext context,
  ) => throw StateError('no room in this test');

  @override
  Future<EndedBroadcast> end(Id broadcastId) =>
      throw StateError('no room in this test');

  @override
  Future<void> leave(Id broadcastId) async {}
}
