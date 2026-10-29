import 'dart:convert';

import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';

/// The settings module's device cache.
///
/// Settings are read from the device first so the switches show the last known
/// state before the API answers, and written after every change so the next
/// launch starts from it. The cache is scoped to the account, because one
/// device can hold several.
///
/// The account is part of the service, not an argument (audit M05): it is
/// created in the account scope, so its key always names the signed-in account
/// and no caller can read or write another one's settings.
final class SettingsLocalService {
  /// Creates the cache for one account.
  ///
  /// An invalid owner is refused rather than being flattened into a key that
  /// belongs to nobody.
  new(this._ownerId, this._storage) {
    if (!_ownerId.isValid) {
      throw const OwnerViolationException(
        message: 'A settings cache needs the account it belongs to.',
      );
    }
  }

  final Id _ownerId;
  final LocalStorage _storage;

  /// This account's cached settings, or null when there are none.
  UserSettings? getSettings() {
    final json = _storage.getString(_key);
    if (json == null) return null;
    return SettingsDto.fromJson(jsonDecode(json) as Map<String, dynamic>)
        .toDomain;
  }

  /// Stores [settings] for this account.
  Future<void> saveSettings(UserSettings settings) {
    return _storage.setString(_key, value: jsonEncode(settings.toDto.toJson()));
  }

  /// The account-qualified key every read and write goes through.
  String get _key => 'account.v2/${_ownerId.getOrCrash}/settings';
}
