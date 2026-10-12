import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/auth/manager/user_manager.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/services/_services.dart';

/// The signed-in account's own profile, and the cache behind it.
///
/// It follows [UserManager.currentUserId] rather than taking an id when it is
/// built. Accounts are switched without restarting the app, and a manager that
/// captured one id would keep serving the previous account's name, avatar and
/// counters — and write them into the new account's cache. Following the
/// active account means every switch puts that account's own profile on
/// screen, from disk first and from the API a moment later.
///
/// It never starts a request of its own: the screens that show it ask for a
/// read, so a signed-in account that never opens the profile is never asked
/// for one.
final class MyProfileManager implements Disposable {
  /// Creates a manager over its service, its cache and the active account.
  new({required this._http, required this._cache, required this._user});

  final ProfileHttpService _http;
  final ProfileCacheService _cache;
  final UserManager _user;

  final ValueNotifier<MyProfile> _profile = ValueNotifier(MyProfile.empty);

  ListenableSubscription? _subscription;

  /// The signed-in account's profile, or [MyProfile.empty] when there is none.
  ValueListenable<MyProfile> get profile => _profile;

  /// Reads the signed-in account's record and saves it for next time.
  late final Command<void, void> fetch = Command.createAsyncNoParamNoResult(
    _fetch,
    errorFilterFn: menoExceptionFilter,
    debugName: 'profile.fetch',
  );

  /// Adopts the account already signed in, then follows the active one.
  ///
  /// Returns `this` so the locator can register it with the `init()` pattern:
  /// the subscription has to exist before the app starts, or the first account
  /// switch would go unnoticed.
  Future<MyProfileManager> init() async {
    _subscription = _user.currentUserId.listen(_onAccountChanged);
    _adopt(_user.currentUserId.value);
    return this;
  }

  /// Adopts [profile] as the account's, after a save the API accepted.
  ///
  /// Nothing is fetched here: the caller already holds the record the API
  /// answered with, and going back for it would only risk overwriting it with
  /// an older copy.
  void update(MyProfile profile) {
    if (profile.isEmpty) return;
    _profile.value = profile;
    unawaited(_cache.saveMyProfile(profile));
  }

  @override
  FutureOr<dynamic> onDispose() {
    _subscription?.cancel();
    fetch.dispose();
    _profile.dispose();
  }

  /// Loads the record for whoever is signed in.
  Future<void> _fetch() async {
    final userId = _user.currentUserId.value;
    // Nobody is signed in: there is no record to read, and a request would be
    // answered with a 401.
    if (userId == null || userId.isEmpty) return;

    final remote = await _http.me();

    // The account can change while the request is in flight. The answer then
    // belongs to the account that asked rather than the one signed in now, so
    // it is dropped instead of shown under the wrong name.
    if (_user.currentUserId.value != userId) return;

    _profile.value = remote;
    await _cache.saveMyProfile(remote);
  }

  /// Follows the active account, putting its cached profile in place.
  ///
  /// The screen re-reads when this happens; the manager only swaps the state,
  /// so signing in somewhere else is never a request nobody asked for.
  void _onAccountChanged(String? userId, ListenableSubscription subscription) {
    _adopt(userId);
  }

  /// Puts [userId]'s cached profile in place, or clears the screen when nobody
  /// is signed in.
  void _adopt(String? userId) {
    if (userId == null || userId.isEmpty) {
      _profile.value = MyProfile.empty;
      return;
    }
    _profile.value = _cache.myProfile(userId) ?? MyProfile.empty;
  }
}
