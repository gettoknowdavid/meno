import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/objectbox.g.dart' hide Id;
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/stub_api.dart';

const _ownerA = '018f2b1e-0000-7000-8000-000000000010';
const _ownerB = '018f2b1e-0000-7000-8000-000000000011';
const _subject = '018f2b1e-0000-7000-8000-000000000012';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('draft isolation', () {
    late Store store;
    late BroadcastLocalService serviceA;
    late BroadcastLocalService serviceB;

    setUp(() {
      store = Store(
        getObjectBoxModel(),
        directory:
            '${Store.inMemoryPrefix}'
            'fence-${DateTime.now().microsecondsSinceEpoch}',
      );
      // One store, two accounts: the owner is part of the service, so isolation
      // is a property of the object rather than of every call.
      final database = Database.fromStore(store);
      serviceA = BroadcastLocalService(
        Id.fromString(_ownerA),
        database,
        logger: MockMenoLogger(),
      );
      serviceB = BroadcastLocalService(
        Id.fromString(_ownerB),
        database,
        logger: MockMenoLogger(),
      );
    });

    tearDown(() => store.close());

    test('a foreign draft read is rejected, not reported as missing', () async {
      final draft = BroadcastDraft(
        id: Id.unique(),
        ownerId: Id.fromString(_ownerA),
        title: 'A',
      );
      await serviceA.saveDraft(draft);

      expect(
        () => serviceB.draft(draft.id),
        throwsA(isA<OwnerViolationException>()),
      );
    });

    test('a foreign upsert is rejected without re-owning the row', () async {
      final draft = BroadcastDraft(
        id: Id.unique(),
        ownerId: Id.fromString(_ownerA),
        title: 'A',
      );
      await serviceA.saveDraft(draft);

      await expectLater(
        serviceB.saveDraft(draft.copyWith(ownerId: Id.fromString(_ownerB))),
        throwsA(isA<OwnerViolationException>()),
      );

      // The row still belongs to A, with A's content.
      expect(serviceA.draft(draft.id)?.title, 'A');
      expect(serviceA.drafts(), hasLength(1));
      expect(serviceB.drafts(), isEmpty);
    });

    test('a service cannot be built without an owner', () {
      expect(
        () => BroadcastLocalService(
          Id.empty,
          Database.fromStore(store),
          logger: MockMenoLogger(),
        ),
        throwsA(isA<OwnerViolationException>()),
      );
    });
  });

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
