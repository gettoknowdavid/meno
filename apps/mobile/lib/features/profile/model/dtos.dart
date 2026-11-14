import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/_model.dart';
import 'package:meno_mobile/features/profile/model/entities.dart';

part 'dtos.freezed.dart';
part 'dtos.g.dart';

/// `POST /users/me/avatar` in Dart: where the uploaded avatar now lives.
@freezed
abstract class AvatarDto with _$AvatarDto {
  const factory({
    @JsonKey(name: 'avatar_id') required String avatarId,
    @JsonKey(name: 'avatar_url') String? avatarUrl,
  }) = _AvatarDto;

  factory fromJson(Map<String, dynamic> json) => _$AvatarDtoFromJson(json);
}

/// `GET /users/me` and `PATCH /users/me` in Dart.
///
/// Field names are the wire's — snake_case, as `MeResponse` declares them in
/// `apps/api/src/modules/profile/dto.rs`. Dates arrive as RFC 3339 strings,
/// which is why the API pins `time`'s serialisation.
///
/// The `settings` block the API embeds is deliberately not modelled: the
/// settings screens own that shape, and a copy of it here would be a second
/// definition to keep in step. Unknown keys are ignored when decoding.
@freezed
abstract class MeDto with _$MeDto {
  const factory({
    required String id,
    @JsonKey(name: 'full_name') required String fullName,
    required String email,
    @JsonKey(name: 'email_verified') @Default(false) bool emailVerified,
    String? bio,
    @JsonKey(name: 'avatar_id') String? avatarId,
    @JsonKey(name: 'avatar_url') String? avatarUrl,
    @Default(<String>[]) List<String> providers,
    @Default(0) int followers,
    @Default(0) int following,
    @Default(0) int broadcasts,
    @JsonKey(name: 'created_at') DateTime? createdAt,
  }) = _MeDto;

  factory fromJson(Map<String, dynamic> json) => _$MeDtoFromJson(json);
}

/// `GET /users/{id}` in Dart.
///
/// The public shape: no email, no providers, no settings. `is_following` is
/// about the *viewer*, not the profile's owner — the API resolves it against
/// whoever is asking.
@freezed
abstract class ProfileDto with _$ProfileDto {
  const factory({
    required String id,
    @JsonKey(name: 'full_name') required String fullName,
    String? bio,
    @JsonKey(name: 'avatar_url') String? avatarUrl,
    @JsonKey(name: 'is_following') @Default(false) bool isFollowing,
    @Default(0) int followers,
    @Default(0) int following,
    @Default(0) int broadcasts,
    @JsonKey(name: 'created_at') DateTime? createdAt,
  }) = _ProfileDto;

  factory fromJson(Map<String, dynamic> json) => _$ProfileDtoFromJson(json);
}

/// The domain view of an [AvatarDto].
extension AvatarDtoX on AvatarDto {
  /// Where the uploaded avatar now lives.
  AvatarUpload get toDomain => AvatarUpload(id: avatarId, url: avatarUrl);
}

/// The domain view of a [MeDto].
extension MeDtoX on MeDto {
  MyProfile get toDomain => MyProfile(
    id: Id.fromWire(id),
    fullName: FullName(fullName),
    email: Email(email),
    emailVerified: emailVerified,
    providers: [for (final wire in providers) ?AuthProvider.fromWire(wire)],
    bio: bio != null ? MultiLineString(bio!) : null,
    avatarId: avatarId,
    avatarUrl: ImageInput.fromUrl(avatarUrl),
    stats: ProfileStats(
      broadcasts: broadcasts,
      followers: followers,
      following: following,
    ),
    createdAt: createdAt,
  );
}

/// The wire view of a [MyProfile].
extension MyProfileDtoX on MyProfile {
  MeDto get toDto => MeDto(
    id: id.getOrCrash,
    fullName: fullName.getOrCrash,
    email: email.getOrCrash,
    emailVerified: emailVerified,
    providers: [for (final provider in providers) provider.wire],
    bio: bio?.getOrNull,
    avatarId: avatarId,
    avatarUrl: avatarUrl?.toUrlOrFilePathOrNull,
    followers: stats.followers,
    following: stats.following,
    broadcasts: stats.broadcasts,
    createdAt: createdAt,
  );
}

/// The domain view of a [ProfileDto].
extension ProfileDtoX on ProfileDto {
  /// This profile as the app models another account.
  Profile get toDomain => Profile(
    id: Id.fromWire(id),
    fullName: SingleLineString(fullName),
    bio: bio != null ? MultiLineString(bio!) : null,
    avatarUrl: ImageInput.fromUrl(avatarUrl),
    isFollowing: isFollowing,
    stats: ProfileStats(
      broadcasts: broadcasts,
      followers: followers,
      following: following,
    ),
    createdAt: createdAt,
  );
}

/// The wire view of a [Profile].
extension ProfileEntityDtoX on Profile {
  /// This profile as the API writes it, for the cache.
  ProfileDto get toDto => ProfileDto(
    id: id.getOrCrash,
    fullName: fullName.orEmpty,
    bio: bio?.getOrNull,
    avatarUrl: avatarUrl?.toUrlOrFilePathOrNull,
    isFollowing: isFollowing,
    followers: stats.followers,
    following: stats.following,
    broadcasts: stats.broadcasts,
    createdAt: createdAt,
  );
}
