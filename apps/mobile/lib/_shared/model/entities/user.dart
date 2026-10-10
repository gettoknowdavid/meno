//
// ignore_for_file: avoid_equals_and_hash_code_on_mutable_classes

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

/// The account the API's auth endpoints return.
///
/// Deliberately not the whole `users` row: `avatar_url` arrives resolved so the
/// app never has to build a storage URL, and the follower counts and search
/// vector are absent because they belong to the profile endpoints.
final class User {
  /// Creates a user.
  const new({
    required this.id,
    required this.fullName,
    required this.email,
    this.bio,
    this.verified = false,
    this.avatarUrl,
    this.providers = const [],
    this.role = UserRole.user,
    this.createdAt,
  });

  /// The account id, as the API's UUID string.
  final String id;

  /// The name shown next to the account.
  final String fullName;

  /// The address the account signs in with.
  final String email;

  /// The account's biography, when it has one.
  final String? bio;

  /// Whether the address has been confirmed with a code.
  final bool verified;

  /// The resolved avatar URL, when the account has an avatar.
  final String? avatarUrl;

  /// The providers this account can sign in with.
  final List<AuthProvider> providers;

  /// What the account may do.
  final UserRole role;

  /// When the account was created.
  final DateTime? createdAt;

  /// A user with no fields set: nobody is signed in.
  static const User empty = User(id: '', fullName: '', email: '');

  /// Whether this is [empty] rather than a real account.
  bool get isEmpty => id.isEmpty;

  /// The first name, for greetings.
  String get firstName {
    final parts = fullName.trim().split(RegExp(r'\s+'));
    return parts.isEmpty ? fullName : parts.first;
  }

  /// Whether the account can sign in with [provider].
  bool signsInWith(AuthProvider provider) => providers.contains(provider);

  /// A copy with the named fields replaced.
  ///
  /// Only the fields the app learns later are overridable — a verified flag
  /// flipped by `/auth/verify-email`, an avatar uploaded elsewhere.
  User copyWith({bool? verified, String? bio, String? avatarUrl}) => User(
    id: id,
    fullName: fullName,
    email: email,
    bio: bio ?? this.bio,
    verified: verified ?? this.verified,
    avatarUrl: avatarUrl ?? this.avatarUrl,
    providers: providers,
    role: role,
    createdAt: createdAt,
  );

  @override
  bool operator ==(Object other) =>
      other is User &&
      other.id == id &&
      other.fullName == fullName &&
      other.email == email &&
      other.verified == verified &&
      other.avatarUrl == avatarUrl &&
      other.role == role;

  @override
  int get hashCode =>
      Object.hash(id, fullName, email, verified, avatarUrl, role);
}
