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

    // Acquired by the *requested* account, which is what the repository keys
    // on and what a release looks up — never the id of a fetched answer.
    _proxy = _profiles.acquire(userId);

    // Whatever the last visit saw is shown straight away, and the fetch that
    // follows replaces it. The repository hands back the proxy somebody else
    // may already be watching, which is the point.
    final cached = _cache.profile(userId);
    if (cached != null) _proxy.target = cached;
  }

  /// The account whose profile this is.
  final Id userId;

  final ProfileHttpService _http;
  final ProfileCacheService _cache;
  final UserProfileRepository _profiles;

  late final UserProfileProxy _proxy;

  /// Whether this manager has been disposed.
  ///
  /// Disposing a command does not cancel its body, so every continuation after
  /// an await checks this before touching a notifier or the cache (audit H15).
  bool _disposed = false;

  /// The profile, with the viewer's own standing layered on top.
  ///
  /// A [ChangeNotifier]: a screen watches it for a rebuild rather than watching
  /// a value, because the follow state changes without the entity changing.
  UserProfileProxy get profile => _proxy;

  /// Reads the profile from the API and saves it for the next visit.
  late final Command<void, void> fetch;

  @override
  FutureOr<dynamic> onDispose() {
    _disposed = true;
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
    if (_disposed || userId.isEmpty) return;

    final remote = await _http.profile(userId);
    // The page may have closed while the answer was on its way; adopting it
    // then would write to a disposed proxy.
    if (_disposed) return;

    _profiles.apply(_proxy, remote);
    await _cache.saveProfile(remote);
  }
}
