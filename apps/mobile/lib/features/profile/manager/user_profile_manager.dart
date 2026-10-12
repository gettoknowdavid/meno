import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/services/_services.dart';

/// Someone else's profile, for the screen that shows it.
///
/// Registered in the scope UserProfilePage pushes, so it exists only while that
/// page does: coming back to a profile reads the cache and refetches rather
/// than showing a stale answer from an earlier visit.
final class UserProfileManager implements Disposable {
  /// Creates a manager for [userId] over its service and its cache.
  new({
    required this.userId,
    required this._http,
    required this._cache,
  }) {
    // Whatever the last visit saw is shown straight away; the fetch that
    // follows replaces it.
    _profile.value = _cache.profile(userId) ?? Profile.empty;
  }

  /// The account whose profile this is.
  final String userId;

  final ProfileHttpService _http;
  final ProfileCacheService _cache;

  final ValueNotifier<Profile> _profile = ValueNotifier(Profile.empty);

  /// The profile, or [Profile.empty] until anything is known.
  ValueListenable<Profile> get profile => _profile;

  /// Reads the profile from the API and saves it for the next visit.
  late final Command<void, void> fetch = Command.createAsyncNoParamNoResult(
    _fetch,
    errorFilterFn: menoExceptionFilter,
    debugName: 'userProfile.fetch',
  );

  @override
  FutureOr<dynamic> onDispose() {
    fetch.dispose();
    _profile.dispose();
  }

  /// Loads the profile.
  Future<void> _fetch() async {
    if (userId.isEmpty) return;

    final remote = await _http.profile(userId);
    _profile.value = remote;
    await _cache.saveProfile(remote);
  }
}
