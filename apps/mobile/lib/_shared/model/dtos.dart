import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/entities.dart';

part 'dtos.freezed.dart';
part 'dtos.g.dart';

/// The `user` object the API's auth endpoints return.
///
/// Field names are the wire's — snake_case, as `UserResponse` declares them in
/// `apps/api/src/modules/auth/dto.rs`. Dates arrive as RFC 3339 strings; the
/// API is explicit about that because `time`'s own serialisation is a nine
/// number tuple no client can read.
@freezed
abstract class UserDto with _$UserDto {
  const factory({
    required String id,
    @JsonKey(name: 'full_name') required String fullName,
    required String email,
    String? bio,
    @Default(false) bool verified,
    @JsonKey(name: 'avatar_id') String? avatarId,
    @JsonKey(name: 'avatar_url') String? avatarUrl,
    @Default([]) List<String> providers,
    @Default('user') String role,
    @JsonKey(name: 'created_at') DateTime? createdAt,
  }) = _UserDto;

  factory fromJson(Map<String, Object?> json) => _$UserDtoFromJson(json);
}

/// The domain view of a [UserDto].
extension UserDtoX on UserDto {
  /// This user as the app models it.
  ///
  /// The wire's own strings are taken as server truth: the form policy lives
  /// on the registration form, never on this mapping, so a server-valid name
  /// or address cannot be rejected while it is being stored.
  User get toDomain => User(
    id: Id.fromWire(id),
    email: Email.fromWire(email),
    fullName: FullName.fromWire(fullName),
    bio: bio == null ? null : MultiLineString(bio!),
    verified: verified,
    image: avatarUrl != null ? ImageInput.fromUrl(avatarUrl) : null,
    providers: [for (final wire in providers) ?AuthProvider.fromWire(wire)],
    role: UserRole.fromWire(role),
    createdAt: createdAt,
  );
}

/// The wire view of a [User].
extension UserX on User {
  /// This user as the API writes it.
  ///
  /// Serialisation never throws: a value object failing its form policy
  /// degrades to an empty string rather than crashing the write, so
  /// invalidity is never *discovered* here.
  UserDto get toDto => UserDto(
    id: id.orEmpty,
    fullName: fullName.getOrElse((_) => ''),
    email: email.getOrElse((_) => ''),
    bio: bio?.getOrNull,
    verified: verified,
    avatarUrl: image?.toUrlOrFilePathOrNull,
    providers: [for (final provider in providers) provider.wire],
    role: role.wire,
    createdAt: createdAt,
  );
}
