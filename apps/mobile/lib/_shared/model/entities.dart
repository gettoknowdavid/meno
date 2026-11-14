import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';

part 'entities.freezed.dart';

/// Sample accounts for skeletonizers, empty-state tests, and previews.
final List<User> fakeUserList = [
  User(
    id: Id.fromString('a1a1a1a1-0001-4a1a-8a1a-1a1a1a1a1a1a'),
    fullName: FullName('Elena Vasquez'),
    email: Email('elena@example.com'),
    verified: true,
    bio: MultiLineString('Building things for the web and mobile.'),
    providers: [.password, .google],
    role: .creator,
  ),
  User(
    id: Id.fromString('a1a1a1a1-0002-4a1a-8a1a-2a1a1a1a1a1a'),
    fullName: FullName('Marcus Reed'),
    email: Email('marcus@example.com'),
    verified: true,
    providers: [.google],
    role: .admin,
  ),
  User(
    id: Id.fromString('a1a1a1a1-0003-4a1a-8a1a-3a1a1a1a1a1a'),
    fullName: FullName('Priya Nair'),
    email: Email('priya@example.com'),
    bio: MultiLineString('Early adopter, always learning.'),
  ),
  User(
    id: Id.fromString('a1a1a1a1-0004-4a1a-8a1a-4a1a1a1a1a1a'),
    fullName: FullName('Jordan Smith'),
    email: Email('jordan@example.com'),
  ),
];

/// How an account can sign in.
///
/// Mirrors `AuthProvider` in `apps/api/src/middleware/auth.rs`. The enum is
/// `#[non_exhaustive]` on the server, so [fromWire] returns null for a value
/// this build has never heard of rather than throwing — an account created with
/// a provider the app does not support yet can still be listed.
enum AuthProvider {
  /// Email and password.
  password('password'),

  /// Google, via OAuth2/OIDC.
  google('google');

  new(this.wire);

  /// The exact string the API sends.
  final String wire;

  /// The provider [wire] names, or null when it is one this build omits.
  static AuthProvider? fromWire(String wire) {
    for (final provider in AuthProvider.values) {
      if (provider.wire == wire) return provider;
    }
    return null;
  }
}

/// The account the API's auth endpoints return.
///
/// Deliberately not the whole `users` row: `avatar_url` arrives resolved so the
/// app never has to build a storage URL, and the follower counts and search
/// vector are absent because they belong to the profile endpoints.
@freezed
abstract class User with _$User {
  /// Creates a user.
  const factory({
    /// The account id, as the API's UUID string.
    required Id id,

    /// The name shown next to the account.
    required FullName fullName,

    /// The address the account signs in with.
    required Email email,

    /// The account's biography, when it has one.
    MultiLineString? bio,

    /// Whether the address has been confirmed with a code.
    @Default(false) bool verified,

    /// The resolved avatar URL, when the account has an avatar.
    ImageInput? image,

    /// The providers this account can sign in with.
    @Default(<AuthProvider>[]) List<AuthProvider> providers,

    /// What the account may do.
    @Default(UserRole.user) UserRole role,

    /// When the account was created.
    DateTime? createdAt,
  }) = _User;

  const new _();

  /// A user with no fields set: nobody is signed in.
  static const User empty = User(
    id: Id.empty,
    fullName: FullName.empty,
    email: Email.empty,
    bio: MultiLineString.empty,
  );

  /// The first name, for greetings.
  String get firstName {
    final name = fullName.orEmpty.trim();
    return name.split(RegExp(r'\s+')).first;
  }

  /// Whether this is [empty] rather than a real account.
  bool get isEmpty => id.isEmpty;

  /// Whether the account can sign in with [provider].
  bool signsInWith(AuthProvider provider) => providers.contains(provider);
}

/// What an account may do.
///
/// Mirrors `UserRole` in `apps/api/src/middleware/auth.rs`, including its
/// non-exhaustiveness: an unrecognised role reads as [user], the least
/// privileged answer.
enum UserRole {
  /// An ordinary signed-in user.
  user('user'),

  /// A creator, who owns broadcasts.
  creator('creator'),

  /// An administrator.
  admin('admin');

  new(this.wire);

  /// The exact string the API sends.
  final String wire;

  /// The role [wire] names, defaulting to [user].
  static UserRole fromWire(String wire) {
    for (final role in UserRole.values) {
      if (role.wire == wire) return role;
    }
    return UserRole.user;
  }
}
