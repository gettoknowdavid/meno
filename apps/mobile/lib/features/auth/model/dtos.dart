import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_shared/model/_model.dart';
import 'package:meno_mobile/features/auth/model/entities.dart';

part 'dtos.freezed.dart';
part 'dtos.g.dart';

@freezed
abstract class AuthResponseDto with _$AuthResponseDto {
  const factory({
    @JsonKey(name: 'access_token') required String accessToken,
    @JsonKey(name: 'expires_in') required int expiresIn,
    @JsonKey(name: 'refresh_token') required String? refreshToken,
    required UserDto user,
  }) = _AuthResponseDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$AuthResponseDtoFromJson(json);
}

@freezed
abstract class SessionInfoDto with _$SessionInfoDto {
  const factory({
    required String id,
    @JsonKey(name: 'device_label') required String deviceLabel,
    @JsonKey(name: 'created_at') DateTime? createdAt,
    @JsonKey(name: 'last_used_at') DateTime? lastUsedAt,
    int? rotations,
  }) = _SessionInfoDto;

  factory fromJson(Map<String, dynamic> json) => _$SessionInfoDtoFromJson(json);
}

/// The domain view of an [AuthResponseDto].
extension AuthResponseDtoDtoX on AuthResponseDto {
  /// This payload as an account the app can act as.
  UserCredential get toDomain => UserCredential(
    user: user.toDomain,
    session: Session(
      accessToken: accessToken,
      refreshToken: refreshToken,
      expiresAt: DateTime.now().add(Duration(seconds: expiresIn)),
    ),
  );
}

/// The wire view of a [UserCredential].
extension UserCredentialDtoX on UserCredential {
  /// This account as the API writes it, so the vault round-trips exactly.
  AuthResponseDto get toDto => AuthResponseDto(
    accessToken: session.accessToken,
    refreshToken: session.refreshToken,
    expiresIn: session.timeUntilExpiry?.inSeconds ?? 0,
    user: user.toDto,
  );
}

/// The domain view of a [SessionInfoDto].
extension SessionInfoDtoX on SessionInfoDto {
  /// This row as the app models it.
  SessionInfo get toDomain => SessionInfo(
    id: id,
    deviceLabel: deviceLabel,
    createdAt: createdAt,
    lastUsedAt: lastUsedAt,
    rotations: rotations,
  );
}
