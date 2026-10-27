import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/exceptions/meno_exception.dart';
import 'package:meno_mobile/_core/time/clock.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/_shared/model/_model.dart';
import 'package:meno_mobile/features/auth/model/credential_record.dart';
import 'package:meno_mobile/features/auth/model/entities.dart';
import 'package:meno_mobile/features/auth/model/session.dart';
import 'package:uuid/uuid.dart';

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

/// The receipt-time view of an [AuthResponseDto].
extension AuthResponseDtoDtoX on AuthResponseDto {
  /// This payload as a stored credential.
  ///
  /// This is the only place a relative lifetime becomes an instant: the
  /// server's `expires_in` is added to one reading of the clock, once. Nothing
  /// downstream may add a duration to "now" again, which is what keeps a
  /// three-day-old token from looking fresh.
  ///
  /// Throws [MalformedResponseException] when the payload cannot produce a
  /// usable record (an invalid account id, an empty access token, or a
  /// lifetime outside the accepted range). A fresh login starts a new epoch.
  CredentialRecord toCredentialRecord(Clock clock) {
    final accountId = Id.fromString(user.id);
    if (!accountId.isValid) {
      throw const MalformedResponseException(
        message: 'The server sent an account id the app cannot use.',
      );
    }

    final token = accessToken.trim();
    if (token.isEmpty) {
      throw const MalformedResponseException(
        message: 'The server sent an empty access token.',
      );
    }

    if (expiresIn <= 0 || expiresIn > _maxAccessTokenLifetimeSeconds) {
      throw const MalformedResponseException(
        message: 'The server sent an unusable access-token lifetime.',
      );
    }

    final at = clock.nowUtc();
    final refresh = refreshToken;
    return CredentialRecord(
      accountId: accountId,
      schemaVersion: CredentialRecord.schemaVersionNumber,
      epoch: const Uuid().v4(),
      revision: 1,
      accessToken: token,
      refreshToken: refresh == null || refresh.isEmpty ? null : refresh,
      accessExpiresAt: at.add(Duration(seconds: expiresIn)),
      receivedAt: at,
      isVerified: user.verified,
      renewalState: RenewalState.idle,
    );
  }

  /// The legacy domain view, used by the pre-vault storage path.
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
    id: Id.fromWire(id),
    deviceLabel: deviceLabel,
    createdAt: createdAt,
    lastUsedAt: lastUsedAt,
    rotations: rotations,
  );
}

/// A lifetime longer than this is not a session the app will honour.
///
/// The bound only rejects absurd values; it is deliberately generous so a
/// server-valid lifetime is never refused (H25).
const int _maxAccessTokenLifetimeSeconds = 30 * 24 * 60 * 60;
