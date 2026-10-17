// import 'dart:async';
// 
// import 'package:flutter_it/flutter_it.dart';
// import 'package:flutter_test/flutter_test.dart';
// import 'package:meno_mobile/_core/_core.dart';
// import 'package:meno_mobile/_shared/_shared.dart';
// import 'package:meno_mobile/features/broadcast/broadcast.dart';
// import 'package:mocktail/mocktail.dart';
// import 'package:shared_preferences/shared_preferences.dart';
// 
// import '../../_support/stub_api.dart';
// 
// class MockMenoLogger extends Mock implements MenoLogger;
// 
// /// A broadcast that is on air, as the API would describe one.
// Broadcast _broadcast() =>
//     Broadcast.empty.copyWith(id: 'b1', state: BroadcastState.live);
// 
// void main() {
//   TestWidgetsFlutterBinding.ensureInitialized();
// 
//   late StubApi api;
//   late LiveScopeManager live;
//   late StreamManager stream;
//   late LiveSessionStore store;
// 
//   setUp(() async {
//     SharedPreferences.setMockInitialValues({});
//     await di.reset();
//     di.pushNewScope(scopeName: Scope.root);
// 
//     // `main` registers one of these before the app runs; a command whose error
//     // has nowhere local to go asserts in debug without it.
//     Command.globalExceptionHandler = (error, stackTrace) {};
// 
//     store = LiveSessionStore(
//       LocalStorage(await SharedPreferences.getInstance()),
//       userId: 'user-1',
//     );
//     api = StubApi();
//     live = LiveScopeManager(
//       'user-1',
//       logger: MockMenoLogger(),
//       store: store,
//       http: BroadcastHttpService(stubClient(api)),
//       endedBroadcasts: const Stream<EndedBroadcast>.empty(),
//     );
//     stream = StreamManager(
//       http: BroadcastHttpService(stubClient(api)),
//       live: live,
//       userId: 'user-1',
//       logger: MockMenoLogger(),
//     );
//   });
// 
//   tearDown(() {
//     Command.globalExceptionHandler = null;
//     return di.reset();
//   });
// 
//   test('joining a broadcast opens the room its session belongs to', () async {
//     api.answers.add(
//       () async => envelope(
//         data: {
//           'broadcast': _broadcast().toDto.toJson(),
//           'token': 'room-token',
//         },
//       ),
//     );
// 
//     await stream.joinBroadcast.runAsync('b1');
// 
//     expect(stream.session.value?.token, 'room-token');
//     expect(di.currentScopeName, Scope.liveFor('b1'));
//     expect(di<LiveSession>().broadcastId, 'b1');
//     expect(live.activeBroadcastId.value, 'b1');
//     // Remembered, so the room is still there for a process that stops here.
//     expect(store.read()?.token, 'room-token');
//   });
// 
//   test('a join the API refuses leaves no room open', () async {
//     api.answers.add(() async => envelope(status: 404));
// 
//     // A caller that awaits the join gets the failure; a page runs it and lets
//     // the error filter report it.
//     await expectLater(
//       stream.joinBroadcast.runAsync('b1'),
//       throwsA(isA<MenoException>()),
//     );
// 
//     expect(stream.session.value, isNull);
//     expect(di.currentScopeName, Scope.root);
//     expect(live.isLive.value, isFalse);
//     expect(store.read(), isNull);
//   });
// }
