import 'dart:convert';

import 'package:meno/_core/keys/storage_keys.dart';
import 'package:meno/_core/value_objects/id.dart';
import 'package:meno/_shared/services/local_storage.dart';
import 'package:meno/features/profile/model/_model.dart';

/// Local data source for the profile feature.
///
/// Responsibilities:
/// - Cache the signed-in user's own [ProfileDto] for instant screen render.
///
/// All data is non-sensitive and stored in [LocalStorage] (SharedPreferences).
/// Keys are namespaced per-user so multi-account is handled transparently.
///
/// Design notes:
/// - Reads are synchronous (SharedPreferences has an in-memory mirror).
/// - Writes are `Future<void>` (fire-and-forget at the call-site is fine).
/// - Corrupted JSON is silently dropped and treated as a cache miss — the
///   repository will re-fetch from the network.
///
final class ProfileLocalService {
  const ProfileLocalService(this._storage);

  final LocalStorage _storage;

  // =========================================================================
  // OWN PROFILE CACHE
  // =========================================================================

  /// Returns the cached [Profile] for [userId], or `null` on a miss.
  Profile? getCachedProfile(Id userId) {
    final raw = _storage.getString(
      StorageKeys.profileCache(userId.getOrCrash()),
    );
    if (raw == null) return null;
    try {
      return ProfileDto.fromJson(jsonDecode(raw)).toDomain;
    } catch (_) {
      // Treat corrupted JSON as a cache miss; let the network fill it.
      return null;
    }
  }

  /// Persists [profile] as the cached profile for [userId].
  ///
  /// Stores only the fields needed for display (via (ProfileDto.stripped)
  /// to keep the payload small.
  Future<void> cacheProfile(Id userId, Profile profile) {
    final jsonStr = profile.toDto.stripped.toJson();
    final value = jsonEncode(jsonStr);
    return _storage.setString(
      StorageKeys.profileCache(userId.getOrCrash()),
      value,
    );
  }

  /// Removes the cached profile for [userId] (e.g. on logout).
  Future<void> clearProfile(Id userId) {
    return _storage.remove(StorageKeys.profileCache(userId.getOrCrash()));
  }

  // =========================================================================
  // BULK CLEAR  (logout / account removal)
  // =========================================================================

  /// Clears all profile-related local data for [userId].
  ///
  /// Call this on logout or when the account is removed so stale data
  /// cannot bleed into a subsequent session.
  Future<void> clearAll(Id userId) => Future.wait([clearProfile(userId)]);
}
