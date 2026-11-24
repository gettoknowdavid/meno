import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

const _accountId = '018f2b1e-0000-7000-8000-000000000000';
const _otherId = '018f2b1e-0000-7000-8000-000000000001';

/// The record `GET /users/me` answers with.
Map<String, dynamic> _profileJson({String id = _accountId, String? name}) => {
  'id': id,
  'full_name': name ?? 'Jim Halpert',
  'email': 'jim@example.com',
  'email_verified': true,
};

Id get _account => Id.fromWire(_accountId);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late StubApi api;
  late ProfileCacheService cache;

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    api = StubApi();
    cache = ProfileCacheService(
      _account,
      LocalStorage(await SharedPreferences.getInstance()),
    );
  });

  /// A manager for [_account] over the stub API and the device cache.
  ///
  /// It takes its account context, not a global selector: the account scope it
  /// lives in is what says which account it answers for (Step 26).
  MyProfileManager newManager({UserProfileRepository? profiles}) {
    final manager = MyProfileManager(
      context: AccountContext(accountId: _account, generation: 1),
      http: ProfileHttpService(stubClient(api)),
      cache: cache,
      profiles: profiles ?? UserProfileRepository(),
      logger: MockMenoLogger(),
    );
    addTearDown(manager.onDispose);
    return manager;
  }

  test(
    'init installs the cached record and asks the API for nothing',
    () async {
      await cache.saveMyProfile(
        MyProfile(
          id: _account,
          fullName: FullName('Jim Halpert'),
          email: Email('jim@example.com'),
        ),
      );

      final manager = await newManager().init();

      expect(manager.profile.value.fullName.orEmpty, 'Jim Halpert');
      expect(api.requests, isEmpty);
    },
  );

  test(
    'init without a cached record installs a placeholder for the account',
    () async {
      final manager = await newManager().init();

      // A profile carrying the account id, not `MyProfile.empty`: the screen is
      // this account's own record, so it paints instead of offering a retry for
      // data that is still on its way.
      expect(manager.profile.value.id, _account);
      expect(manager.profile.value.isEmpty, isFalse);
      expect(manager.isLoading.value, isFalse);
      expect(api.requests, isEmpty);
    },
  );

  test('refresh reads the record once the account is ready', () async {
    api.answers.add(() async => envelope(data: _profileJson()));

    final manager = await newManager().init();
    expect(api.requests, isEmpty);

    await manager.refresh();

    expect(api.requests, hasLength(1));
    expect(manager.profile.value.fullName.orEmpty, 'Jim Halpert');
    expect(cache.myProfile()?.fullName.orEmpty, 'Jim Halpert');
    expect(manager.isLoading.value, isFalse);
  });

  test("following somebody moves this account's own count at once", () async {
    // The report behind this test: subscribing from a broadcast, then opening
    // My Profile, showed the old "Subscriptions" number. The follow is decided
    // on somebody else's page, but the count belongs to this account.
    di.registerSingleton<ProfileHttpService>(
      ProfileHttpService(stubClient(api)),
    );
    addTearDown(di.reset);

    final profiles = UserProfileRepository();
    api.answers.add(() async => envelope(data: _profileJson()));
    final manager = await newManager(profiles: profiles).init();
    await manager.refresh();
    expect(manager.profile.value.stats.following, 0);

    // The follow itself: a proxy for the account being subscribed to, over the
    // same repository the manager listens to.
    api.answers.add(() async => envelope(data: _profileJson()));
    final proxy = profiles.acquire(Id.fromWire(_otherId));
    addTearDown(() => profiles.release(proxy));

    await proxy.toggleFollow.runAsync();

    expect(proxy.isFollowing, isTrue);
    expect(
      manager.profile.value.stats.following,
      1,
      reason: 'the optimistic count must not wait for another read',
    );
    expect(
      cache.myProfile()?.stats.following,
      1,
      reason: 'the next launch shows the same number',
    );
  });

  test('an answer that lands after disposal is discarded', () async {
    final answer = Completer<ResponseBody>();
    api.answers.add(() => answer.future);

    final manager = await newManager().init();
    final read = manager.refresh();
    await pumpUntil(() => api.requests.isNotEmpty);
    expect(api.requests, hasLength(1));

    await manager.onDispose();
    answer.complete(envelope(data: _profileJson(name: 'Too late')));
    await read;

    expect(manager.profile.value.fullName.orEmpty, isEmpty);
    expect(cache.myProfile(), isNull);
  });

  test('a record for another account is not applied or cached', () async {
    api.answers.add(() async => envelope(data: _profileJson(id: _otherId)));

    final manager = await newManager().init();
    await manager.refresh();

    expect(manager.profile.value.id, _account);
    expect(manager.profile.value.fullName.orEmpty, isEmpty);
    expect(cache.myProfile(), isNull);
  });
}
