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
/// # A stable subject, not the fetched id
///
/// The account this proxy is about is fixed at construction ([subjectId]) and
/// never changes. The empty placeholder every cold profile used to share is
/// gone: two profiles loading at once used to be one object whose identity was
/// rewritten by whichever answer arrived first, so a release looked up an id
/// nobody had acquired (audit H13). Nothing can retarget this proxy either — an
/// answer for a different account is refused rather than adopted.
///
/// # Overrides, not `copyWith`
///
/// The follow state is an override rather than a rewritten entity. Writing
/// `copyWith(isFollowing: true)` into the cached value would make a lie
/// indistinguishable from an answer — nothing downstream could tell the
/// optimistic flag from a server one, and a failed request would leave the
/// fabrication behind. `isFollowing` reads `override ?? entity`, and a fresh
/// target clears the override, so the API always wins in the end.
///
/// # One proxy per account
///
/// Get one from `UserProfileRepository`, never construct one directly: the same
/// account is drawn in a profile page, a broadcast card and a room header at
/// the same time, and they must agree about whether the viewer follows them.
final class UserProfileProxy extends ChangeNotifier {
  /// A proxy for [subjectId], loading until a target arrives.
  ///
  /// The screens draw a skeleton while [targetOrNull] is null; the id is
  /// already known, so a feed or a request can be started from it. [profile]
  /// fills it at construction for the places that were handed a profile as part
  /// of something else — a participant card — and never fetch one of their own.
  new(this.subjectId, {Profile? profile}) : _target = profile {
    if (profile != null && profile.id != subjectId) {
      throw MalformedResponseException(
        message:
            'A proxy for $subjectId cannot hold the profile of '
            '${profile.id.getOrNull}.',
      );
    }
    _createCommands();
  }

  /// The account this proxy is about. Fixed for its whole life.
  final Id subjectId;

  Profile? _target;

  /// The viewer's follow, while it is ahead of the API's answer.
  ///
  /// Null whenever the entity's own field is the truth, which is the state the
  /// proxy starts in and returns to on the next [target].
  bool? _isFollowingOverride;

  int _referenceCount = 0;

  /// The entity, or a placeholder naming only the subject while it loads.
  Profile get target =>
      _target ?? Profile(id: subjectId, fullName: SingleLineString.empty);

  /// The entity, or null while nothing has been loaded.
  Profile? get targetOrNull => _target;

  /// Adopts [value] as the account's profile, dropping any override.
  ///
  /// Fresh data supersedes a guess, so this is where the optimistic follow is
  /// settled — whether the request that produced it succeeded or not. A value
  /// for another account is refused: applying it would make this proxy lie
  /// about whose profile it holds.
  set target(Profile value) {
    if (value.id.isValid && value.id != subjectId) {
      throw MalformedResponseException(
        message:
            'Asked for the profile of $subjectId but the API answered for '
            '${value.id.getOrNull}.',
      );
    }

    _target = value;
    _isFollowingOverride = null;
    notifyListeners();
  }

  /// The account's id — the one it was acquired by, not one an answer named.
  Id get id => subjectId;

  /// The name to draw, empty until something is known.
  String get displayName => _target?.fullName.orEmpty ?? '';

  SingleLineString get fullName => _target?.fullName ?? SingleLineString.empty;

  MultiLineString? get bio => _target?.bio;

  ImageInput? get avatarUrl => _target?.avatarUrl;

  DateTime? get createdAt => _target?.createdAt;

  /// Whether there is no account behind this proxy yet.
  bool get isEmpty => _target == null || _target!.isEmpty;

  /// Whether the viewer follows this account.
  ///
  /// The override while a change is in flight, the API's field otherwise.
  bool get isFollowing =>
      _isFollowingOverride ?? (_target?.isFollowing ?? false);

  /// The counters, with the viewer's own follow already counted.
  ///
  /// A follower count that only moved after the round trip would make the
  /// optimistic state look half-applied, so the delta travels with the
  /// override: plus one while following, minus one while not.
  ProfileStats get stats {
    final target = _target;
    if (target == null) return ProfileStats.zero;
    if (_isFollowingOverride == null ||
        _isFollowingOverride == target.isFollowing) {
      return target.stats;
    }

    final delta = _isFollowingOverride! ? 1 : -1;
    return target.stats.copyWith(
      followers: (target.stats.followers + delta).clamp(0, 1 << 31),
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

  void _createCommands() {
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
      errorFilter: null,
      errorFilterFn: menoExceptionFilter,
      debugName: 'userProfile.toggleFollow',
    );
  }

  /// Moves the override, clearing it when it agrees with the entity again.
  void _setFollowing(bool value) {
    _isFollowingOverride = value == (_target?.isFollowing ?? false)
        ? null
        : value;
    notifyListeners();
  }

  @override
  void dispose() {
    toggleFollow.dispose();
    super.dispose();
  }
}
