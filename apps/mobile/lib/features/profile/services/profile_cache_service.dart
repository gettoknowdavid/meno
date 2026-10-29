import 'dart:async';
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
/// every account that signs in on it, which is why every key names the
/// account it belongs to: one account must never read another's.
///
/// Every other profile is keyed by **viewer *and* subject**, not by subject
/// alone (H14): the same profile can show a different `isFollowing` to two
/// accounts, and a subject-only key hands the second viewer whatever the first
/// one saw. The viewer is fixed when this cache is created — it lives in the
/// account scope — so a viewer-less read cannot be written at all.
///
/// Reads are synchronous (preferences keep an in-memory mirror), writes are
/// fire-and-forget at the call site, and a value that cannot be decoded is
/// treated as a miss *and removed*, so a malformed entry cannot survive to be
/// misread again — the next fetch refills it.
final class ProfileCacheService {
  /// Creates a cache over [_storage] for the signed-in [_ownerId].
  ///
  /// The viewer is part of the service, not an argument: a cache with no viewer
  /// part could only key a profile by its subject, which is the H14 leak, and a
  /// caller that could name the viewer could name somebody else's. An invalid
  /// id is refused here instead of being flattened into a key that belongs to
  /// nobody.
  new(this._ownerId, this._storage) {
    if (!_ownerId.isValid) {
      throw const OwnerViolationException(
        message: 'A profile cache needs the viewer it belongs to.',
      );
    }
  }

  final LocalStorage _storage;

  /// The account reading and writing this cache.
  final Id _ownerId;

  /// This account's own cached profile, or null when nothing usable is stored.
  MyProfile? myProfile() {
    return _read(_ownProfileKey, (json) => MeDto.fromJson(json).toDomain);
  }

  /// Stores [profile] as this account's own.
  ///
  /// A profile for another account is foreign input: the own record is keyed by
  /// the service's owner, so writing somebody else's here would be a lie.
  Future<void> saveMyProfile(MyProfile profile) {
    if (profile.id.isValid && profile.id != _ownerId) {
      throw const OwnerViolationException();
    }
    return _write(_ownProfileKey, profile.toDto.toJson());
  }

  /// The cached copy of [userId]'s profile *as this viewer saw it*, or null
  /// on a miss.
  ///
  /// The legacy subject-only key (`profile.public_<id>`) is never read: its
  /// entries were written by whichever account fetched them first.
  Profile? profile(Id userId) => _read(
    _publicProfileKey(userId),
    (json) => ProfileDto.fromJson(json).toDomain,
  );

  /// Stores [profile] as this viewer's copy of that profile.
  Future<void> saveProfile(Profile profile) =>
      _write(_publicProfileKey(profile.id), profile.toDto.toJson());

  /// The key this account's own record is stored under.
  String get _ownProfileKey =>
      '$_prefix/${_ownerId.getOrCrash}/${_ownerId.getOrCrash}';

  /// Viewer-relative key for [subject]'s profile.
  String _publicProfileKey(Id subject) =>
      '$_prefix/${_ownerId.getOrCrash}/${subject.getOrCrash}';

  /// The cache's namespace on the device.
  static const _prefix = 'profile.v2';

  /// Removes the keys older builds wrote without a viewer segment.
  ///
  /// Those entries were written by whichever account fetched a profile first,
  /// so their `isFollowing` belongs to nobody in particular — they cannot be
  /// migrated, only deleted (audit H14, assumption A13: installed builds may
  /// exist). Nothing here ever read them; this is the sweep that takes them off
  /// the device.
  Future<void> purgeLegacyEntries() async {
    final legacy = _storage.keys
        .where((key) => key.startsWith(StorageKeys.publicProfile))
        .toList();
    for (final key in legacy) {
      await _storage.remove(key);
    }
  }

  /// Reads and decodes [key], treating anything unreadable as a miss.
  ///
  /// An entry that cannot be *decoded* is removed as well as skipped: it would
  /// otherwise sit there forever, a permanent miss that no fetch can ever turn
  /// into a hit.
  T? _read<T>(String key, T Function(Map<String, dynamic> json) decode) {
    final raw = _storage.getString(key);
    if (raw == null) return null;

    try {
      final json = jsonDecode(raw);
      if (json is! Map<String, dynamic>) throw const FormatException();
      return decode(json);
    } on Exception catch (_) {
      // A cache that cannot be read is a cache miss, not a failure: the
      // network is about to fill it again.
      unawaited(_storage.remove(key));
      return null;
    }
  }

  /// Encodes and stores [json] under [key].
  Future<void> _write(String key, Map<String, dynamic> json) =>
      _storage.setString(key, value: jsonEncode(json));
}
