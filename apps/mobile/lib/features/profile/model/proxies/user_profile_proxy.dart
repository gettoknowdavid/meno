import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/profile/model/entities.dart';
import 'package:meno_mobile/features/profile/services/profile_http_service.dart';

/// One account's public profile, with the viewer's own standing layered on top.
///
/// # Why a proxy and not the entity
///
/// [Profile] is a value: it is what the API answered with, and it cannot say
/// anything the API did not. Half of what a profile screen draws is not that —
/// whether *the viewer* follows this account changes the moment they tap, and
/// the request behind it takes a round trip. A proxy adds exactly that layer:
/// it holds the entity, exposes the parts screens read, and puts an override on
/// top for the fields the user has moved ahead of the server.
///
/// # Overrides, not `copyWith`
///
/// The follow state is an override rather than a rewritten entity. Writing
/// `copyWith(isFollowing: true)` into the cached value would make a lie
/// indistinguishable from an answer — nothing downstream could tell the
/// optimistic flag from a server one, and a failed request would leave the
/// fabrication behind. `isFollowing` reads `override ?? entity`, and a fresh
/// [target] clears the override, so the API always wins in the end.
///
/// # One proxy per account
///
/// Get one from [UserProfileRepository], never construct one directly: the same
/// account is drawn in a profile page, a broadcast card and a room header at
/// the same time, and they must agree about whether the viewer follows them.
final class UserProfileProxy extends ChangeNotifier {
  /// Wraps [_profile] — an initial or a fresh copy.
  new(this._profile) {
    toggleFollow = .createUndoableNoParamNoResult<bool>(
      (stack) async {
        stack.push(isFollowing);

        _setFollowing(!isFollowing);
        final service = di<ProfileHttpService>();
        if (isFollowing) {
          await service.follow(id);
        } else {
          await service.unfollow(id);
        }
      },
      undo: (stack, _) => _setFollowing(stack.pop()),
      errorFilterFn: menoExceptionFilter,
      debugName: 'userProfile.toggleFollow',
    );
  }

  Profile _profile;

  /// The viewer's follow, while it is ahead of the API's answer.
  ///
  /// Null whenever the entity's own field is the truth, which is the state the
  /// proxy starts in and returns to on the next [target].
  bool? _isFollowingOverride;

  int _referenceCount = 0;

  /// The entity, for code that takes a plain [Profile].
  Profile get target => _profile;

  /// Adopts [value] as the account's profile, dropping any override.
  ///
  /// Fresh data supersedes a guess, so this is where the optimistic follow is
  /// settled — whether the request that produced it succeeded or not.
  set target(Profile value) {
    _profile = value;
    _isFollowingOverride = null;
    notifyListeners();
  }

  /// The account's id, or the empty string before anything is known.
  String get id => _profile.id;

  String get fullName => _profile.fullName;

  String? get bio => _profile.bio;

  String? get avatarUrl => _profile.avatarUrl;

  DateTime? get createdAt => _profile.createdAt;

  /// Whether there is no account behind this proxy yet.
  bool get isEmpty => _profile.isEmpty;

  /// Whether the viewer follows this account.
  ///
  /// The override while a change is in flight, the API's field otherwise.
  bool get isFollowing => _isFollowingOverride ?? _profile.isFollowing;

  /// The counters, with the viewer's own follow already counted.
  ///
  /// A follower count that only moved after the round trip would make the
  /// optimistic state look half-applied, so the delta travels with the
  /// override: plus one while following, minus one while not.
  ProfileStats get stats {
    if (_isFollowingOverride == null ||
        _isFollowingOverride == _profile.isFollowing) {
      return _profile.stats;
    }

    final delta = _isFollowingOverride! ? 1 : -1;
    return _profile.stats.copyWith(
      followers: (_profile.stats.followers + delta).clamp(0, 1 << 31),
    );
  }

  /// A second subscriber to this account's follow state.
  ///
  /// The count is the repository's, but it is kept here because it identifies
  /// the proxy: a holder releases the same instance it acquired.
  void retain() => _referenceCount++;

  /// Gives up one hold, answering whether this was the last one.
  bool release() {
    _referenceCount--;
    return _referenceCount <= 0;
  }

  /// Follows or stops following the account, ahead of the API.
  ///
  /// Undoable rather than error-listened: the button has exactly one sensible
  /// rollback — the state it was in — and the command's undo stack is that,
  /// applied when the request fails. A caller that wants to say something about
  /// the failure listens to this command's `errors`.
  late final Command<void, void> toggleFollow;

  /// Moves the override, clearing it when it agrees with the entity again.
  void _setFollowing(bool value) {
    _isFollowingOverride = value == _profile.isFollowing ? null : value;
    notifyListeners();
  }

  @override
  void dispose() {
    toggleFollow.dispose();
    super.dispose();
  }
}

/// The live proxies, one per account, for as long as somebody is watching.
///
/// # Why a repository rather than a manager each
///
/// An account's profile is drawn in several places at once — their profile
/// page, a broadcast card, the header of a room they are hosting — and each of
/// those wants the *same* answer about whether the viewer follows them. Giving
/// every screen its own proxy would give the app as many answers as it has
/// screens, and following someone on their page would leave the card behind it
/// stale.
///
/// # Reference counting
///
/// A proxy is created when the first holder acquires it and disposed when the
/// last one releases it; the holder is whatever brought it on screen, which in
/// practice is a page's manager. Counting is not decoration: without it, the
/// first page to close would dispose a proxy a second page is still drawing.
final class UserProfileRepository {
  final _proxies = <String, UserProfileProxy>{};

  /// The proxy for [profile]'s account, for one more holder.
  ///
  /// Returns the live proxy when there is one — refreshed with the newest data,
  /// which clears any optimistic override — and a new one otherwise. An
  /// account-less profile (the empty id a page shows before it knows which
  /// account it is about) shares one placeholder, which is harmless: nothing
  /// can be done to it.
  UserProfileProxy acquire(Profile profile) {
    final existing = _proxies[profile.id];
    if (existing != null) {
      // Only a *different* copy is adopted: re-acquiring with the cached value
      // an earlier visit stored would otherwise wipe a follow that is still in
      // flight.
      if (existing.target != profile) existing.target = profile;
      existing.retain();
      return existing;
    }

    final proxy = UserProfileProxy(profile)..retain();
    _proxies[profile.id] = proxy;
    return proxy;
  }

  /// Gives up one hold on [proxy], disposing it when it was the last.
  ///
  /// Tolerates a proxy this repository no longer owns, so a double release is a
  /// no-op rather than a second `dispose` on a disposed notifier.
  void release(UserProfileProxy proxy) {
    if (_proxies[proxy.id] != proxy) return;
    if (!proxy.release()) return;

    _proxies.remove(proxy.id);
    proxy.dispose();
  }

  /// How many accounts currently have a live proxy.
  ///
  /// For tests and diagnostics; the app never needs the number.
  int get length => _proxies.length;
}
