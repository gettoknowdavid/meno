import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/features/discover/discover.dart';
import 'package:shared_preferences/shared_preferences.dart';

const _owner = 'cfa3dd64-e5bb-4bc8-b28b-efda92512b16';
const _otherOwner = 'c8899b7b-35e8-4653-83a0-d7a8a3e9cda8';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late LocalStorage storage;
  late DiscoverLocalService local;

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    storage = LocalStorage(await SharedPreferences.getInstance());
    local = DiscoverLocalService(Id.fromString(_owner), storage);
  });

  test('recent searches are kept newest first, without repeats', () async {
    await local.addRecentSearch('ada');
    await local.addRecentSearch('grace');
    // Case does not make a new search: the same word typed again moves to the
    // top instead of appearing twice.
    await local.addRecentSearch('ADA');

    expect(local.recentSearches, ['ADA', 'grace']);
  });

  test('blank terms are not searches and are not stored', () async {
    await local.addRecentSearch('   ');
    await local.addRecentSearch('');

    expect(local.recentSearches, isEmpty);
    expect(storage.keys, isEmpty);
  });

  test('the history keeps at most ten terms, dropping the oldest', () async {
    for (var i = 0; i < DiscoverLocalService.maxRecentSearches + 2; i++) {
      await local.addRecentSearch('term $i');
    }

    final recents = local.recentSearches;

    expect(recents, hasLength(DiscoverLocalService.maxRecentSearches));
    expect(recents.first, 'term 11');
    expect(recents, isNot(contains('term 0')));
    expect(recents, isNot(contains('term 1')));
  });

  test(
    'a term can be dropped, and dropping an absent one changes nothing',
    () async {
      await local.addRecentSearch('ada');
      await local.addRecentSearch('grace');

      await local.removeRecentSearch('ada');
      expect(local.recentSearches, ['grace']);

      // A stale tap in a list that has already moved on must not rewrite it.
      await local.removeRecentSearch('ada');
      expect(local.recentSearches, ['grace']);
    },
  );

  test('clearing forgets every term and its key', () async {
    await local.addRecentSearch('ada');

    await local.clearRecentSearches();

    expect(local.recentSearches, isEmpty);
    expect(storage.keys, isEmpty);
  });

  test('the history is stored under the account that typed it', () async {
    await local.addRecentSearch('ada');

    // Another account reads its own key, which nothing has written.
    final other = DiscoverLocalService(Id.fromString(_otherOwner), storage);
    expect(other.recentSearches, isEmpty);

    // And the key names the owner, so neither can read the other's list.
    expect(
      storage.containsKey(
        StorageKeys.forUser(StorageKeys.recentSearches, _owner),
      ),
      isTrue,
    );
    expect(
      storage.containsKey(
        StorageKeys.forUser(StorageKeys.recentSearches, _otherOwner),
      ),
      isFalse,
    );
  });

  test('a history with no owner is refused rather than shared', () {
    expect(
      () => DiscoverLocalService(Id.empty, storage),
      throwsA(isA<OwnerViolationException>()),
    );
  });

  test('the returned list is a copy', () async {
    await local.addRecentSearch('ada');

    local.recentSearches.add('not stored');

    expect(local.recentSearches, ['ada']);
  });
}
