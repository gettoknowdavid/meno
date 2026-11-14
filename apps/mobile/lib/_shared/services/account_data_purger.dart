import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:meno_mobile/objectbox.g.dart' as ob;

/// Removes every local trace of one account from the device.
///
/// Signing out ends a session; deleting an account ends the account itself, so
/// the caches that only exist to make that account's next launch fast must not
/// outlive it. This service owns that sweep: the account-scoped preference keys
/// and the ObjectBox rows whose `ownerId` names the account. Other accounts on
/// the device are never touched — every lookup is keyed by the deleted
/// account's id.
///
/// The vault is deliberately not handled here. It is secure storage with its
/// own mutation queue, and `SessionService.forgetAccount` is the one operation
/// that removes the account and its remembered identity from it.
final class AccountDataPurger {
  /// Creates a purger over the device's preferences and object store.
  const new(this._storage, this._database);

  final LocalStorage _storage;
  final Database _database;

  /// Removes this account's cached preferences and local rows.
  Future<void> purge(Id accountId) async {
    if (!accountId.isValid) {
      throw const OwnerViolationException(
        message: 'Account data needs the account it belongs to.',
      );
    }

    final owner = accountId.getOrCrash;
    await _removePreferences(owner);
    await _removeRows(owner);
  }

  /// Removes every preference key scoped to [owner].
  Future<void> _removePreferences(String owner) async {
    // Snapshot the keys first: removing one while walking the live set would
    // mutate the collection under the loop.
    final keys = _storage.keys
        .where((key) => _belongsTo(key, owner))
        .toList(growable: false);

    for (final key in keys) {
      await _storage.remove(key);
    }
  }

  /// Whether [key] is an account-scoped key for [owner].
  bool _belongsTo(String key, String owner) {
    return key.startsWith('account.v2/$owner/') ||
        key.startsWith('profile.v2/$owner/') ||
        key == StorageKeys.forUser(StorageKeys.profile, owner) ||
        key == StorageKeys.forUser(StorageKeys.settings, owner) ||
        key == StorageKeys.forUser(StorageKeys.recentSearches, owner) ||
        key == StorageKeys.forUser(StorageKeys.liveSession, owner);
  }

  /// Removes every ObjectBox row this account owns.
  ///
  /// The Bible tables are intentionally absent: they hold public reference text
  /// and carry no owner, so they are device data rather than an account's.
  Future<void> _removeRows(String owner) async {
    await _removeOwned(
      _database.draftBox,
      ob.BroadcastDraftRow_.ownerId.equals(owner),
    );
    await _removeOwned(
      _database.favouriteBroadcastBox,
      ob.FavouriteBroadcastRow_.ownerId.equals(owner),
    );
    await _removeOwned(_database.noteBox, ob.NoteRow_.ownerId.equals(owner));
    await _removeOwned(
      _database.noteFolderBox,
      ob.NoteFolderRow_.ownerId.equals(owner),
    );
  }

  /// Removes the rows matching [condition] from [box].
  Future<void> _removeOwned<T>(ob.Box<T> box, ob.Condition<T> condition) async {
    final query = box.query(condition).build();
    try {
      final ids = query.findIds();
      if (ids.isNotEmpty) await box.removeManyAsync(ids);
    } finally {
      query.close();
    }
  }
}
