import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:shared_preferences/shared_preferences.dart';

const _ownerA = '018f2b1e-0000-7000-8000-000000000010';
const _ownerB = '018f2b1e-0000-7000-8000-000000000011';
const _subject = '018f2b1e-0000-7000-8000-000000000012';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('profile cache isolation', () {
    test(
      'a viewer-less key is refused and two viewers never share one',
      () async {
        SharedPreferences.setMockInitialValues({});
        final storage = LocalStorage(await SharedPreferences.getInstance());
        final viewerA = Id.fromString(_ownerA);
        final viewerB = Id.fromString(_ownerB);
        final subject = Id.fromString(_subject);

        final cacheA = ProfileCacheService(viewerA, storage);
        await cacheA.saveProfile(
          Profile(
            id: subject,
            fullName: SingleLineString('Subject'),
            isFollowing: true,
          ),
        );

        expect(cacheA.profile(subject)?.isFollowing, isTrue);
        expect(storage.containsKey('profile.v2/$_ownerA/$_subject'), isTrue);

        // B sees a miss, not A's `isFollowing`.
        final cacheB = ProfileCacheService(viewerB, storage);
        expect(cacheB.profile(subject), isNull);

        // The subject-only key the old build wrote is never read.
        await storage.setString(
          'profile.public_$_subject',
          value: '{"id":"$_subject","fullName":"Legacy"}',
        );
        expect(cacheB.profile(subject), isNull);

        // No viewer part is not a cache at all.
        expect(
          () => ProfileCacheService(Id.empty, storage),
          throwsA(isA<OwnerViolationException>()),
        );
      },
    );
  });
}
