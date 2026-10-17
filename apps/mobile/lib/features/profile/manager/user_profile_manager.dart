import 'dart:async';

import 'package:flutter/cupertino.dart' show ChangeNotifier;
import 'package:flutter/foundation.dart' show ChangeNotifier;
import 'package:flutter/material.dart' show ChangeNotifier;
import 'package:flutter/widgets.dart' show ChangeNotifier;
import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart' show ChangeNotifier;
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/services/_services.dart';

/// Someone else's profile, for the screen that shows it.
///
/// Registered in the scope UserProfilePage pushes, so it exists only while that
/// page does: coming back to a profile reads the cache and refetches rather
/// than showing a stale answer from an earlier visit.
///
/// The profile itself is a [UserProfileProxy] taken from
/// [UserProfileRepository], not a raw entity. That is what lets the page and
/// every other place drawing this account — a broadcast card, the header of a
/// room they host — share one answer about whether the viewer follows them, and
/// see it change the moment the button is tapped. This manager is one *holder*:
/// it acquires the proxy when it is built and releases it when its page's scope
/// is popped.
final class UserProfileManager implements Disposable {
  /// Creates a manager for [userId] over its service, its cache and the
  /// repository of live proxies.
  new({
    required this.userId,
    required this._http,
    required this._cache,
    required this._profiles,
  }) {
    fetch = .createAsyncNoParamNoResult(
      _fetch,
      errorFilterFn: menoExceptionFilter,
      debugName: 'userProfile.fetch',
    );

    // Whatever the last visit saw is shown straight away, and the fetch that
    // follows replaces it. The repository hands back the proxy somebody else
    // may already be watching, which is the point.
    _proxy = _profiles.acquire(_cache.profile(userId) ?? .empty);
  }

  /// The account whose profile this is.
  final Id userId;

  final ProfileHttpService _http;
  final ProfileCacheService _cache;
  final UserProfileRepository _profiles;

  late final UserProfileProxy _proxy;

  /// The profile, with the viewer's own standing layered on top.
  ///
  /// A [ChangeNotifier]: a screen watches it for a rebuild rather than watching
  /// a value, because the follow state changes without the entity changing.
  UserProfileProxy get profile => _proxy;

  /// Reads the profile from the API and saves it for the next visit.
  late final Command<void, void> fetch;

  @override
  FutureOr<dynamic> onDispose() {
    fetch.dispose();
    // One hold given up. The proxy itself lives until the last holder lets go,
    // which may be a card behind this page.
    _profiles.release(_proxy);
  }

  /// Loads the profile.
  ///
  /// The answer is adopted through the proxy's `target`, which also settles any
  /// optimistic follow against what the API actually recorded.
  Future<void> _fetch() async {
    if (userId.isEmpty) return;

    final remote = await _http.profile(userId);
    _proxy.target = remote;
    await _cache.saveProfile(remote);
  }
}
