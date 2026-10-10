import 'package:meno_mobile/_shared/model/dtos/user_dto.dart';
import 'package:meno_mobile/_shared/model/value_objects/session.dart';
import 'package:meno_mobile/features/auth/model/entities/user_credential.dart';

/// The body of a successful register, login, refresh or Google sign-in.
///
/// `AuthResponse` in `apps/api/src/modules/auth/dto.rs`, snake_case as declared
/// there. The pair is always returned together — registering, signing in,
/// refreshing and verifying an address all hand back a usable session.
final class AuthResponseDto {
  /// Creates a DTO.
  const new({
    required this.accessToken,
    required this.user,
    this.refreshToken,
    this.expiresIn = 0,
  });

  /// Reads [json] as the API's token payload.
  factory AuthResponseDto.fromJson(Map<String, dynamic> json) {
    final user = json['user'];
    if (user is! Map<String, dynamic>) {
      throw FormatException('The auth response has no user object.', json);
    }

    return AuthResponseDto(
      accessToken: json['access_token'] as String? ?? '',
      refreshToken: json['refresh_token'] as String?,
      expiresIn: json['expires_in'] as int? ?? 0,
      user: UserDto.fromJson(user),
    );
  }

  /// The signed access token.
  final String accessToken;

  /// The signed refresh token, rotated on every refresh.
  final String? refreshToken;

  /// Seconds until [accessToken] expires.
  final int expiresIn;

  /// The signed-in account.
  final UserDto user;

  /// Writes this payload back as the wire object.
  Map<String, dynamic> toJson() => {
    'access_token': accessToken,
    'refresh_token': refreshToken,
    'expires_in': expiresIn,
    'user': user.toJson(),
  };
}

/// The domain view of an [AuthResponseDto].
extension AuthResponseDtoX on AuthResponseDto {
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
