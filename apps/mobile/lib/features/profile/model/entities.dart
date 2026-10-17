import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/_model.dart';

part 'entities.freezed.dart';

/// Sample public profiles for cards, lists, and placeholder states.
final List<Profile> fakeProfileList = [
  Profile(
    id: Id.fromString('b1b1b1b1-0001-4b1b-8b1b-1b1b1b1b1b1b'),
    fullName: SingleLineString('Grace Okonkwo'),
    bio: MultiLineString('Sharing moments of worship, prayer, and testimony.'),
    isFollowing: true,
    stats: const ProfileStats(broadcasts: 5, followers: 120, following: 34),
    createdAt: DateTime(2023, 9, 20),
  ),
  Profile(
    id: Id.fromString('b1b1b1b1-0002-4b1b-8b1b-2b1b1b1b1b1b'),
    fullName: SingleLineString('Daniel Cho'),
    bio: MultiLineString('Teacher, speaker, and lifelong learner.'),
    stats: const ProfileStats(broadcasts: 1, followers: 8, following: 2),
  ),
  Profile(
    id: Id.fromString('b1b1b1b1-0003-4b1b-8b1b-3b1b1b1b1b1b'),
    fullName: SingleLineString('Sofia Bianchi'),
    stats: const ProfileStats(broadcasts: 10, followers: 90, following: 3),
    createdAt: DateTime(2026, 1, 15),
  ),
  Profile(
    id: Id.fromString('b1b1b1b1-0004-4b1b-8b1b-4b1b1b1b1b1b'),
    fullName: SingleLineString('Aisha Rahman'),
    isFollowing: true,
  ),
];

List<Profile> fakeProfiles = <Profile>[];

/// Where an uploaded avatar now lives.
///
/// `POST /users/me/avatar` answers with the two keys the account's row was
/// pointed at, and nothing else — there is no pre-signed URL and no expiry,
/// because nothing was pre-signed.
@freezed
abstract class AvatarUpload with _$AvatarUpload {
  /// Creates an upload's answer.
  const factory({
    /// The storage key now held by the account.
    required String id,

    /// The CDN URL for it, when the deployment publishes one.
    String? url,
  }) = _AvatarUpload;
}

/// The signed-in account's own profile.
///
/// Everything `GET /users/me` answers with, minus the settings block: the
/// settings screens own that, and a second copy of it here could disagree with
/// theirs. `providers` is present because the editor is where an account sees
/// how it can sign in.
@freezed
abstract class MyProfile with _$MyProfile {
  /// Creates the signed-in account's profile.
  const factory({
    /// The account id, as the API's UUID string.
    required Id id,

    /// The name shown next to the account.
    required FullName fullName,

    /// The address the account signs in with.
    required Email email,

    /// Whether that address has been confirmed.
    @Default(false) bool emailVerified,

    /// The identity providers linked to the account.
    @Default(<AuthProvider>[]) List<AuthProvider> providers,

    /// The account's biography, when it has one.
    MultiLineString? bio,

    /// The storage key of the current avatar, when one was uploaded.
    String? avatarId,

    /// The resolved avatar URL, when one was uploaded and the deployment
    /// publishes a base URL for object storage.
    ImageInput? avatarUrl,

    /// The account's counters.
    @Default(ProfileStats.zero) ProfileStats stats,

    /// When the account was created.
    DateTime? createdAt,
  }) = _MyProfile;

  const new _();

  /// A profile with nobody behind it: the state before a sign-in.
  static const MyProfile empty = MyProfile(
    id: .empty,
    fullName: .empty,
    email: .empty,
  );

  /// This account as everybody else sees it.
  ///
  /// What the share button and the editor hand to code that only deals in
  /// public profiles.
  Profile get asProfile => Profile(
    id: id,
    fullName: SingleLineString(fullName.orEmpty),
    bio: bio,
    avatarUrl: avatarUrl,
    stats: stats,
    createdAt: createdAt,
  );

  /// Whether this is [empty] rather than a real profile.
  bool get isEmpty => id.isEmpty;
}

/// Another account, as it looks to whoever is signed in.
///
/// The public half of a profile: no email, no providers and no settings, which
/// is exactly what `GET /users/{id}` answers with.
@freezed
abstract class Profile with _$Profile {
  /// Creates a public profile.
  const factory({
    /// The account id, as the API's UUID string.
    required Id id,

    /// The name shown next to the account.
    required SingleLineString fullName,

    /// The account's biography, when it has one.
    MultiLineString? bio,

    /// The resolved avatar URL, when the account has an avatar.
    ImageInput? avatarUrl,

    /// Whether the signed-in account follows this one.
    @Default(false) bool isFollowing,

    /// The account's counters.
    @Default(ProfileStats.zero) ProfileStats stats,

    /// When the account was created.
    DateTime? createdAt,
  }) = _Profile;

  const new _();

  /// A profile with no account behind it: the state before anything is known.
  static const Profile empty = Profile(id: .empty, fullName: .empty);

  /// Whether this is [empty] rather than a real profile.
  bool get isEmpty => id.isEmpty;
}

/// The three counters a profile shows.
///
/// The fields keep the API's names (`MeResponse` and `PublicProfileResponse` in
/// `apps/api/src/modules/profile/dto.rs`): a *follower* is what the screens call
/// a subscriber, and the following count is what they call subscriptions. That
/// renaming is a copy decision and lives with the layout, so the model never
/// disagrees with the wire about what a number means.
@freezed
abstract class ProfileStats with _$ProfileStats {
  /// Creates a set of counters.
  const factory({
    /// How many broadcasts the account has made.
    @Default(0) int broadcasts,

    /// How many accounts follow it.
    @Default(0) int followers,

    /// How many accounts it follows.
    @Default(0) int following,
  }) = _ProfileStats;

  const new _();

  /// The counters of a profile nobody has counted yet.
  static const ProfileStats zero = ProfileStats();
}
