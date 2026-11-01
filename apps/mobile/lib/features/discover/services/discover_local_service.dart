import 'package:flutter/foundation.dart' show Listenable;
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';

/// Discover's own slice of device storage: the terms somebody searched for.
///
/// Nothing else is kept here. Broadcast and profile results are the API's
/// answer to a query, not a preference, so they are not cached; what survives a
/// restart is the short list of searches the screen offers before anything has
/// been typed.
///
/// The account is part of the service, not an argument (audit M05): it is
/// created in the account scope, so its key always names the signed-in account
/// and no caller can read or write another one's history.
final class DiscoverLocalService {
  /// Creates the store for one account.
  ///
  /// An invalid owner is refused rather than being flattened into a key that
  /// belongs to nobody.
  new(this._ownerId, this._storage) {
    if (!_ownerId.isValid) {
      throw const OwnerViolationException(
        message: 'Recent searches need the account they belong to.',
      );
    }
  }

  final Id _ownerId;
  final LocalStorage _storage;

  /// How many terms are kept. Older ones fall off the end.
  static const int maxRecentSearches = 10;

  /// Tells a listener when the stored terms changed.
  ///
  /// The storage is one notifier for every key on the device, so a listener
  /// re-reads this account's key and ignores the rest rather than trying to
  /// work out whether the change was its own.
  Listenable get changes => _storage;

  /// The stored terms, most recent first.
  ///
  /// A copy: the caller mutates its own list, and nothing it does can reach the
  /// stored value.
  List<String> get recentSearches =>
      List.of(_storage.getStringList(_key) ?? const <String>[]);

  /// Puts [term] at the top of the list, de-duplicated case-insensitively.
  ///
  /// A blank term is not a search and is ignored. Nothing is written when the
  /// list would not change, so repeating a search does not wake every listener
  /// for an identical value.
  Future<void> addRecentSearch(String term) async {
    final trimmed = term.trim();
    if (trimmed.isEmpty) return;

    final current = recentSearches;
    current.removeWhere(
      (entry) => entry.toLowerCase() == trimmed.toLowerCase(),
    );
    current.insert(0, trimmed);

    if (current.length > maxRecentSearches) {
      current.removeRange(maxRecentSearches, current.length);
    }

    await _storage.setStringList(_key, value: current);
  }

  /// Drops one term.
  ///
  /// A term that is not stored is not an error, and nothing is written: a stale
  /// tap in a list that has already moved on must not rewrite the list.
  Future<void> removeRecentSearch(String term) async {
    final current = recentSearches;
    final removed = current.length;
    current.removeWhere((entry) => entry == term);
    if (current.length == removed) return;

    await _storage.setStringList(_key, value: current);
  }

  /// Forgets every term.
  Future<void> clearRecentSearches() => _storage.remove(_key);

  /// The account-qualified key every read and write goes through.
  String get _key =>
      StorageKeys.forUser(StorageKeys.recentSearches, _ownerId.getOrCrash);
}
