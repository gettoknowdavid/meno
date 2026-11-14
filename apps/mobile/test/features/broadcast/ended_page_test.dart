import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/features/auth/model/auth_context.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/broadcast/pages/ended_broadcast_page.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/stub_api.dart';

const _account = '018f2b1e-0000-7000-8000-000000000030';
const _broadcastId = '018f2b1e-0000-7000-8000-000000000031';
const _title = 'Your live broadcast is complete! Great job.';

Map<String, dynamic> _fixture() => {
  'id': _broadcastId,
  'title': 'Morning show',
  'description': 'Every Thursday',
  'timeZone': 'Africa/Lagos',
  'imageUrl': null,
  'imageId': null,
  'createdAt': '2026-10-28T08:00:00Z',
  'startTime': '2026-10-28T08:00:00Z',
  'endTime': '2026-10-28T08:12:00Z',
  'publishedAt': '2026-10-28T08:00:00Z',
  'durationSeconds': 720,
  'status': 'ended',
  'state': 'ended',
  'participantRole': 'host',
  'isJoined': false,
  'isBookmarked': false,
  'liveParticipantsCount': 0,
  'totalParticipants': 4,
  'recordingEnabled': false,
  'recordingUrl': null,
  'endReason': 'normal',
  'timeRemainingSeconds': null,
  'creatorId': _account,
  'creator': {'id': _account, 'fullName': 'Ada Lovelace', 'avatarId': null},
  'cohosts': <Map<String, dynamic>>[],
};

Broadcast _retained() => Broadcast(
  id: Id.fromString(_broadcastId),
  title: SingleLineString('Morning show'),
  state: .ended,
  endTime: DateTime.utc(2026, 10, 28, 8, 12),
  durationSeconds: 720,
  totalParticipants: 4,
  creator: Participant(
    id: Id.fromString(_account),
    fullName: SingleLineString('Ada Lovelace'),
  ),
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late ActiveBroadcastManager manager;
  late StubApi api;

  setUp(() async {
    Command.globalExceptionHandler = (_, _) {};
    SharedPreferences.setMockInitialValues({});
    await di.reset();
    di.pushNewScope(scopeName: Scope.root);
    di.registerSingleton<MenoLogger>(MockMenoLogger());

    manager = ActiveBroadcastManager(
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
    di.registerSingleton<ActiveBroadcastManager>(manager);

    api = StubApi();
    di.registerSingleton<BroadcastHttpService>(
      BroadcastHttpService(stubClient(api)),
    );
  });

  tearDown(() async {
    Command.globalExceptionHandler = null;
    await di.reset();
  });

  Future<void> pumpPage(WidgetTester tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: MTheme.dark,
        home: EndedBroadcastPage(broadcastId: Id.fromString(_broadcastId)),
      ),
    );
    // The load is a real HTTP round trip, so it is given real time to finish.
    for (var i = 0; i < 50; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 10)),
      );
      await tester.pump(const Duration(milliseconds: 20));
      if (find.text(_title).evaluate().isNotEmpty) return;
    }
  }

  testWidgets('the retained summary renders without loading anything', (
    tester,
  ) async {
    manager.snapshot.value = LiveSnapshot(
      phase: .ended,
      endedSummary: _retained(),
    );

    await pumpPage(tester);

    expect(find.text(_title), findsOne);
    expect(find.text('00:12:00'), findsOne);
    expect(api.requests, isEmpty, reason: 'the summary is already in memory');
  });

  testWidgets('without a summary the page loads the broadcast by its id', (
    tester,
  ) async {
    api.answers.add(() async => envelope(data: _fixture()));

    await pumpPage(tester);

    expect(api.requests, hasLength(1));
    expect(find.text(_title), findsOne);
    expect(api.requests.single.path, '/broadcasts/$_broadcastId');
  });

  testWidgets('an unknown id shows a typed refusal, not a blank screen', (
    tester,
  ) async {
    api.answers.add(() async => envelope(status: 404));

    await pumpPage(tester);

    expect(find.text('This broadcast could not be found.'), findsOne);
    expect(find.text(_title), findsNothing);
  });
}

/// The gateway no test in this file enters through.
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
