import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/auth/model/auth_context.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/services/_services.dart';

/// The signed-in account's own profile, and the cache behind it.
///
/// The account it answers for is injected once, as an immutable
/// [AccountContext], rather than followed from a global selector: this manager
/// lives in that account's get_it scope, so a switch builds a *new* manager
/// and disposes this one — which is what stops a late answer from a previous
/// account from being shown (or cached) under the next one (M14).
///
/// An account's record is read as soon as that account becomes active, so the
/// screen opens on it instead of waiting for a request of its own. That read is
/// a background one — its failure is logged, not raised, because nobody asked
/// for it. [fetch] stays the door the screens use for a retry or a pull to
/// refresh, and it is where a failure reaches the user.
final class MyProfileManager implements Disposable {
  /// Creates a manager for the context's account over its service and cache.
  new({
    required this._context,
    required this._http,
    required this._cache,
    required this._logger,
  }) {
    fetch = .createAsyncNoParamNoResult(
      _load,
      errorFilterFn: menoExceptionFilter,
      debugName: 'myProfile.fetch',
    );
  }

  final AccountContext _context;
  final ProfileHttpService _http;
  final ProfileCacheService _cache;
  final MenoLogger _logger;

  final ValueNotifier<MyProfile> _profile = ValueNotifier(.empty);
  final ValueNotifier<bool> _isLoading = ValueNotifier(false);

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

  /// Installs the cached record for this account.
  ///
  /// The local half is synchronous, so the registrar can await it before the
  /// owner publishes `ready`; no remote read starts here — the first refresh
  /// begins only after the account is ready, from the account subtree's first
  /// widget (Step 18/19).
  Future<MyProfileManager> init() async {
    _adopt(_context.accountId);
    return this;
  }

  /// Starts the first background read after the account is ready.
  ///
  /// This is the door the account subtree opens once `ready`; it is the same
  /// read a retry or a pull-to-refresh asks for, and its failure is logged
  /// rather than raised because nobody asked for it.
  Future<void> refresh() => _read();

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
    final userId = _context.accountId;
    if (_disposed || userId.isEmpty) return;
    if (_loadingUserId == userId) return;

    _loadingUserId = userId;
    _isLoading.value = true;
    try {
      final remote = await _http.me();

      // The manager can be torn down while the request is in flight. The
      // answer then belongs to a moment that has passed, so it is dropped.
      if (_disposed) return;

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

  /// Puts this account's cached profile in place.
  void _adopt(Id userId) {
    di<MenoLogger>().w('Adopting profile for $userId');
    final something = _cache.myProfile() ?? .empty;
    di<MenoLogger>().w('Adopted profile from cache: $something');

    if (userId.isEmpty) {
      _profile.value = .empty;
      return;
    }
    _profile.value = _cache.myProfile() ?? .empty;
    di<MenoLogger>().w('Adopted profile: ${_profile.value}');
  }
}
