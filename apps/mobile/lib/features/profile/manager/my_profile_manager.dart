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
/// An account's record is read as soon as that account becomes active, so the
/// screen opens on it instead of waiting for a request of its own. That read is
/// a background one — its failure is logged, not raised, because nobody asked
/// for it. [fetch] stays the door the screens use for a retry or a pull to
/// refresh, and it is where a failure reaches the user.
final class MyProfileManager implements Disposable {
  /// Creates a manager over its service, its cache, the active account and the
  /// log.
  new({
    required this._http,
    required this._cache,
    required this._user,
    required this._logger,
  }) {
    fetch = .createAsyncNoParamNoResult(
      _load,
      errorFilterFn: menoExceptionFilter,
      debugName: 'myProfile.fetch',
    );
  }

  final ProfileHttpService _http;
  final ProfileCacheService _cache;
  final UserManager _user;
  final MenoLogger _logger;

  final ValueNotifier<MyProfile> _profile = ValueNotifier(.empty);
  final ValueNotifier<bool> _isLoading = ValueNotifier(false);

  ListenableSubscription? _subscription;
  Id? _loadingUserId;
  bool _disposed = false;

  /// The signed-in account's profile, or [MyProfile.empty] when there is none.
  ValueListenable<MyProfile> get profile => _profile;

  /// Whether a read is in flight — whoever started it.
  ///
  /// The screen shows its skeleton on this rather than on `fetch.isRunning`:
  /// the read that started when the account became active is the same read it
  /// would otherwise ask for.
  ValueListenable<bool> get isLoading => _isLoading;

  /// Reads the signed-in account's record and saves it for next time.
  late final Command<void, void> fetch;

  /// Adopts the account already signed in, then follows the active one.
  ///
  /// Returns `this` so the locator can register it with the `init()` pattern:
  /// the subscription has to exist before the app starts, or the first account
  /// switch would go unnoticed.
  Future<MyProfileManager> init() async {
    _subscription = _user.currentUserId.listen(_onAccountChanged);
    _adopt(_user.currentUserId.value);
    unawaited(_read());
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
    _disposed = true;
    _subscription?.cancel();
    fetch.dispose();
    _profile.dispose();
    _isLoading.dispose();
  }

  /// Loads the record for whoever is signed in.
  ///
  /// This is the only door to the API: [fetch] runs it for the screens, and the
  /// background read runs it when an account becomes active. A read already in
  /// flight for the same account is left to finish rather than doubled, which
  /// is what makes opening the screen while it is still on its way free.
  Future<void> _load() async {
    final userId = _user.currentUserId.value;
    // Nobody is signed in: there is no record to read, and a request would be
    // answered with a 401.
    if (_disposed || userId == null || userId.isEmpty) return;
    if (_loadingUserId == userId) return;

    _loadingUserId = userId;
    _isLoading.value = true;
    try {
      final remote = await _http.me();

      // The account can change, or the manager be torn down, while the request
      // is in flight. The answer then belongs to a moment that has passed,
      // so it is dropped instead of shown under the wrong name.
      if (_disposed || _user.currentUserId.value != userId) return;

      _profile.value = remote;
      await _cache.saveMyProfile(remote);
    } finally {
      if (!_disposed && _loadingUserId == userId) {
        _loadingUserId = null;
        _isLoading.value = false;
      }
    }
  }

  /// Reads the record in the background.
  ///
  /// A failure here is logged rather than raised: the screen is already showing
  /// whatever was on disk, and nobody asked for this.
  Future<void> _read() async {
    try {
      await _load();
    } on MenoException catch (error) {
      _logger.w(
        'Could not read the profile (${error.code}); showing the cached copy',
        tag: MenoLogTag.lifecycle.label,
      );
    }
  }

  /// Follows the active account, putting its cached profile in place and then
  /// reading the account's own record.
  void _onAccountChanged(Id? userId, ListenableSubscription subscription) {
    if (_disposed) return;
    _adopt(userId);
    unawaited(_read());
  }

  /// Puts [userId]'s cached profile in place, or clears the screen when nobody
  /// is signed in.
  void _adopt(Id? userId) {
    if (userId == null || userId.isEmpty) {
      _profile.value = .empty;
      return;
    }
    _profile.value = _cache.myProfile(userId) ?? .empty;
  }
}
