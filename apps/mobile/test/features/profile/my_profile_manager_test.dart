// import 'dart:async';
// 
// import 'package:dio/dio.dart';
// import 'package:flutter_it/flutter_it.dart';
// import 'package:flutter_secure_storage/flutter_secure_storage.dart';
// import 'package:flutter_test/flutter_test.dart';
// import 'package:meno_mobile/_core/_core.dart';
// import 'package:meno_mobile/_shared/_shared.dart';
// import 'package:meno_mobile/features/auth/auth.dart';
// import 'package:meno_mobile/features/profile/profile.dart';
// import 'package:mocktail/mocktail.dart';
// import 'package:shared_preferences/shared_preferences.dart';
// 
// import '../../_support/pump_until_found.dart';
// import '../../_support/stub_api.dart';
// 
// UserCredential _account(String id) => AuthResponseDto.fromJson({
//   'access_token': 'access-token',
//   'refresh_token': 'refresh-token',
//   'expires_in': 3600,
//   'user': {
//     'id': id,
//     'full_name': 'Account $id',
//     'email': '$id@example.com',
//     'verified': true,
//   },
// }).toDomain;
// 
// Map<String, dynamic> _profileJson(String id) => {
//   'id': id,
//   'full_name': 'Account $id',
//   'email': '$id@example.com',
//   'email_verified': true,
//   'bio': 'Writes, most days.',
//   'followers': 7,
//   'following': 5,
//   'broadcasts': 11,
// };
// 
// void main() {
//   TestWidgetsFlutterBinding.ensureInitialized();
// 
//   late StubApi api;
//   late ProfileCacheService cache;
//   late SessionStore store;
//   late UserManager user;
// 
//   setUp(() async {
//     SharedPreferences.setMockInitialValues({});
//     FlutterSecureStorage.setMockInitialValues({});
//     await di.reset();
// 
//     api = StubApi();
//     cache = ProfileCacheService(
//       LocalStorage(await SharedPreferences.getInstance()),
//     );
//     store = SessionStore(SecureStorage(const FlutterSecureStorage()));
// 
//     di.registerSingleton<MenoLogger>(MockMenoLogger());
//     configureGlobalExceptionHandler();
// 
//     final client = stubClient(api);
//     final auth = AuthManager(
//       http: AuthHttpService(client),
//       store: store,
//       logger: MockMenoLogger(),
//     );
//     await auth.init();
//     user = UserManager(auth);
//   });
// 
//   tearDown(di.reset);
// 
//   /// Signs [id] in, and waits until the app's own state agrees: the store
//   /// publishes a write in a later turn than the one it was written in.
//   Future<void> signIn(String id) async {
//     await store.saveActive(_account(id));
//     await pumpUntil(() => user.currentUserId.value == id);
//   }
// 
//   MyProfileManager newManager({MenoLogger? logger}) => MyProfileManager(
//     http: ProfileHttpService(stubClient(api)),
//     cache: cache,
//     user: user,
//     logger: logger ?? MockMenoLogger(),
//   );
// 
//   test('the account signed in at startup is read without the screen asking',
//       () async {
//     api.answers.add(() async => envelope(data: _profileJson('1')));
//     await signIn('1');
// 
//     final profile = newManager();
//     await profile.init();
//     await pumpUntil(() => !profile.profile.value.isEmpty);
// 
//     expect(profile.profile.value.id, '1');
//     expect(api.requests, hasLength(1));
//   });
// 
//   test('a read already in flight is not started again by the screen',
//       () async {
//     final answer = Completer<ResponseBody>();
//     api.answers.add(() => answer.future);
//     await signIn('1');
// 
//     final profile = newManager();
//     await profile.init();
//     await pumpUntil(
//       () => api.requests.length == 1 && profile.isLoading.value,
//     );
// 
//     // What the screen does when it opens while the background read is
//     // still on its way: it must not double the request.
//     await profile.fetch.runAsync();
//     expect(api.requests, hasLength(1));
// 
//     answer.complete(envelope(data: _profileJson('1')));
//     await pumpUntil(() => !profile.isLoading.value);
// 
//     expect(profile.profile.value.id, '1');
//   });
// 
//   test('a switch reads the new account and drops the answer for the old one',
//       () async {
//     final firstAnswer = Completer<ResponseBody>();
//     api.answers
//       ..add(() => firstAnswer.future)
//       ..add(() async => envelope(data: _profileJson('2')));
//     await signIn('1');
// 
//     final profile = newManager();
//     await profile.init();
//     await pumpUntil(
//       () => api.requests.length == 1 && profile.isLoading.value,
//     );
// 
//     await signIn('2');
//     await pumpUntil(() => profile.profile.value.id == '2');
// 
//     // The account that left answers late; the screen is not repainted with it.
//     firstAnswer.complete(envelope(data: _profileJson('1')));
//     await pumpUntil(() => !profile.isLoading.value);
// 
//     expect(profile.profile.value.id, '2');
//     expect(cache.myProfile('2')?.id, '2');
//   });
// 
//   test('a background read that fails is logged, and the cache still paints',
//       () async {
//     api.answers.add(() async => envelope(status: 500));
//     final logger = MockMenoLogger();
// 
//     await cache.saveMyProfile(
//       const MyProfile(
//         id: '1',
//         fullName: 'Account 1',
//         email: '1@example.com',
//       ),
//     );
//     await signIn('1');
// 
//     final profile = newManager(logger: logger);
//     await profile.init();
//     await pumpUntil(() => api.requests.length == 1);
//     await pumpEventQueue();
// 
//     // Nothing was raised at whoever was signing in, and what is on disk is
//     // still what the screen paints.
//     verify(() => logger.w(any(), tag: any(named: 'tag'))).called(1);
//     expect(profile.profile.value.id, '1');
//     expect(profile.isLoading.value, isFalse);
//   });
// 
//   test('a read the screen asks for reports its failure', () async {
//     api.answers.add(() async => envelope(status: 500));
//     await signIn('1');
// 
//     final profile = newManager();
//     await profile.init();
//     await pumpUntil(() => api.requests.length == 1);
//     await pumpEventQueue();
// 
//     await expectLater(
//       profile.fetch.runAsync(),
//       throwsA(isA<MenoException>()),
//     );
//     expect(profile.isLoading.value, isFalse);
//   });
// }
