import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/services/broadcast_rows.dart';
import 'package:meno_mobile/features/notes/model/note_rows.dart';
import 'package:meno_mobile/objectbox.g.dart' as ob;
import 'package:shared_preferences/shared_preferences.dart';

const _accountA = '018f2b1e-0000-7000-8000-000000000000';
const _accountB = '018f2b1e-0000-7000-8000-000000000001';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late LocalStorage storage;
  late Database database;
  late AccountDataPurger purger;

  setUp(() async {
    SharedPreferences.setMockInitialValues({
      'account.v2/$_accountA/settings': '{}',
      'account.v2/$_accountB/settings': '{}',
      'profile.v2/$_accountA/$_accountA': '{}',
      'profile.v2/$_accountA/$_accountB': '{}',
      'profile.v2/$_accountB/$_accountB': '{}',
      StorageKeys.forUser(StorageKeys.profile, _accountA): '{}',
      StorageKeys.forUser(StorageKeys.settings, _accountA): '{}',
      StorageKeys.forUser(StorageKeys.recentSearches, _accountA): '[]',
      StorageKeys.forUser(StorageKeys.liveSession, _accountA): '{}',
      StorageKeys.forUser(StorageKeys.recentSearches, _accountB): '[]',
      StorageKeys.onboardingCompleted: true,
    });
    storage = LocalStorage(await SharedPreferences.getInstance());
    database = Database.fromStore(
      ob.Store(
        ob.getObjectBoxModel(),
        directory: 'memory:meno-purge-${DateTime.now().microsecondsSinceEpoch}',
      ),
    );
    purger = AccountDataPurger(storage, database);

    await database.draftBox.putAsync(
      BroadcastDraftRow(draftId: _accountA, ownerId: _accountA),
    );
    await database.draftBox.putAsync(
      BroadcastDraftRow(draftId: _accountB, ownerId: _accountB),
    );
    await database.favouriteBroadcastBox.putAsync(
      FavouriteBroadcastRow(broadcastId: _accountA, ownerId: _accountA),
    );
    await database.noteBox.putAsync(
      NoteRow(noteId: _accountA, ownerId: _accountA),
    );
    await database.noteFolderBox.putAsync(
      NoteFolderRow(folderId: _accountA, ownerId: _accountA),
    );
  });

  tearDown(() async {
    storage.dispose();
    await database.onDispose();
  });

  test('removes only the deleted account keys and rows', () async {
    await purger.purge(Id.fromWire(_accountA));

    expect(storage.containsKey('account.v2/$_accountA/settings'), isFalse);
    expect(storage.containsKey('profile.v2/$_accountA/$_accountA'), isFalse);
    expect(storage.containsKey('profile.v2/$_accountA/$_accountB'), isFalse);
    expect(
      storage.containsKey(StorageKeys.forUser(StorageKeys.profile, _accountA)),
      isFalse,
    );
    expect(
      storage.containsKey(StorageKeys.forUser(StorageKeys.settings, _accountA)),
      isFalse,
    );
    expect(
      storage.containsKey(
        StorageKeys.forUser(StorageKeys.recentSearches, _accountA),
      ),
      isFalse,
    );
    expect(
      storage.containsKey(
        StorageKeys.forUser(StorageKeys.liveSession, _accountA),
      ),
      isFalse,
    );

    expect(storage.getBool(StorageKeys.onboardingCompleted), isTrue);
    expect(storage.containsKey('account.v2/$_accountB/settings'), isTrue);
    expect(storage.containsKey('profile.v2/$_accountB/$_accountB'), isTrue);
    expect(
      storage.containsKey(
        StorageKeys.forUser(StorageKeys.recentSearches, _accountB),
      ),
      isTrue,
    );

    expect(database.draftBox.count(), 1);
    expect(database.draftBox.getAll().single.ownerId, _accountB);
    expect(database.favouriteBroadcastBox.count(), 0);
    expect(database.noteBox.count(), 0);
    expect(database.noteFolderBox.count(), 0);
  });
}
