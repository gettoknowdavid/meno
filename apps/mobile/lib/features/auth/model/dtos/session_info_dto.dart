import 'package:meno_mobile/features/auth/model/entities/session_info.dart';

/// One row of `GET /auth/sessions`, snake_case as `SessionResponse` declares it
/// in `apps/api/src/modules/auth/dto.rs`.
final class SessionInfoDto {
  /// Creates a DTO.
  const new({
    required this.id,
    required this.deviceLabel,
    this.createdAt,
    this.lastUsedAt,
    this.rotations = 0,
  });

  /// Reads [json] as a session row.
  factory SessionInfoDto.fromJson(Map<String, dynamic> json) => SessionInfoDto(
    id: json['id'] as String? ?? '',
    deviceLabel: json['device_label'] as String? ?? '',
    createdAt: _date(json['created_at']),
    lastUsedAt: _date(json['last_used_at']),
    rotations: json['rotations'] as int? ?? 0,
  );

  /// The session id.
  final String id;

  /// Something a person recognises.
  final String deviceLabel;

  /// When the session was created, as RFC 3339.
  final DateTime? createdAt;

  /// When it was last used to refresh, as RFC 3339.
  final DateTime? lastUsedAt;

  /// How many times the refresh chain has been rotated.
  final int rotations;

  /// Reads an RFC 3339 field, tolerating its absence.
  static DateTime? _date(Object? value) =>
      value is String ? DateTime.tryParse(value) : null;
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
