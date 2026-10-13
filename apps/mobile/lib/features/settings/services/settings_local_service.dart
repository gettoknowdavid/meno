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
final class SettingsLocalService {
  /// Creates a service over [_storage].
  const new(this._storage);

  final LocalStorage _storage;

  /// The cached settings for [userId], or null when there are none.
  UserSettings? getSettings(String userId) {
    final json = _storage.getString(
      StorageKeys.forUser(StorageKeys.settings, userId),
    );
    if (json == null) return null;
    return SettingsDto.fromJson(
      jsonDecode(json) as Map<String, dynamic>,
    ).toDomain;
  }

  /// Stores [settings] for [userId].
  Future<void> saveSettings(String userId, UserSettings settings) {
    return _storage.setString(
      StorageKeys.forUser(StorageKeys.settings, userId),
      value: jsonEncode(settings.toDto.toJson()),
    );
  }
}
