import 'package:meno_mobile/_shared/model/entities/user.dart';

/// The `user` object the API's auth endpoints return.
///
/// Field names are the wire's — snake_case, as `UserResponse` declares them in
/// `apps/api/src/modules/auth/dto.rs`. Dates arrive as RFC 3339 strings; the
/// API is explicit about that because `time`'s own serialisation is a nine
/// number tuple no client can read.
final class UserDto {
  /// Creates a DTO.
  const new({
    required this.id,
    required this.fullName,
    required this.email,
    this.bio,
    this.verified = false,
    this.avatarId,
    this.avatarUrl,
    this.providers = const [],
    this.role = 'user',
    this.createdAt,
  });

  /// Reads [json] as the API's user object.
  ///
  /// Throws [FormatException] when a required field is missing or mistyped,
  /// which the HTTP layer reports as a malformed response rather than a UI
  /// error.
  factory fromJson(Map<String, dynamic> json) => UserDto(
    id: _string(json, 'id'),
    fullName: _string(json, 'full_name'),
    email: _string(json, 'email'),
    bio: json['bio'] as String?,
    verified: json['verified'] as bool? ?? false,
    avatarId: json['avatar_id'] as String?,
    avatarUrl: json['avatar_url'] as String?,
    providers: [
      for (final provider in json['providers'] as List<dynamic>? ?? const [])
        provider as String,
    ],
    role: json['role'] as String? ?? 'user',
    createdAt: switch (json['created_at']) {
      final String raw => DateTime.tryParse(raw),
      _ => null,
    },
  );

  /// The account id.
  final String id;

  /// The display name.
  final String fullName;

  /// The login address.
  final String email;

  /// The biography, when the account has one.
  final String? bio;

  /// Whether the address has been confirmed.
  final bool verified;

  /// The avatar's storage key.
  final String? avatarId;

  /// The resolved avatar URL.
  final String? avatarUrl;

  /// The providers the account can sign in with.
  final List<String> providers;

  /// The account's role.
  final String role;

  /// When the account was created, as RFC 3339.
  final DateTime? createdAt;

  /// Writes this DTO back as the wire object.
  Map<String, dynamic> toJson() => {
    'id': id,
    'full_name': fullName,
    'email': email,
    'bio': bio,
    'verified': verified,
    'avatar_id': avatarId,
    'avatar_url': avatarUrl,
    'providers': providers,
    'role': role,
    'created_at': createdAt?.toUtc().toIso8601String(),
  };

  /// Reads a required string field.
  static String _string(Map<String, dynamic> json, String field) {
    final value = json[field];
    if (value is String) return value;
    throw FormatException('The user object has no "$field".', json);
  }
}

/// The domain view of a [UserDto].
extension UserDtoX on UserDto {
  /// This user as the app models it.
  User get toDomain => User(
    id: id,
    fullName: fullName,
    email: email,
    bio: bio,
    verified: verified,
    avatarUrl: avatarUrl,
    providers: [
      for (final wire in providers)
        if (AuthProvider.fromWire(wire) case final provider?) provider,
    ],
    role: UserRole.fromWire(role),
    createdAt: createdAt,
  );
}

/// The wire view of a [User].
extension UserX on User {
  /// This user as the API writes it.
  UserDto get toDto => UserDto(
    id: id,
    fullName: fullName,
    email: email,
    bio: bio,
    verified: verified,
    avatarUrl: avatarUrl,
    providers: [for (final provider in providers) provider.wire],
    role: role.wire,
    createdAt: createdAt,
  );
}
