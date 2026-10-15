import 'dart:convert';

import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';

/// The profile screens' read-through cache.
///
/// Two things are kept, both under the account they describe:
///
/// * the signed-in account's own record, so the profile screen can paint a
///   name, an avatar and the counters before the network answers;
/// * every other profile that has been opened, so revisiting one is instant.
///
/// Nothing here is secret — it is a name, a bio and some counts — so it lives
/// in [LocalStorage] rather than secure storage. Device storage is shared by
/// every account that signs in on it, which is why every key is scoped to the
/// account it belongs to ([StorageKeys.forUser]): one account must never read
/// another's.
///
/// Reads are synchronous (preferences keep an in-memory mirror), writes are
/// fire-and-forget at the call site, and a value that cannot be decoded is
/// treated as a miss — the next fetch refills it.
final class ProfileCacheService {
  /// Creates a cache over [_storage].
  const new(this._storage);

  final LocalStorage _storage;

  /// The cached profile of [userId], or null when nothing usable is stored.
  MyProfile? myProfile(String userId) => _read(
    StorageKeys.forUser(StorageKeys.profile, userId),
    (json) => MeDto.fromJson(json).toDomain,
  );

  /// Stores [profile] as the signed-in account's own.
  Future<void> saveMyProfile(MyProfile profile) => _write(
    StorageKeys.forUser(StorageKeys.profile, profile.id.getOrCrash),
    profile.toDto.toJson(),
  );

  /// The cached copy of [userId]'s profile, or null on a miss.
  Profile? profile(String userId) => _read(
    StorageKeys.forUser(StorageKeys.publicProfile, userId),
    (json) => ProfileDto.fromJson(json).toDomain,
  );

  /// Stores [profile] as the profile of the account it belongs to.
  Future<void> saveProfile(Profile profile) => _write(
    StorageKeys.forUser(StorageKeys.publicProfile, profile.id.getOrCrash),
    profile.toDto.toJson(),
  );

  /// Reads and decodes [key], treating anything unreadable as a miss.
  T? _read<T>(String key, T Function(Map<String, dynamic> json) decode) {
    final raw = _storage.getString(key);
    if (raw == null) return null;

    try {
      final json = jsonDecode(raw);
      if (json is! Map<String, dynamic>) return null;
      return decode(json);
    } on Exception catch (_) {
      // A cache that cannot be read is a cache miss, not a failure: the
      // network is about to fill it again.
      return null;
    }
  }

  /// Encodes and stores [json] under [key].
  Future<void> _write(String key, Map<String, dynamic> json) =>
      _storage.setString(key, value: jsonEncode(json));
}
